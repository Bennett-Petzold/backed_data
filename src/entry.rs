pub trait BackedEntryUnload {
    /// Removes the entry from memory.
    fn unload(&mut self);
}

use std::{
    io::{Read, Seek, Write},
    ops::{Deref, DerefMut},
};

use bincode::{deserialize_from, serialize_into};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// Entry kept on some backing storage, loaded into memory on request.
///
/// This is only useful for types that can occupy zero/almost zero heap,
/// so most types should use the generic Option wrapper with a boxed type.
/// Boxed arrays can occupy zero heap, so they have a special implementation.
/// Broader collections do not have a special implementation, as they do not
/// guarantee memory freeing.
///
/// Writing/reading is serialized with bincode
///     <https://docs.rs/bincode/latest/bincode/> for space efficiency.
///
/// # Generics
///
/// * `T`: the type to store (should use heap, to make clearing useful)
/// * `Disk`: any backing store with write/read (or the async equivalent)
///
/// # Examples
/// See [`BackedEntryArr`] and [`BackedEntryOption`]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackedEntry<T, Disk: ?Sized> {
    value: T,
    disk_entry: Disk,
}

impl<T, Disk> BackedEntry<T, Disk> {
    pub fn get_disk(&self) -> &Disk {
        &self.disk_entry
    }

    pub fn get_disk_mut(&mut self) -> &mut Disk {
        &mut self.disk_entry
    }
}

/// Typedef of [`BackedEntry`].
///
/// # Example
///
/// ```rust
/// use std::fs::{File, remove_file};
/// use backed_array::entry::BackedEntryArr;
///
/// let FILENAME = std::env::temp_dir().join("example_array");
///
/// // Write array to file
/// let file = File::create(FILENAME.clone()).unwrap();
/// let mut writer: BackedEntryArr<i32, _> = BackedEntryArr::new(file);
/// writer.write_unload(&[1, 2, 3]).unwrap();
/// drop(writer);
///
/// // Read array from file
/// let file = File::open(FILENAME.clone()).unwrap();
/// let mut sparse: BackedEntryArr<i32, _> = BackedEntryArr::new(file);
/// assert_eq!(sparse.load().unwrap(), [1, 2, 3]);
///
/// // Cleanup
/// remove_file(FILENAME).unwrap();
/// ```
pub type BackedEntryArr<T, Disk> = BackedEntry<Box<[T]>, Disk>;

/// Typedef of [`BackedEntry`].
///
/// # Example
///
/// ```rust
/// use std::fs::{File, remove_file};
/// use std::env::temp_dir;
/// use backed_array::entry::BackedEntryOption;
///
/// let FILENAME = std::env::temp_dir().join("example_option");
///
/// // Write array to file
/// let file = File::create(FILENAME.clone()).unwrap();
/// let mut writer: BackedEntryOption<String, _> = BackedEntryOption::new(file);
/// writer.write_unload(&"Limited memory".to_string()).unwrap();
/// drop(writer);
///
/// // Read array from file
/// let file = File::open(FILENAME.clone()).unwrap();
/// let mut sparse: BackedEntryOption<String, _> = BackedEntryOption::new(file);
/// assert_eq!(sparse.load().unwrap(), "Limited memory");
///
/// // Cleanup
/// remove_file(FILENAME).unwrap();
/// ```
pub type BackedEntryOption<T, Disk> = BackedEntry<Option<T>, Disk>;

// ----- Initializations ----- //

impl<T, Disk> BackedEntryArr<T, Disk> {
    pub fn new(disk_entry: Disk) -> Self {
        Self {
            value: Box::new([]),
            disk_entry,
        }
    }
}

impl<T, Disk> BackedEntryOption<T, Disk> {
    pub fn new(disk_entry: Disk) -> Self {
        Self {
            value: None,
            disk_entry,
        }
    }
}

// ----- General Implementations ----- //

impl<T: Serialize, Disk: Write> BackedEntry<T, Disk> {
    /// Updates underlying storage with the current entry
    fn update(&mut self) -> bincode::Result<()> {
        serialize_into(&mut self.disk_entry, &self.value)?;
        self.disk_entry.flush()?; // Make sure buffer is emptied
        Ok(())
    }

