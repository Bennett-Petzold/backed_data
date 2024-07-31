#[cfg(any(feature = "zstdmt", feature = "async-zstdmt"))]
use {lazy_static::lazy_static, std::sync::Mutex};

#[cfg(any(feature = "zstdmt", feature = "async-zstdmt"))]
lazy_static! {
    static ref ZSTD_MULTITHREAD: Mutex<u32> = Mutex::new(1);
}

#[cfg(any(feature = "zstdmt", feature = "async-zstdmt"))]
/// Set the level of zstdmt multithreading
///
/// Default is 1 (one extra thread).
/// Values greater than 0 run compression in N background threads.
pub fn set_zstd_multithread(value: u32) {
    *ZSTD_MULTITHREAD.lock().unwrap() = value;
}

pub mod sync_impl {
    use std::{
        fs::{copy, create_dir_all, remove_file, rename, File},
        io::{BufReader, Read, Seek, Write},
        ops::{Deref, DerefMut},
        path::PathBuf,
    };

    use bincode::{deserialize_from, serialize_into};
    use derive_getters::Getters;
    use serde::{
        de::{DeserializeOwned, Error},
        Deserialize, Serialize,
    };
    use uuid::Uuid;
    use zstd::{Decoder, Encoder};

    use crate::{array::sync_impl::BackedArray, meta::sync_impl::BackedArrayWrapper};

    #[cfg(feature = "zstdmt")]
    use super::ZSTD_MULTITHREAD;

