use std::{
    ops::{Deref, DerefMut},
    path::PathBuf,
    pin::Pin,
};

use async_bincode::tokio::{AsyncBincodeReader, AsyncBincodeWriter};
use async_compression::tokio::{bufread::ZstdDecoder, write::ZstdEncoder};

#[cfg(feature = "async-zstdmt")]
use async_compression::zstd::CParameter;

use async_trait::async_trait;
use derive_getters::Getters;
use futures::{SinkExt, StreamExt};
use itertools::Itertools;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::{
    fs::{copy, create_dir_all, remove_dir_all, remove_file, rename, File},
    io::{AsyncRead, AsyncSeek, AsyncWrite, AsyncWriteExt, BufReader},
    spawn,
    task::JoinSet,
};
use uuid::Uuid;

use crate::{
    array::async_impl::BackedArray,
    entry::async_impl::{AsyncReadDisk, AsyncWriteDisk},
    meta::async_impl::BackedArrayWrapper,
};

#[cfg(feature = "async-zstdmt")]
use super::ZSTD_MULTITHREAD;

/// File encoded with zstd
#[derive(Serialize, Deserialize)]
pub struct ZstdFile {
    path: PathBuf,
    zstd_level: i32,
}

impl ZstdFile {
    /// Create a new ZstdFile
    ///
    /// * `path`: A valid filesystem path
    /// * `zstd_level`: An optional level bound [0-22]. 0 for library default.
    pub async fn new(path: PathBuf, zstd_level: Option<i32>) -> std::io::Result<Self> {
        let zstd_level = zstd_level.unwrap_or(0);
        if !(0..=22).contains(&zstd_level) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("zstd_level ({zstd_level}) not [0, 22]"),
            ));
        };
        Ok(Self { path, zstd_level })
    }
}

pub struct ZstdDecoderWrapper(ZstdDecoder<BufReader<File>>);

impl AsyncRead for ZstdDecoderWrapper {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        Pin::new(&mut (self.get_mut()).0).poll_read(cx, buf)
    }
}

impl AsyncSeek for ZstdDecoderWrapper {
    fn poll_complete(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<u64>> {
        Pin::new(&mut (self.get_mut()).0.get_mut()).poll_complete(cx)
    }
    fn start_seek(
        self: std::pin::Pin<&mut Self>,
        position: std::io::SeekFrom,
    ) -> std::io::Result<()> {
        Pin::new(&mut (self.get_mut()).0.get_mut()).start_seek(position)
    }
}

impl AsyncReadDisk for ZstdFile {
    type ReadDisk = ZstdDecoderWrapper;

    async fn read_disk(&mut self) -> std::io::Result<Self::ReadDisk> {
        Ok(ZstdDecoderWrapper(ZstdDecoder::new(BufReader::new(
            File::open(self.path.clone()).await?,
        ))))
    }
}

impl AsyncWriteDisk for ZstdFile {
    type WriteDisk = ZstdEncoder<File>;