    /// Writes the new value to memory and disk.
    ///
    /// See [`Self::write_unload`] to skip the memory write.
    pub fn write(&mut self, new_value: T) -> bincode::Result<()> {
        serialize_into(&mut self.disk_entry, &new_value)?;
        self.disk_entry.flush()?; // Make sure buffer is emptied
        self.value = new_value;
        Ok(())
    }
}
impl<T: Serialize, Disk: Write> BackedEntry<T, Disk> where BackedEntry<T, Disk>: BackedEntryUnload {}

// ----- Boxed Array Implementations ----- //

impl<T: DeserializeOwned, Disk: Read + Seek> BackedEntryArr<T, Disk> {
    /// Returns the entry, loading from disk if not in memory.
    ///
    /// Will remain in memory until an explicit call to unload.
    pub fn load(&mut self) -> bincode::Result<&[T]> {
        if self.value.is_empty() {
            self.disk_entry.rewind()?;
            self.value = deserialize_from(&mut self.disk_entry)?;
        }
        Ok(&self.value)
    }
}

impl<T, U> BackedEntryUnload for BackedEntryArr<T, U> {
    fn unload(&mut self) {
        self.value = Box::new([]);
    }
}

impl<T: Serialize, U: Write> BackedEntryArr<T, U> {
    /// Write the value to disk only, unloading current memory.
    ///
    /// See [`Self::write`] to keep the value in memory.
    pub fn write_unload(&mut self, new_value: &[T]) -> bincode::Result<()> {
        self.unload();
        serialize_into(&mut self.disk_entry, new_value)?;
        self.disk_entry.flush()?; // Make sure buffer is emptied
        Ok(())
    }
}

// ----- Option Implementations ----- //

impl<T: DeserializeOwned, Disk: Read + Seek> BackedEntryOption<T, Disk> {
    /// Returns the entry, loading from disk if not in memory.
    ///
    /// Will remain in memory until an explicit call to unload.
    pub fn load(&mut self) -> bincode::Result<&T> {
        if self.value.is_none() {
            self.disk_entry.rewind()?;
            self.value = Some(deserialize_from(&mut self.disk_entry)?);
        }
        Ok(self.value.as_ref().unwrap())
    }
}
impl<T, U> BackedEntryUnload for BackedEntryOption<T, U> {
    fn unload(&mut self) {
        self.value = None;
    }
}

impl<T: Serialize, U: Write> BackedEntryOption<T, U> {
    /// Write the value to disk only, unloading current memory.
    ///
    /// See [`Self::write`] to keep the value in memory.
    pub fn write_unload(&mut self, new_value: &T) -> bincode::Result<()> {
        self.unload();
        serialize_into(&mut self.disk_entry, new_value)?;
        self.disk_entry.flush()?; // Make sure buffer is emptied
        Ok(())
    }
}

/// Indicates that a disk entry can be overwritten safely.
///
/// Okay for many types of storage (e.g. a solo reference to a file),
/// dangerous for some types of storage (e.g. a slice into a file used to store
/// multiple entries).
/// Only implement this for backing stores that can be freely overwritten with
/// any size.
pub trait DiskOverwritable {}

/// Gives mutable handle to a backed entry.
///
/// Modifying by [`BackedEntry::write`] writes the entire value to the
/// underlying storage on every modification. This allows for multiple values
/// values to be written before syncing with disk.
///
/// Call [`BackedEntry::flush`] to sync with underlying storage before
/// dropping. Otherwise, a panicking drop implementation runs.
pub struct BackedEntryMut<'a, T: Serialize, Disk: Write> {
    entry: &'a mut BackedEntry<T, Disk>,
    modified: bool,
}

impl<'a, T: Serialize, Disk: Write> Deref for BackedEntryMut<'a, T, Disk> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.entry.value
    }
}

impl<'a, T: Serialize, Disk: Write> DerefMut for BackedEntryMut<'a, T, Disk> {
    /// [`DerefMut::deref_mut`] that sets a modified flag.
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.modified = true;
        &mut self.entry.value
    }
}

impl<'a, T: Serialize, Disk: Write> BackedEntryMut<'a, T, Disk> {
    /// Returns true if the memory version is desynced from the disk version
    #[allow(dead_code)]
    pub fn is_modified(&self) -> bool {
        self.modified
    }

