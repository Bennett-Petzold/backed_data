use std::{
    ops::{Deref, DerefMut},
    path::PathBuf,
    pin::Pin,
};

use async_trait::async_trait;
use futures::executor::block_on;

use itertools::Itertools;
use serde::{
    de::{DeserializeOwned, Error},
    Deserialize, Serialize,
};
use tokio::{
    fs::{copy, rename},
    io::{AsyncRead, AsyncWrite},
    task::JoinSet,
};
use tokio::{
    fs::{create_dir_all, remove_file, File},
    io::AsyncSeek,
};
use uuid::Uuid;

use crate::{array::async_impl::BackedArray, meta::async_impl::BackedArrayWrapper};

use super::PathBufVisitor;

/// File, but serializes based on path string
#[derive(Debug)]
pub struct SerialFile {
    pub file: File,
    pub path: PathBuf,
}

impl SerialFile {
    pub async fn new(path: PathBuf) -> Result<Self, tokio::io::Error> {
        Ok(Self {
            file: File::options()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(path.clone())
                .await?,
            path,
        })
    }
}

impl Deref for SerialFile {
    type Target = File;
    fn deref(&self) -> &Self::Target {
        &self.file
    }
}

impl DerefMut for SerialFile {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.file
    }
}

impl AsyncWrite for SerialFile {
    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        Pin::new(&mut (self.get_mut()).file).poll_shutdown(cx)
    }
    fn poll_write_vectored(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        bufs: &[std::io::IoSlice<'_>],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        Pin::new(&mut (self.get_mut()).file).poll_write_vectored(cx, bufs)
    }
    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        Pin::new(&mut (self.get_mut()).file).poll_flush(cx)
    }
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        Pin::new(&mut (self.get_mut()).file).poll_write(cx, buf)
    }
    fn is_write_vectored(&self) -> bool {
        self.file.is_write_vectored()
    }
}

impl AsyncRead for SerialFile {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        Pin::new(&mut (self.get_mut()).file).poll_read(cx, buf)
    }
}

impl AsyncSeek for SerialFile {
    fn poll_complete(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<u64>> {
        Pin::new(&mut (self.get_mut()).file).poll_complete(cx)
    }
    fn start_seek(
        self: std::pin::Pin<&mut Self>,
        position: std::io::SeekFrom,
    ) -> std::io::Result<()> {
        Pin::new(&mut (self.get_mut()).file).start_seek(position)
    }
}

impl Serialize for SerialFile {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.path.to_str().unwrap())
    }
}

impl<'de> Deserialize<'de> for SerialFile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        block_on(async {
            let path = deserializer.deserialize_str(PathBufVisitor)?;
            let file = File::options()
                .read(true)
                .write(true)
                .open(path.clone())
                .await
                .map_err(|err| D::Error::custom(format!("{:#?}", err)))?;
            Ok(Self { file, path })
        })
    }
}

/// [`BackedArray`] that uses a directory of plain files
#[derive(Debug, Serialize, Deserialize)]
pub struct DirectoryBackedArray<T> {
    array: BackedArray<T, SerialFile>,
    directory_root: PathBuf,
}

#[async_trait]
impl<T: Serialize + DeserializeOwned + Sync + Send> BackedArrayWrapper<T>
    for DirectoryBackedArray<T>
{
    type Storage = SerialFile;
    type BackingError = std::io::Error;

    async fn remove(&mut self, entry_idx: usize) -> Result<&Self, std::io::Error> {
        remove_file(self.get_disks()[entry_idx].path.clone()).await?;
        self.array.remove(entry_idx);
        Ok(self)
    }

    async fn append(&mut self, values: &[T]) -> bincode::Result<&Self> {
        let next_target = self.next_target().await.map_err(bincode::Error::custom)?;
        self.array.append(values, next_target).await?;
        Ok(self)
    }

    async fn append_memory(&mut self, values: Box<[T]>) -> bincode::Result<&Self> {
        let next_target = self.next_target().await.map_err(bincode::Error::custom)?;
        self.array.append_memory(values, next_target).await?;
        Ok(self)
    }

    async fn append_array(&mut self, mut rhs: Self) -> Result<&Self, Self::BackingError> {
        rhs.move_root(self.directory_root.clone()).await?;
        self.array.append_array(rhs.array);
        Ok(self)
    }
}

impl<T> Deref for DirectoryBackedArray<T> {
    type Target = BackedArray<T, SerialFile>;

    fn deref(&self) -> &Self::Target {
        &self.array
    }
}

impl<T> DerefMut for DirectoryBackedArray<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.array
    }
}

impl<T> DirectoryBackedArray<T> {
    /// Creates a new directory at `directory_root`.
    ///
    /// * `directory_root`: Valid read/write directory on system
    pub async fn new(directory_root: PathBuf) -> std::io::Result<Self> {
        create_dir_all(directory_root.clone()).await?;
        Ok(DirectoryBackedArray {
            array: BackedArray::default(),
            directory_root,
        })
    }

    /// Updates the root of the directory backed array.
    ///
    /// Does not move any files or directories, just changes pointers.
    pub fn update_root(&mut self, new_root: PathBuf) -> &Self {
        self.array.get_disks_mut().iter_mut().for_each(|disk| {
            disk.path = new_root.join(disk.path.file_name().unwrap());
        });
        self.directory_root = new_root;
        self
    }

    /// Moves the directory to a new location wholesale.
    pub async fn move_root(&mut self, new_root: PathBuf) -> std::io::Result<&Self> {
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

impl<T: Serialize> DirectoryBackedArray<T> {
    async fn next_target(&self) -> std::io::Result<SerialFile> {
        SerialFile::new(self.directory_root.join(Uuid::new_v4().to_string())).await
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
        let directory = temp_dir().join("directory_write_async");
        let _ = remove_dir_all(directory.clone()).await;
        let mut arr = DirectoryBackedArray::new(directory.clone()).await.unwrap();
        let (values, second_values) = values();

        arr.append_memory(values.into()).await.unwrap();
        arr.append(&second_values).await.unwrap();
        assert_eq!(arr.get(100).await.unwrap(), &"TEST STRING");
        assert_eq!(arr.get(15_000).await.unwrap(), &"OTHER VALUE");

        remove_dir_all(directory).await.unwrap();
    }

    #[tokio::test]
    async fn write_and_read() {
        let directory = temp_dir().join("directory_write_and_read_async");
        let _ = remove_dir_all(directory.clone()).await;
        let mut arr = DirectoryBackedArray::new(directory.clone()).await.unwrap();
        let (values, second_values) = values();

        arr.append(&values).await.unwrap();
        arr.append_memory(second_values.into()).await.unwrap();
        arr.save_to_disk(&mut File::create(directory.join("directory")).await.unwrap())
            .await
            .unwrap();
        drop(arr);

        let mut arr: DirectoryBackedArray<String> =
            DirectoryBackedArray::load(&mut File::open(directory.join("directory")).await.unwrap())
                .await
                .unwrap();
        assert_eq!(arr.get(100).await.unwrap(), &"TEST STRING");
        assert_eq!(arr.get(15_000).await.unwrap(), &"OTHER VALUE");
        assert_eq!(arr.get(200).await.unwrap(), &"TEST STRING");
        assert_eq!(arr.get(1).await.unwrap(), &"TEST STRING");

        remove_dir_all(directory).await.unwrap();
    }
}