    /// File encoded with zstd
    pub struct ZstdFile<'a> {
        file: File,
        path: PathBuf,
        #[allow(dead_code)]
        decoder: Decoder<'a, BufReader<File>>,
        #[allow(dead_code)]
        encoder: Encoder<'a, File>,
        zstd_level: i32,
    }

    #[derive(Serialize, Deserialize)]
    struct ZstdFileSerialized {
        path: String,
        zstd_level: i32,
    }

    impl ZstdFile<'_> {
        /// Create a new ZstdFile
        ///
        /// * `path`: A valid filesystem path
        /// * `zstd_level`: An optional level bound [0-22]. 0 for library default.
        pub fn new(path: PathBuf, zstd_level: Option<i32>) -> std::io::Result<Self> {
            let zstd_level = zstd_level.unwrap_or(0);
            if !(0..=22).contains(&zstd_level) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("zstd_level ({zstd_level}) not [0, 22]"),
                ));
            };
            let file = File::options()
                .read(true)
                .write(true)
                .create(true)
                .open(path.clone())?;

            #[allow(unused_mut)]
            let mut encoder = Encoder::new(file.try_clone()?, zstd_level)?;
            #[cfg(feature = "zstdmt")]
            encoder.multithread(*ZSTD_MULTITHREAD.lock().unwrap())?;

            Ok(Self {
                file: file.try_clone()?,
                path,
                decoder: Decoder::new(file)?,
                encoder,
                zstd_level,
            })
        }
    }

    impl Serialize for ZstdFile<'_> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            let serial_form = ZstdFileSerialized {
                path: self.path.to_str().unwrap().to_string(),
                zstd_level: self.zstd_level,
            };
            serial_form.serialize(serializer)
        }
    }

    impl<'de> Deserialize<'de> for ZstdFile<'_> {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            let ZstdFileSerialized { path, zstd_level } =
                ZstdFileSerialized::deserialize(deserializer)?;
            let file = File::options()
                .read(true)
                .write(true)
                .open(path.clone())
                .map_err(|err| D::Error::custom(format!("{:#?}", err)))?;

            #[allow(unused_mut)]
            let mut encoder = Encoder::new(file.try_clone().map_err(D::Error::custom)?, zstd_level)
                .map_err(D::Error::custom)?;
            #[cfg(feature = "zstdmt")]
            encoder
                .multithread(*ZSTD_MULTITHREAD.lock().unwrap())
                .map_err(D::Error::custom)?;

            Ok(Self {
                file: file.try_clone().map_err(D::Error::custom)?,
                path: path.into(),
                decoder: Decoder::new(file.try_clone().map_err(D::Error::custom)?)
                    .map_err(D::Error::custom)?,
                encoder,
                zstd_level,
            })
        }
    }

    impl Read for ZstdFile<'_> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.decoder.read(buf)
        }
        fn read_vectored(
            &mut self,
            bufs: &mut [std::io::IoSliceMut<'_>],
        ) -> std::io::Result<usize> {
            self.decoder.read_vectored(bufs)
        }
    }

    impl Write for ZstdFile<'_> {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.encoder.write(buf)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            // Use the flush call as an opportunity to call finish on encoding
            // Without calling finish, the metadata won't be complete
            self.encoder.flush()?;
            self.encoder.do_finish()?;
            self.encoder = Encoder::new(self.file.try_clone()?, self.zstd_level)?;
            #[cfg(feature = "zstdmt")]
            self.encoder
                .multithread(*ZSTD_MULTITHREAD.lock().unwrap())?;
            Ok(())
        }
        fn write_vectored(&mut self, bufs: &[std::io::IoSlice<'_>]) -> std::io::Result<usize> {
            self.encoder.write_vectored(bufs)
        }
    }

    impl Seek for ZstdFile<'_> {
        fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
            self.decoder.get_mut().seek(pos)
        }
    }

    /// [`BackedArray`] that uses a directory of zstd compressed files
    #[derive(Serialize, Deserialize, Getters)]
    pub struct ZstdDirBackedArray<'a, T> {
        array: BackedArray<T, ZstdFile<'a>>,
        directory_root: PathBuf,
        zstd_level: Option<i32>,
    }

    impl<T> ZstdDirBackedArray<'_, T> {
        /// Creates a new array backed by zstd compressed files in a directory
        ///
        /// * `directory_root`: base directory for every file
        /// * `zstd_level`: An optional level bound [0-22]. 0 for library default.
        pub fn new(directory_root: PathBuf, zstd_level: Option<i32>) -> std::io::Result<Self> {
            create_dir_all(directory_root.clone())?;
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

    impl<'a, T: Serialize + DeserializeOwned> BackedArrayWrapper<T> for ZstdDirBackedArray<'a, T> {
        type Storage = ZstdFile<'a>;
        type BackingError = std::io::Error;

        fn remove(&mut self, entry_idx: usize) -> Result<&Self, std::io::Error> {
            remove_file(self.get_disks()[entry_idx].path.clone())?;
            self.array.remove(entry_idx);
            Ok(self)
        }

        fn append(&mut self, values: &[T]) -> bincode::Result<&Self> {
            self.array.append(
                values,
                ZstdFile::new(
                    self.directory_root
                        .clone()
                        .join(Uuid::new_v4().to_string() + ".zstd"),
                    self.zstd_level,
                )?,
            )?;
            Ok(self)
        }

        fn append_memory(&mut self, values: Box<[T]>) -> bincode::Result<&Self> {
            self.array.append_memory(
                values,
                ZstdFile::new(
                    self.directory_root
                        .clone()
                        .join(Uuid::new_v4().to_string() + ".zstd"),
                    self.zstd_level,
                )?,
            )?;
            Ok(self)
        }

        fn append_array(&mut self, mut rhs: Self) -> Result<&Self, Self::BackingError> {
            rhs.move_root(self.directory_root.clone())?;
            self.array.append_array(rhs.array);
            Ok(self)
        }
    }

    impl<'a, T> Deref for ZstdDirBackedArray<'a, T> {
        type Target = BackedArray<T, ZstdFile<'a>>;

        fn deref(&self) -> &Self::Target {
            &self.array
        }
    }

    impl<T> DerefMut for ZstdDirBackedArray<'_, T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.array
        }
    }

    impl<T: Serialize> ZstdDirBackedArray<'_, T> {
        /// Wraps [`BackedArray::save_to_disk`] to include its own metadata
        pub fn save_to_disk<W: Write>(&mut self, writer: W) -> bincode::Result<()> {
            self.array.clear_memory();
            serialize_into(writer, self)
        }
    }

    impl<T: DeserializeOwned> ZstdDirBackedArray<'_, T> {
        /// Wraps [`BackedArray::load`] to include its own metadata
        pub fn load<W: Read>(writer: W) -> bincode::Result<Self> {
            deserialize_from(writer)
        }
    }

    impl<T> ZstdDirBackedArray<'_, T> {
        /// Updates the root of the zstd directory backed array.
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
        pub fn move_root(&mut self, new_root: PathBuf) -> std::io::Result<&Self> {
            if rename(self.directory_root.clone(), new_root.clone()).is_err() {
                create_dir_all(new_root.clone())?;
                self.array
                    .get_disks()
                    .iter()
                    .map(|disk| {
                        copy(
                            disk.path.clone(),
                            new_root.join(disk.path.file_name().unwrap()),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
            }
            Ok(self.update_root(new_root))
        }
    }

    #[cfg(test)]
    mod tests {
        use std::{env::temp_dir, fs::remove_dir_all};

        use itertools::Itertools;

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

        #[test]
        fn write() {
            let directory = temp_dir().join("zstd_directory_write");
            let _ = remove_dir_all(directory.clone());
            let mut arr = ZstdDirBackedArray::new(directory.clone(), None).unwrap();
            let (values, second_values) = values();

            arr.append_memory(values.into()).unwrap();
            arr.append(&second_values).unwrap();
            assert_eq!(arr.get(100).unwrap(), &"TEST STRING");
            assert_eq!(arr.get(200).unwrap(), &"TEST STRING");
            assert_eq!(arr.get(150).unwrap(), &"TEST STRING");
            assert_eq!(arr.get(15_000).unwrap(), &"OTHER VALUE");

            remove_dir_all(directory).unwrap();
        }

        #[test]
        fn write_and_read() {
            let directory = temp_dir().join("zstd_directory_write_and_read");
            let _ = remove_dir_all(directory.clone());
            let mut arr = ZstdDirBackedArray::new(directory.clone(), None).unwrap();
            let (values, second_values) = values();

            arr.append(&values).unwrap();
            arr.append_memory(second_values.into()).unwrap();
            arr.save_to_disk(File::create(directory.join("directory")).unwrap())
                .unwrap();
            drop(arr);

            let mut arr: ZstdDirBackedArray<String> =
                ZstdDirBackedArray::load(File::open(directory.join("directory")).unwrap()).unwrap();
            assert_eq!(arr.get(100).unwrap(), &"TEST STRING");
            assert_eq!(arr.get(15_000).unwrap(), &"OTHER VALUE");
            assert_eq!(arr.get(200).unwrap(), &"TEST STRING");
            assert_eq!(arr.get(1).unwrap(), &"TEST STRING");

            remove_dir_all(directory).unwrap();
        }
    }
}

#[cfg(feature = "async-zstd")]
pub mod async_impl {
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
    use futures::{executor::block_on, SinkExt, StreamExt};
    use itertools::Itertools;
    use serde::{
        de::{DeserializeOwned, Error},
        Deserialize, Serialize,
    };
    use tokio::{
        fs::{copy, create_dir_all, remove_file, rename, File},
        io::{AsyncRead, AsyncSeek, AsyncWrite, AsyncWriteExt, BufReader},
        task::JoinSet,
    };
    use uuid::Uuid;

    use crate::{array::async_impl::BackedArray, meta::async_impl::BackedArrayWrapper};

    #[cfg(feature = "async-zstdmt")]
    use super::ZSTD_MULTITHREAD;

    /// File encoded with zstd
    pub struct ZstdFile {
        file: File,
        path: PathBuf,
        #[allow(dead_code)]
        decoder: ZstdDecoder<BufReader<File>>,
        #[allow(dead_code)]
        encoder: ZstdEncoder<File>,
        zstd_level: i32,
    }

    #[derive(Serialize, Deserialize)]
    struct ZstdFileSerialized {
        path: String,
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
            let file = File::options()
                .read(true)
                .write(true)
                .create(true)
                .open(path.clone())
                .await?;
            Ok(Self {
                file: file.try_clone().await?,
                path,
                decoder: ZstdDecoder::new(BufReader::new(file.try_clone().await?)),
                #[cfg(not(feature = "async-zstdmt"))]
                encoder: ZstdEncoder::with_quality(
                    file,
                    async_compression::Level::Precise(zstd_level),
                ),
                #[cfg(feature = "async-zstdmt")]
                encoder: ZstdEncoder::with_quality_and_params(
                    file,
                    async_compression::Level::Precise(zstd_level),
                    &[CParameter::nb_workers(*ZSTD_MULTITHREAD.lock().unwrap())],
                ),
                zstd_level,
            })
        }
    }

    impl Serialize for ZstdFile {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            let serial_form = ZstdFileSerialized {
                path: self.path.to_str().unwrap().to_string(),
                zstd_level: self.zstd_level,
            };
            serial_form.serialize(serializer)
        }
    }

    impl<'de> Deserialize<'de> for ZstdFile {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            block_on(async {
                let ZstdFileSerialized { path, zstd_level } =
                    ZstdFileSerialized::deserialize(deserializer)?;
                let file = File::options()
                    .read(true)
                    .write(true)
                    .open(path.clone())
                    .await
                    .map_err(|err| D::Error::custom(format!("{:#?}", err)))?;
                Ok(Self {
                    file: file.try_clone().await.map_err(D::Error::custom)?,
                    path: path.into(),
                    decoder: ZstdDecoder::new(BufReader::new(
                        file.try_clone().await.map_err(D::Error::custom)?,
                    )),
                    #[cfg(not(feature = "async-zstdmt"))]
                    encoder: ZstdEncoder::with_quality(
                        file,
                        async_compression::Level::Precise(zstd_level),
                    ),
                    #[cfg(feature = "async-zstdmt")]
                    encoder: ZstdEncoder::with_quality_and_params(
                        file,
                        async_compression::Level::Precise(zstd_level),
                        &[CParameter::nb_workers(*ZSTD_MULTITHREAD.lock().unwrap())],
                    ),
                    zstd_level,
                })
            })
        }
    }

    impl AsyncRead for ZstdFile {
        fn poll_read(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            Pin::new(&mut (self.get_mut()).file).poll_read(cx, buf)
        }
    }

    impl AsyncWrite for ZstdFile {
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

    impl AsyncSeek for ZstdFile {
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
        async fn remove(&mut self, entry_idx: usize) -> Result<&Self, std::io::Error> {
            remove_file(self.get_disks()[entry_idx].path.clone()).await?;
            self.array.remove(entry_idx);
            Ok(self)
        }

        async fn append(&mut self, values: &[T]) -> bincode::Result<&Self> {
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

        async fn append_memory(&mut self, values: Box<[T]>) -> bincode::Result<&Self> {
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

        async fn append_array(&mut self, mut rhs: Self) -> Result<&Self, Self::BackingError> {
            rhs.move_root(self.directory_root.clone()).await?;
            self.array.append_array(rhs.array);
            Ok(self)
        }
    }

    impl<T> ZstdDirBackedArray<T> {
        /// Creates a new array backed by zstd compressed files in a directory
        ///
        /// * `directory_root`: base directory for every file
        /// * `zstd_level`: An optional level bound [0-22]. 0 for library default.
        pub async fn new(
            directory_root: PathBuf,
            zstd_level: Option<i32>,
        ) -> std::io::Result<Self> {
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

            let mut arr: ZstdDirBackedArray<String> = ZstdDirBackedArray::load(
                &mut File::open(directory.join("directory")).await.unwrap(),
            )
            .await
            .unwrap();
            assert_eq!(arr.get(100).await.unwrap(), &"TEST STRING");
            assert_eq!(arr.get(15_000).await.unwrap(), &"OTHER VALUE");
            assert_eq!(arr.get(200).await.unwrap(), &"TEST STRING");
            assert_eq!(arr.get(1).await.unwrap(), &"TEST STRING");

            remove_dir_all(directory).await.unwrap();
        }
    }
}