    /// Saves modifications to disk, unsetting the modified flag if sucessful.
    pub fn flush(&mut self) -> bincode::Result<&mut Self> {
        self.entry.update()?;
        self.modified = false;
        Ok(self)
    }
}

impl<'a, T: Serialize, Disk: Write> Drop for BackedEntryMut<'a, T, Disk> {
    /// [`Drop::drop`] that attempts a write if modified, and panics if that
    /// write returns and error.
    fn drop(&mut self) {
        if self.modified {
            self.flush().unwrap();
        }
    }
}

impl<T: Serialize + DeserializeOwned, Disk: Write + Read + Seek + DiskOverwritable>
    BackedEntryArr<T, Disk>
{
    /// Returns [`BackedEntryMut`] to allow efficient in-memory modifications
    /// if variable-sized writes are safe for the underlying storage.
    ///
    /// Make sure to call [`BackedEntryMut::flush`] to sync with disk before
    /// dropping.
    pub fn mut_handle(&mut self) -> bincode::Result<BackedEntryMut<Box<[T]>, Disk>> {
        self.load()?;
        Ok(BackedEntryMut {
            entry: self,
            modified: false,
        })
    }
}

impl<T: Serialize + DeserializeOwned, Disk: Write + Read + Seek + DiskOverwritable>
    BackedEntryOption<T, Disk>
{
    /// Returns [`BackedEntryMut`] to allow efficient in-memory modifications
    /// if variable-sized writes are safe for the underlying storage.
    ///
    /// Make sure to call [`BackedEntryMut::flush`] to sync with disk before
    /// dropping.
    pub fn mut_handle(&mut self) -> bincode::Result<BackedEntryMut<Option<T>, Disk>> {
        self.load()?;
        Ok(BackedEntryMut {
            entry: self,
            modified: false,
        })
    }
}

#[cfg(feature = "async")]
pub mod async_impl {
    use async_bincode::tokio::{AsyncBincodeReader, AsyncBincodeWriter};
    use bincode::deserialize_from;
    use futures::{SinkExt, StreamExt};
    use serde::{de::DeserializeOwned, Deserialize, Serialize};
    use std::{
        borrow::Borrow,
        ops::{Deref, DerefMut},
    };
    use tokio::io::{AsyncRead, AsyncSeek, AsyncSeekExt, AsyncWrite, AsyncWriteExt};
    use tokio_util::io::SyncIoBridge;

    use super::{
        BackedEntry, BackedEntryArr, BackedEntryOption, BackedEntryUnload, DiskOverwritable,
    };