    async fn write_disk(&mut self) -> std::io::Result<Self::WriteDisk> {
        let file = File::options()
            .write(true)
            .create(true)
            .truncate(true)
            .open(self.path.clone())
            .await?;

        #[cfg(feature = "async-zstdmt")]
        {
            Ok(ZstdEncoder::with_quality_and_params(
                file,
                async_compression::Level::Precise(self.zstd_level),
                &[CParameter::nb_workers(*ZSTD_MULTITHREAD.lock().unwrap())],
            ))
        }

        #[cfg(not(feature = "async-zstdmt"))]
        {
            Ok(ZstdEncoder::with_quality(
                file,
                async_compression::Level::Precise(self.zstd_level),
            ))
        }
    }
}

/// [`BackedArray`] that uses a directory of zstd compressed files
#[derive(Serialize, Deserialize, Getters)]
pub struct ZstdDirBackedArray<T> {
    array: BackedArray<T, ZstdFile>,
    directory_root: PathBuf,
    zstd_level: Option<i32>,
}

#[async_trait]
impl<T: Serialize + DeserializeOwned + Send + Sync> BackedArrayWrapper<T>
    for ZstdDirBackedArray<T>
{
    type Storage = ZstdFile;
    type BackingError = std::io::Error;

    /// Wraps [`BackedArray::remove`] to delete the file
    async fn remove(&mut self, entry_idx: usize) -> Result<&mut Self, std::io::Error> {
        remove_file(self.get_disks()[entry_idx].path.clone()).await?;
        self.array.remove(entry_idx);
        Ok(self)
    }

    async fn append(&mut self, values: &[T]) -> bincode::Result<&mut Self> {
        self.array
            .append(
                values,
                ZstdFile::new(
                    self.directory_root
                        .clone()
                        .join(Uuid::new_v4().to_string() + ".zstd"),
                    self.zstd_level,
                )
                .await?,
            )
            .await?;
        Ok(self)
    }

    async fn append_memory(&mut self, values: Box<[T]>) -> bincode::Result<&mut Self> {
        self.array
            .append_memory(
                values,
                ZstdFile::new(
                    self.directory_root
                        .clone()
                        .join(Uuid::new_v4().to_string() + ".zstd"),
                    self.zstd_level,
                )
                .await?,
            )
            .await?;
        Ok(self)
    }

    async fn append_array(&mut self, rhs: Self) -> Result<&mut Self, Self::BackingError> {
        let mut copy_futures = JoinSet::new();

        let remove = self.directory_root != rhs.directory_root;

        let disks: Vec<PathBuf> = rhs
            .array
            .get_disks()
            .into_iter()
            .map(|x| x.path.clone())
            .collect_vec();
        disks.into_iter().for_each(|path| {
            let new_root_clone = self.directory_root.clone();
            copy_futures.spawn(async move {
                copy(path.clone(), new_root_clone.join(path.file_name().unwrap())).await
            });
        });

        if remove {
            remove_dir_all(rhs.directory_root).await?;
        }

        self.array.append_array(rhs.array);

        while let Some(future) = copy_futures.join_next().await {
            let _ = future?;
        }
        dir_remove.await??;

        Ok(self)
    }
}

impl<T> ZstdDirBackedArray<T> {
    /// Creates a new array backed by zstd compressed files in a directory
    ///
    /// * `directory_root`: base directory for every file
    /// * `zstd_level`: An optional level bound [0-22]. 0 for library default.
    pub async fn new(directory_root: PathBuf, zstd_level: Option<i32>) -> std::io::Result<Self> {
        create_dir_all(directory_root.clone()).await?;
        Ok(ZstdDirBackedArray {
            array: BackedArray::default(),
            directory_root,
            zstd_level,
        })
    }

    /// Sets a new zstd_level for all future arrays
    ///
    /// Does not impact already-compressed arrays
    pub fn set_level(&mut self, zstd_level: i32) {
        self.zstd_level = Some(zstd_level);
    }
}

impl<T> Deref for ZstdDirBackedArray<T> {
    type Target = BackedArray<T, ZstdFile>;

    fn deref(&self) -> &Self::Target {
        &self.array
    }
}

impl<T> DerefMut for ZstdDirBackedArray<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.array
    }
}

impl<T: Serialize> ZstdDirBackedArray<T> {
    /// Wraps [`BackedArray::save_to_disk`] to include its own metadata
    pub async fn save_to_disk<W: AsyncWrite + Unpin>(
        &mut self,
        writer: &mut W,
    ) -> bincode::Result<()> {
        self.clear_memory();
        let mut bincode_writer = AsyncBincodeWriter::from(writer).for_async();
        bincode_writer.send(&self).await?;
        bincode_writer.get_mut().flush().await?;
        Ok(())
    }
}