    /// Used by [`BackedEntryAsync`] to track the valid reader.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub enum BackedEntryWriteMode {
        Sync,
        Async,
    }

    /// Async variant of [`BackedEntry`].
    ///
    /// # Why Separate?
    /// AsyncBincodeReader and BincodeReader use different formats.
    /// Therefore all writes and reads from AsyncBincodeWriter must support
    /// one or the other. This adds resolution so only the valid read is possible.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BackedEntryAsync<T, Disk: ?Sized> {
        mode: BackedEntryWriteMode,
        inner: BackedEntry<T, Disk>,
    }

    impl<T, Disk> BackedEntryUnload for BackedEntryAsync<T, Disk>
    where
        BackedEntry<T, Disk>: BackedEntryUnload,
    {
        fn unload(&mut self) {
            self.inner.unload()
        }
    }

    impl<T, Disk> BackedEntryAsync<T, Disk> {
        pub fn get_disk(&self) -> &Disk {
            &self.inner.disk_entry
        }

        pub fn get_disk_mut(&mut self) -> &mut Disk {
            &mut self.inner.disk_entry
        }

        pub fn get_mode(&self) -> &BackedEntryWriteMode {
            &self.mode
        }
    }

    /// Async version of [`BackedEntryArr`]
    pub type BackedEntryArrAsync<T, Disk> = BackedEntryAsync<Box<[T]>, Disk>;

    /// Async version of [`BackedEntryOption`]
    pub type BackedEntryOptionAsync<T, Disk> = BackedEntryAsync<Option<T>, Disk>;

    impl<T, Disk> BackedEntryArrAsync<T, Disk> {
        /// Set underlying storage and [`BackedEntryWriteMode`].
        ///
        /// # Arguments
        /// * `disk_entry`: Async storage to write/read
        /// * `mode`: Whether to write to sync or async reads. Default async.
        pub fn new(disk_entry: Disk, mode: Option<BackedEntryWriteMode>) -> Self {
            let mode = mode.unwrap_or(BackedEntryWriteMode::Async);
            Self {
                inner: BackedEntryArr::new(disk_entry),
                mode,
            }
        }
    }

    impl<T, Disk> BackedEntryOptionAsync<T, Disk> {
        pub fn new(disk_entry: Disk, mode: Option<BackedEntryWriteMode>) -> Self {
            let mode = mode.unwrap_or(BackedEntryWriteMode::Async);
            Self {
                inner: BackedEntryOption::new(disk_entry),
                mode,
            }
        }
    }

    impl<T: Serialize, Disk: AsyncWrite + Unpin> BackedEntryAsync<T, Disk> {
        #[inline]
        async fn update_lower_async<U: Borrow<T>>(&mut self, new_value: U) -> bincode::Result<()> {
            let new_value = new_value.borrow();
            let mut bincode_writer = AsyncBincodeWriter::from(&mut self.inner.disk_entry);
            match self.mode {
                BackedEntryWriteMode::Async => {
                    bincode_writer.for_async().send(&new_value).await?;
                }
                BackedEntryWriteMode::Sync => {
                    bincode_writer.send(&new_value).await?;
                }
            };

            self.inner.disk_entry.flush().await?;
            Ok(())
        }

        /// Updates underlying storage with the current entry
        async fn update(&mut self) -> bincode::Result<()> {
            let mut bincode_writer = AsyncBincodeWriter::from(&mut self.inner.disk_entry);
            match self.mode {
                BackedEntryWriteMode::Async => {
                    bincode_writer.for_async().send(&self.inner.value).await?;
                }
                BackedEntryWriteMode::Sync => {
                    bincode_writer.send(&self.inner.value).await?;
                }
            };

            self.inner.disk_entry.flush().await?;
            Ok(())
        }

        /// Writes the new value to memory and disk.
        ///
        /// See [`Self::write_unload`] to skip the memory write.
        pub async fn write(&mut self, new_value: T) -> bincode::Result<()> {
            self.update_lower_async(&new_value).await?;
            self.inner.value = new_value;
            Ok(())
        }

        /// Convert the backed version to be sync read compatible
        pub async fn conv_to_sync(&mut self) -> bincode::Result<()> {
            if self.mode == BackedEntryWriteMode::Async {
                self.mode = BackedEntryWriteMode::Sync;
                AsyncBincodeWriter::from(&mut self.inner.disk_entry)
                    .send(&self.inner.value)
                    .await?;
                self.inner.disk_entry.flush().await?;
            }
            Ok(())
        }

        /// Convert the backed version to be async read compatible
        pub async fn conv_to_async(&mut self) -> bincode::Result<()> {
            if self.mode == BackedEntryWriteMode::Sync {
                self.mode = BackedEntryWriteMode::Async;
                AsyncBincodeWriter::from(&mut self.inner.disk_entry)
                    .for_async()
                    .send(&self.inner.value)
                    .await?;
                self.inner.disk_entry.flush().await?;
            }
            Ok(())
        }
    }

    impl<T: Serialize, Disk: AsyncWrite + Unpin> BackedEntryArrAsync<T, Disk> {
        /// Write the value to disk only, unloading current memory.
        ///
        /// See [`Self::write`] to keep the value in memory.
        pub async fn write_unload(&mut self, new_value: &[T]) -> bincode::Result<()> {
            self.unload();
            let mut bincode_writer = AsyncBincodeWriter::from(&mut self.inner.disk_entry);
            match self.mode {
                BackedEntryWriteMode::Async => {
                    bincode_writer.for_async().send(&new_value).await?;
                }
                BackedEntryWriteMode::Sync => {
                    bincode_writer.send(&new_value).await?;
                }
            };

            self.inner.disk_entry.flush().await?;
            Ok(())
        }
    }

    impl<T: Serialize, Disk: AsyncWrite + Unpin> BackedEntryOptionAsync<T, Disk> {
        /// Write the value to disk only, unloading current memory.
        ///
        /// See [`Self::write`] to keep the value in memory.
        pub async fn write_unload(&mut self, new_value: &T) -> bincode::Result<()> {
            self.unload();
            let mut bincode_writer = AsyncBincodeWriter::from(&mut self.inner.disk_entry);
            match self.mode {
                BackedEntryWriteMode::Async => {
                    bincode_writer.for_async().send(&new_value).await?;
                }
                BackedEntryWriteMode::Sync => {
                    bincode_writer.send(&new_value).await?;
                }
            };

            self.inner.disk_entry.flush().await?;
            Ok(())
        }
    }

    impl<T: DeserializeOwned, Disk: AsyncRead + AsyncSeek + Unpin> BackedEntryArrAsync<T, Disk> {
        /// Async version of [`BackedEntryArr::load`].
        ///
        /// Will use AsyncBincodeReader or BincodeReader depending on mode.
        /// Check that [`Self::get_mode`] is async for optimal read performance.
        /// The sync implementation will block the thread.
        pub async fn load(&mut self) -> Result<&[T], Box<bincode::ErrorKind>> {
            if self.inner.value.is_empty() {
                self.inner.disk_entry.rewind().await?;
                match self.mode {
                    BackedEntryWriteMode::Sync => {
                        self.inner.value =
                            deserialize_from(SyncIoBridge::new(&mut self.inner.disk_entry))?;
                    }
                    BackedEntryWriteMode::Async => {
                        self.inner.value = AsyncBincodeReader::from(&mut self.inner.disk_entry)
                            .next()
                            .await
                            .ok_or(bincode::ErrorKind::Custom(
                                "AsyncBincodeReader stream empty".to_string(),
                            ))??
                    }
                }
            }
            Ok(&self.inner.value)
        }
    }

    impl<T: DeserializeOwned, Disk: AsyncRead + AsyncSeek + Unpin> BackedEntryOptionAsync<T, Disk> {
        /// Async version of [`BackedEntryOption::load`].
        ///
        /// Will use AsyncBincodeReader or BincodeReader depending on mode.
        /// Check that [`Self::get_mode`] is async for optimal read performance.
        /// The sync implementation will block the thread.
        pub async fn load(&mut self) -> Result<&T, Box<bincode::ErrorKind>> {
            if self.inner.value.is_none() {
                self.inner.disk_entry.rewind().await?;
                match self.mode {
                    BackedEntryWriteMode::Sync => {
                        self.inner.value =
                            deserialize_from(SyncIoBridge::new(&mut self.inner.disk_entry))?;
                    }
                    BackedEntryWriteMode::Async => {
                        self.inner.value = AsyncBincodeReader::from(&mut self.inner.disk_entry)
                            .next()
                            .await
                            .ok_or(bincode::ErrorKind::Custom(
                                "AsyncBincodeReader stream empty".to_string(),
                            ))??
                    }
                }
            }
            Ok(self.inner.value.as_ref().unwrap())
        }
    }

    /// Async version of [`super::BackedEntryMut`].
    ///
    /// Modifying by [`BackedEntryAsync::write`] writes the entire value to the
    /// underlying storage on every modification. This allows for multiple values
    /// values to be written before syncing with disk.
    ///
    /// Call [`BackedEntry::flush`] to sync with underlying storage before
    /// dropping. Otherwise, drop panics.
    pub struct BackedEntryMutAsync<'a, T: Serialize, Disk: AsyncWrite + Unpin> {
        entry: &'a mut BackedEntryAsync<T, Disk>,
        modified: bool,
    }

    impl<'a, T: Serialize, Disk: AsyncWrite + Unpin> Deref for BackedEntryMutAsync<'a, T, Disk> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            &self.entry.inner.value
        }
    }

    impl<'a, T: Serialize, Disk: AsyncWrite + Unpin> DerefMut for BackedEntryMutAsync<'a, T, Disk> {
        /// [`DerefMut::deref_mut`] that sets a modified flag.
        fn deref_mut(&mut self) -> &mut Self::Target {
            self.modified = true;
            &mut self.entry.inner.value
        }
    }

    impl<'a, T: Serialize, Disk: AsyncWrite + Unpin> BackedEntryMutAsync<'a, T, Disk> {
        /// Returns true if the memory version is desynced from the disk version
        #[allow(dead_code)]
        pub fn is_modified(&self) -> bool {
            self.modified
        }

        /// Saves modifications to disk, unsetting the modified flag if sucessful.
        pub async fn flush(&mut self) -> bincode::Result<&mut Self> {
            self.entry.update().await?;
            self.modified = false;
            Ok(self)
        }
    }

    impl<'a, T: Serialize, Disk: AsyncWrite + Unpin> Drop for BackedEntryMutAsync<'a, T, Disk> {
        /// [`Drop::drop`] panics if the value isn't written to disk with
        /// [`Self::flush`].
        fn drop(&mut self) {
            assert!(!self.modified)
        }
    }

    impl<
            T: Serialize + DeserializeOwned,
            Disk: AsyncWrite + Unpin + AsyncRead + AsyncSeek + DiskOverwritable,
        > BackedEntryArrAsync<T, Disk>
    {
        /// Returns [`BackedEntryMutAsync`] to allow efficient in-memory modifications
        /// if variable-sized writes are safe for the underlying storage.
        ///
        /// Make sure to call [`BackedEntryMutAsync::flush`] to sync with disk before
        /// dropping.
        pub async fn mut_handle(&mut self) -> bincode::Result<BackedEntryMutAsync<Box<[T]>, Disk>> {
            self.load().await?;
            Ok(BackedEntryMutAsync {
                entry: self,
                modified: false,
            })
        }
    }

    impl<
            T: Serialize + DeserializeOwned,
            Disk: AsyncWrite + Unpin + AsyncRead + AsyncSeek + DiskOverwritable,
        > BackedEntryOptionAsync<T, Disk>
    {
        /// Returns [`BackedEntryMutAsync`] to allow efficient in-memory modifications
        /// if variable-sized writes are safe for the underlying storage.
        ///
        /// Make sure to call [`BackedEntryMutAsync::flush`] to sync with disk before
        /// dropping.
        pub async fn mut_handle(
            &mut self,
        ) -> bincode::Result<BackedEntryMutAsync<Option<T>, Disk>> {
            self.load().await?;
            Ok(BackedEntryMutAsync {
                entry: self,
                modified: false,
            })
        }
    }

    #[cfg(test)]
    mod tests {

        use std::{io::Cursor, pin::Pin};

        use super::*;

        #[derive(Debug)]
        struct CursorVec(pub Cursor<Vec<u8>>);

        impl Unpin for CursorVec {}

        impl DiskOverwritable for &mut CursorVec {}

        impl AsyncWrite for CursorVec {
            fn poll_shutdown(
                self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Result<(), std::io::Error>> {
                Pin::new(&mut (self.get_mut()).0.get_mut()).poll_shutdown(cx)
            }
            fn poll_write_vectored(
                self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
                bufs: &[std::io::IoSlice<'_>],
            ) -> std::task::Poll<Result<usize, std::io::Error>> {
                Pin::new(&mut (self.get_mut()).0.get_mut()).poll_write_vectored(cx, bufs)
            }
            fn poll_flush(
                self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Result<(), std::io::Error>> {
                Pin::new(&mut (self.get_mut()).0.get_mut()).poll_flush(cx)
            }
            fn poll_write(
                self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
                buf: &[u8],
            ) -> std::task::Poll<Result<usize, std::io::Error>> {
                Pin::new(&mut (self.get_mut()).0.get_mut()).poll_write(cx, buf)
            }
            fn is_write_vectored(&self) -> bool {
                self.0.get_ref().is_write_vectored()
            }
        }

        impl AsyncRead for CursorVec {
            fn poll_read(
                self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
                buf: &mut tokio::io::ReadBuf<'_>,
            ) -> std::task::Poll<std::io::Result<()>> {
                Pin::new(&mut (self.get_mut()).0).poll_read(cx, buf)
            }
        }

        impl AsyncSeek for CursorVec {
            fn poll_complete(
                self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<std::io::Result<u64>> {
                Pin::new(&mut (self.get_mut()).0).poll_complete(cx)
            }
            fn start_seek(
                self: std::pin::Pin<&mut Self>,
                position: std::io::SeekFrom,
            ) -> std::io::Result<()> {
                Pin::new(&mut (self.get_mut()).0).start_seek(position)
            }
        }

        #[tokio::test]
        async fn mutate() {
            const FIB: &[u8] = &[0, 1, 1, 5, 7];
            let mut back_vec = CursorVec(Cursor::new(Vec::with_capacity(10)));
            let back_vec_ptr: *mut CursorVec = &mut back_vec;

            // Intentional unsafe access to later peek underlying storage
            let mut backed_entry = unsafe { BackedEntryArrAsync::new(&mut *back_vec_ptr, None) };
            backed_entry.write_unload(FIB).await.unwrap();

            assert_eq!(backed_entry.load().await.unwrap(), FIB);

            let backing_store = back_vec.0.get_ref();
            assert_eq!(&backing_store[backing_store.len() - FIB.len()..], FIB);

            let mut handle = backed_entry.mut_handle().await.unwrap();
            handle[0] = 20;
            handle[2] = 30;

            let backing_store = back_vec.0.get_ref();
            assert_eq!(backing_store[backing_store.len() - FIB.len()], FIB[0]);
            assert_eq!(handle[0], 20);
            assert_eq!(handle[2], 30);

            handle.flush().await.unwrap();
            let backing_store = back_vec.0.get_ref();
            assert_eq!(backing_store[backing_store.len() - FIB.len()], 20);
            assert_eq!(backing_store[backing_store.len() - FIB.len() + 2], 30);
            assert_eq!(backing_store[backing_store.len() - FIB.len() + 1], FIB[1]);

            drop(handle);
            assert_eq!(backed_entry.load().await.unwrap(), [20, 1, 30, 5, 7]);
        }

        #[should_panic]
        #[tokio::test]
        async fn mutate_panic() {
            const FIB: &[u8] = &[0, 1, 1, 5, 7];
            let mut back_vec = CursorVec(Cursor::new(Vec::with_capacity(10)));
            let back_vec_ptr: *mut CursorVec = &mut back_vec;

            // Intentional unsafe access to later peek underlying storage
            let mut backed_entry = unsafe { BackedEntryArrAsync::new(&mut *back_vec_ptr, None) };
            backed_entry.write_unload(FIB).await.unwrap();

            assert_eq!(backed_entry.load().await.unwrap(), FIB);

            let backing_store = back_vec.0.get_ref();
            assert_eq!(&backing_store[backing_store.len() - FIB.len()..], FIB);

            let mut handle = backed_entry.mut_handle().await.unwrap();
            handle[0] = 20;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[derive(Debug)]
    struct CursorVec(pub Cursor<Vec<u8>>);

    impl DiskOverwritable for &mut CursorVec {}

    impl Write for CursorVec {
        fn flush(&mut self) -> std::io::Result<()> {
            self.0.get_mut().flush()
        }
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.get_mut().write(buf)
        }
    }

    impl Read for CursorVec {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.0.read(buf)
        }
    }

    impl Seek for CursorVec {
        fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
            self.0.seek(pos)
        }

        fn rewind(&mut self) -> std::io::Result<()> {
            self.0.rewind()
        }
    }

    #[test]
    fn mutate() {
        const FIB: &[u8] = &[0, 1, 1, 5, 7];
        let mut back_vec = CursorVec(Cursor::new(Vec::with_capacity(10)));
        let back_vec_ptr: *mut CursorVec = &mut back_vec;

        // Intentional unsafe access to later peek underlying storage
        let mut backed_entry = unsafe { BackedEntryArr::new(&mut *back_vec_ptr) };
        backed_entry.write_unload(FIB).unwrap();

        assert_eq!(backed_entry.load().unwrap(), FIB);

        let backing_store = back_vec.0.get_ref();
        assert_eq!(&backing_store[backing_store.len() - FIB.len()..], FIB);

        let mut handle = backed_entry.mut_handle().unwrap();
        handle[0] = 20;
        handle[2] = 30;

        let backing_store = back_vec.0.get_ref();
        assert_eq!(backing_store[backing_store.len() - FIB.len()], FIB[0]);
        assert_eq!(handle[0], 20);
        assert_eq!(handle[2], 30);

        handle.flush().unwrap();
        let backing_store = back_vec.0.get_ref();
        assert_eq!(backing_store[backing_store.len() - FIB.len()], 20);
        assert_eq!(backing_store[backing_store.len() - FIB.len() + 2], 30);
        assert_eq!(backing_store[backing_store.len() - FIB.len() + 1], FIB[1]);

        drop(handle);
        assert_eq!(backed_entry.load().unwrap(), [20, 1, 30, 5, 7]);
    }
}