impl<T: DeserializeOwned> ZstdDirBackedArray<T> {
    /// Wraps [`BackedArray::load`] to include its own metadata
    pub async fn load<R: AsyncRead + Unpin>(writer: &mut R) -> bincode::Result<Self> {
        AsyncBincodeReader::from(writer)
            .next()
            .await
            .ok_or(bincode::ErrorKind::Custom(
                "AsyncBincodeReader stream empty".to_string(),
            ))?
    }
}

impl<T> ZstdDirBackedArray<T> {
    /// Updates the root of the zstd directory backed array.
    ///
    /// Does not move any files or directories, just changes pointers.
    pub fn update_root(&mut self, new_root: PathBuf) -> &mut Self {
        self.array.get_disks_mut().iter_mut().for_each(|disk| {
            disk.path = new_root.join(disk.path.file_name().unwrap());
        });
        self.directory_root = new_root;
        self
    }

    /// Moves the directory to a new location wholesale.
    pub async fn move_root(&mut self, new_root: PathBuf) -> std::io::Result<&mut Self> {
        let mut copy_futures = JoinSet::new();

        if rename(self.directory_root.clone(), new_root.clone())
            .await
            .is_err()
        {
            create_dir_all(new_root.clone()).await?;

            let disks: Vec<PathBuf> = self
                .array
                .get_disks()
                .into_iter()
                .map(|x| x.path.clone())
                .collect_vec();
            disks.into_iter().for_each(|path| {
                let new_root_clone = new_root.clone();
                copy_futures.spawn(async move {
                    copy(path.clone(), new_root_clone.join(path.file_name().unwrap())).await
                });
            });
        }

        self.update_root(new_root);
        while let Some(future) = copy_futures.join_next().await {
            let _ = future?;
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use std::env::temp_dir;

    use itertools::Itertools;
    use tokio::fs::remove_dir_all;

    use super::*;

    fn values() -> (Vec<String>, Vec<String>) {
        (
            ["TEST STRING".to_string()]
                .into_iter()
                .cycle()
                .take(10_000)
                .collect_vec(),
            ["OTHER VALUE".to_string()]
                .into_iter()
                .cycle()
                .take(10_000)
                .collect_vec(),
        )
    }

    #[tokio::test]
    async fn write() {
        let directory = temp_dir().join("zstd_directory_write_async");
        let _ = remove_dir_all(directory.clone()).await;
        let mut arr = ZstdDirBackedArray::new(directory.clone(), None)
            .await
            .unwrap();
        let (values, second_values) = values();

        arr.append_memory(values.into()).await.unwrap();
        arr.append(&second_values).await.unwrap();
        assert_eq!(arr.get(100).await.unwrap(), &"TEST STRING");
        assert_eq!(arr.get(200).await.unwrap(), &"TEST STRING");
        assert_eq!(arr.get(150).await.unwrap(), &"TEST STRING");
        assert_eq!(arr.get(15_000).await.unwrap(), &"OTHER VALUE");

        remove_dir_all(directory).await.unwrap();
    }

    #[tokio::test]
    async fn write_and_read() {
        let directory = temp_dir().join("zstd_directory_write_and_read_async");
        let _ = remove_dir_all(directory.clone()).await;
        let mut arr = ZstdDirBackedArray::new(directory.clone(), None)
            .await
            .unwrap();
        let (values, second_values) = values();

        arr.append(&values).await.unwrap();
        arr.append_memory(second_values.into()).await.unwrap();
        arr.save_to_disk(&mut File::create(directory.join("directory")).await.unwrap())
            .await
            .unwrap();
        drop(arr);

        let mut arr: ZstdDirBackedArray<String> =
            ZstdDirBackedArray::load(&mut File::open(directory.join("directory")).await.unwrap())
                .await
                .unwrap();
        assert_eq!(arr.get(100).await.unwrap(), &"TEST STRING");
        assert_eq!(arr.get(15_000).await.unwrap(), &"OTHER VALUE");
        assert_eq!(arr.get(200).await.unwrap(), &"TEST STRING");
        assert_eq!(arr.get(1).await.unwrap(), &"TEST STRING");

        remove_dir_all(directory).await.unwrap();
    }
}
