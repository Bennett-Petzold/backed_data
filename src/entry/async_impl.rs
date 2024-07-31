use async_bincode::tokio::{AsyncBincodeReader, AsyncBincodeWriter};
use futures::{Future, SinkExt, StreamExt};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    borrow::Borrow,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    path::PathBuf,
    pin::pin,
};
use tokio::{
    fs::File,
    io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, BufWriter},
};

use crate::utils::ToMut;

use super::{BackedEntry, BackedEntryUnload, DiskOverwritable};

pub trait AsyncReadDisk: Unpin + Serialize + for<'de> Deserialize<'de> {
    type ReadDisk: AsyncRead;
    fn read_disk(&mut self) -> impl Future<Output = std::io::Result<Self::ReadDisk>> + Send + Sync;
}

pub trait AsyncWriteDisk: Unpin + Serialize + for<'de> Deserialize<'de> {
    type WriteDisk: AsyncWrite;
    fn write_disk(
        &mut self,
    ) -> impl Future<Output = std::io::Result<Self::WriteDisk>> + Send + Sync;
}

impl AsyncReadDisk for PathBuf {
    type ReadDisk = BufReader<File>;

    async fn read_disk(&mut self) -> std::io::Result<Self::ReadDisk> {
        Ok(BufReader::new(File::open(self.clone()).await?))
    }
}

impl AsyncWriteDisk for PathBuf {
    type WriteDisk = BufWriter<File>;

    async fn write_disk(&mut self) -> std::io::Result<Self::WriteDisk> {
        Ok(BufWriter::new(
            File::options()
                .write(true)
                .create(true)
                .truncate(true)
                .open(self.clone())
                .await?,
        ))
    }
}

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
pub struct BackedEntryAsync<T, Disk: for<'df> Deserialize<'df>> {
    mode: BackedEntryWriteMode,
    #[serde(bound = "BackedEntry<T, Disk>: Serialize + for<'df> Deserialize<'df>")]
    inner: BackedEntry<T, Disk>,
}

impl<T, Disk: for<'de> Deserialize<'de>> BackedEntryUnload for BackedEntryAsync<T, Disk>
where
    BackedEntry<T, Disk>: BackedEntryUnload,
{
    fn unload(&mut self) {
        self.inner.unload()
    }
}

impl<T, Disk: for<'de> Deserialize<'de>> BackedEntryAsync<T, Disk> {
    pub fn get_disk(&self) -> &Disk {
        self.inner.get_disk()
    }

    pub fn get_disk_mut(&mut self) -> &mut Disk {
        self.inner.get_disk_mut()
    }

    pub fn get_mode(&self) -> &BackedEntryWriteMode {
        &self.mode
    }
}

/// Async version of [`BackedEntryBox`]
pub type BackedEntryBoxAsync<T, Disk> = BackedEntryAsync<Box<T>, Disk>;
///
/// Async version of [`BackedEntryArr`]
pub type BackedEntryArrAsync<T, Disk> = BackedEntryBoxAsync<[T], Disk>;

impl<T, Disk: for<'de> Deserialize<'de>> BackedEntryAsync<T, Disk> {
    /// Set underlying storage and [`BackedEntryWriteMode`].
    ///
    /// # Arguments
    /// * `disk.read_disk().await?`: Async storage to write/read
    /// * `mode`: Whether to write to sync or async reads. Default async.
    pub fn new(disk: Disk, mode: Option<BackedEntryWriteMode>) -> Self {
        let mode = mode.unwrap_or(BackedEntryWriteMode::Async);
        Self {
            mode,
            inner: BackedEntry::new(disk),
        }
    }
}

impl<T: Serialize, Disk: AsyncWriteDisk> BackedEntryAsync<T, Disk> {
    #[inline]
    async fn update_lower_async<U: Borrow<T>>(&mut self, new_value: U) -> bincode::Result<()> {
        let new_value = new_value.borrow();
        let mut write_disk = pin!(self.inner.disk.write_disk().await?);
        let mut bincode_writer = AsyncBincodeWriter::from(&mut write_disk);
        match self.mode {
            BackedEntryWriteMode::Async => {
                let mut bincode_writer = bincode_writer.for_async();
                bincode_writer.send(new_value).await?;
                bincode_writer.flush().await?;
            }
            BackedEntryWriteMode::Sync => {
                bincode_writer.send(new_value).await?;
                bincode_writer.flush().await?;
                bincode_writer.flush().await?;
            }
        };

        write_disk.flush().await?;
        write_disk.shutdown().await?;
        Ok(())
    }

    /// Updates underlying storage with the current entry
    async fn update(&mut self) -> bincode::Result<()> {
        if let Some(val) = self.inner.value.as_ref() {
            let mut write_disk = pin!(self.inner.disk.write_disk().await?);
            let mut bincode_writer = AsyncBincodeWriter::from(&mut write_disk);
            match self.mode {
                BackedEntryWriteMode::Async => {
                    let mut bincode_writer = bincode_writer.for_async();
                    bincode_writer.send(val).await?;
                    bincode_writer.flush().await?;
                }
                BackedEntryWriteMode::Sync => {
                    bincode_writer.send(val).await?;
                    bincode_writer.flush().await?;
                }
            };

            write_disk.flush().await?;
            write_disk.shutdown().await?;
        }
        Ok(())
    }

    /// Writes the new value to memory and disk.
    ///
    /// See [`Self::write_unload`] to skip the memory write.
    pub async fn write(&mut self, new_value: T) -> bincode::Result<()> {
        self.update_lower_async(&new_value).await?;
        self.inner.value = Some(new_value);
        Ok(())
    }

    /// Convert the backed version to be sync read compatible
    pub async fn conv_to_sync(&mut self) -> bincode::Result<()> {
        if let Some(val) = self.inner.value.as_ref() {
            if self.mode == BackedEntryWriteMode::Async {
                self.mode = BackedEntryWriteMode::Sync;

                let mut write_disk = pin!(self.inner.disk.write_disk().await?);
                let mut bincode_writer = AsyncBincodeWriter::from(&mut write_disk);
                bincode_writer.send(val).await?;
                bincode_writer.flush().await?;
                write_disk.flush().await?;
                write_disk.shutdown().await?;
            }
        }
        Ok(())
    }

    /// Convert the backed version to be async read compatible
    pub async fn conv_to_async(&mut self) -> bincode::Result<()> {
        if let Some(val) = self.inner.value.as_ref() {
            if self.mode == BackedEntryWriteMode::Sync {
                self.mode = BackedEntryWriteMode::Async;

                let mut write_disk = pin!(self.inner.disk.write_disk().await?);
                let mut bincode_writer = AsyncBincodeWriter::from(&mut write_disk).for_async();
                bincode_writer.send(val).await?;
                bincode_writer.flush().await?;
                write_disk.flush().await?;
                write_disk.shutdown().await?;
            }
        }
        Ok(())
    }
}

impl<T: Serialize, Disk: AsyncWriteDisk> BackedEntryAsync<T, Disk> {
    pub async fn into_sync_entry(mut self) -> bincode::Result<BackedEntry<T, Disk>> {
        self.conv_to_sync().await?;
        Ok(self.inner)
    }

    pub async fn from_sync_entry(sync_entry: BackedEntry<T, Disk>) -> bincode::Result<Self> {
        let mut this = Self {
            inner: sync_entry,
            mode: BackedEntryWriteMode::Sync,
        };
        this.conv_to_async().await?;
        Ok(this)
    }
}

impl<T: Serialize, Disk: AsyncWriteDisk> BackedEntryAsync<T, Disk> {
    /// Write the value to disk only, unloading current memory.
    ///
    /// See [`Self::write`] to keep the value in memory.
    pub async fn write_unload<U: Into<T>>(&mut self, new_value: U) -> bincode::Result<()> {
        self.unload();

        let mut write_disk = pin!(self.inner.disk.write_disk().await?);
        let mut bincode_writer = AsyncBincodeWriter::from(&mut write_disk);
        match self.mode {
            BackedEntryWriteMode::Async => {
                let mut bincode_writer = bincode_writer.for_async();
                bincode_writer.send(new_value.into()).await?;
                bincode_writer.flush().await?;
            }
            BackedEntryWriteMode::Sync => {
                bincode_writer.send(new_value.into()).await?;
                bincode_writer.flush().await?;
            }
        };

        write_disk.flush().await?;
        write_disk.shutdown().await?;
        Ok(())
    }
}

impl<T: DeserializeOwned, Disk: AsyncReadDisk> BackedEntryAsync<T, Disk> {
    /// Async version of [`BackedEntryArr::load`].
    ///
    /// Will use AsyncBincodeReader or BincodeReader depending on mode.
    /// Check that [`Self::get_mode`] is async for optimal read performance.
    /// The sync implementation will block the thread.
    pub async fn load(&mut self) -> Result<&T, Box<bincode::ErrorKind>> {
        if self.inner.value.is_none() {
            let mut read_disk = pin!(self.inner.disk.read_disk().await?);
            match self.mode {
                BackedEntryWriteMode::Sync => {
                    return Err(Box::new(bincode::ErrorKind::Custom(
                        "Encoded as sync, not async.".to_string(),
                    )));
                }
                BackedEntryWriteMode::Async => {
                    self.inner.value = Some(
                        AsyncBincodeReader::from(&mut read_disk)
                            .next()
                            .await
                            .ok_or(bincode::ErrorKind::Custom(
                                "AsyncBincodeReader stream empty".to_string(),
                            ))??,
                    )
                }
            }
        }
        Ok(self.inner.value.as_ref().unwrap())
    }
}

impl<T, Disk: for<'de> Deserialize<'de>> BackedEntryAsync<T, Disk> {
    pub fn is_loaded(&self) -> bool {
        self.inner.value.is_some()
    }
}

/// Async version of [`super::sync_impl::BackedEntryMut`].
///
/// Modifying by [`BackedEntryAsync::write`] writes the entire value to the
/// underlying storage on every modification. This allows for multiple values
/// values to be written before syncing with disk.
///
/// Call [`BackedEntryMutAsync::flush`] to sync with underlying storage before
/// dropping. Otherwise, drop panics.
pub struct BackedEntryMutAsync<
    T: Serialize,
    Disk: AsyncWriteDisk,
    E: AsMut<BackedEntryAsync<T, Disk>>,
> {
    entry: E,
    modified: bool,
    _phantom: (PhantomData<T>, PhantomData<Disk>),
}

impl<
        T: Serialize,
        Disk: AsyncWriteDisk,
        E: AsRef<BackedEntry<T, Disk>> + AsMut<BackedEntryAsync<T, Disk>>,
    > Deref for BackedEntryMutAsync<T, Disk, E>
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.entry.as_ref().value.as_ref().unwrap()
    }
}

impl<
        T: Serialize,
        Disk: AsyncWriteDisk,
        E: AsRef<BackedEntry<T, Disk>> + AsMut<BackedEntryAsync<T, Disk>>,
    > DerefMut for BackedEntryMutAsync<T, Disk, E>
{
    /// [`DerefMut::deref_mut`] that sets a modified flag.
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.modified = true;
        self.entry.as_mut().inner.value.as_mut().unwrap()
    }
}

impl<T: Serialize, Disk: AsyncWriteDisk, E: AsMut<BackedEntryAsync<T, Disk>>>
    BackedEntryMutAsync<T, Disk, E>
{
    /// Returns true if the memory version is desynced from the disk version.
    #[allow(dead_code)]
    pub fn is_modified(&self) -> bool {
        self.modified
    }

    /// Saves modifications to disk, unsetting the modified flag if sucessful.
    pub async fn flush(&mut self) -> bincode::Result<&mut Self> {
        self.entry.as_mut().update().await?;
        self.modified = false;
        Ok(self)
    }

    pub async fn conv_to_sync(&mut self) -> bincode::Result<&mut Self> {
        self.entry.as_mut().conv_to_sync().await?;
        Ok(self)
    }

    pub async fn conv_to_async(&mut self) -> bincode::Result<&mut Self> {
        self.entry.as_mut().conv_to_async().await?;
        Ok(self)
    }
}

impl<'a, T: Serialize, Disk: AsyncWriteDisk, E: AsMut<BackedEntryAsync<T, Disk>>> Drop
    for BackedEntryMutAsync<T, Disk, E>
{
    /// [`Drop::drop`] panics if the value isn't written to disk with
    /// [`Self::flush`].
    fn drop(&mut self) {
        assert!(!self.modified)
    }
}

impl<
        T: Serialize + DeserializeOwned,
        Disk: AsyncWriteDisk + AsyncReadDisk,
        E: AsMut<BackedEntryAsync<T, Disk>>,
    > BackedEntryMutAsync<T, Disk, E>
{
    /// Returns [`BackedEntryMutAsync`] to allow efficient in-memory modifications
    /// if variable-sized writes are safe for the underlying storage.
    ///
    /// Make sure to call [`BackedEntryMutAsync::flush`] to sync with disk before
    /// dropping.
    pub async fn mut_handle(mut backed: E) -> bincode::Result<BackedEntryMutAsync<T, Disk, E>> {
        backed.as_mut().load().await?;
        Ok(BackedEntryMutAsync {
            entry: backed,
            modified: false,
            _phantom: (PhantomData, PhantomData),
        })
    }
}

impl<T: Serialize + for<'de> Deserialize<'de>, Disk: AsyncWriteDisk + AsyncReadDisk>
    BackedEntryAsync<T, Disk>
{
    /// Convenience wrapper for [`BackedEntryMut::mut_handle`]
    pub async fn mut_handle(
        &mut self,
    ) -> bincode::Result<BackedEntryMutAsync<T, Disk, ToMut<Self>>> {
        BackedEntryMutAsync::mut_handle(ToMut(self)).await
    }
}

#[cfg(test)]
mod tests {

    use std::{collections::HashMap, io::Cursor};

    use crate::test_utils::CursorVec;

    use super::*;

    #[tokio::test]
    async fn mutate() {
        const FIB: &[u8] = &[0, 1, 1, 5, 7];
        let mut back_vec = CursorVec {
            inner: &mut Cursor::new(Vec::with_capacity(10)),
        };
        let back_vec_ptr: *mut CursorVec = &mut back_vec;

        // Intentional unsafe access to later peek underlying storage
        let mut backed_entry = unsafe { BackedEntryArrAsync::new(&mut *back_vec_ptr, None) };
        backed_entry.write_unload(FIB).await.unwrap();

        assert_eq!(backed_entry.load().await.unwrap().as_ref(), FIB);

        let backing_store = back_vec.inner.get_ref();
        assert_eq!(&backing_store[backing_store.len() - FIB.len()..], FIB);

        let mut handle = backed_entry.mut_handle().await.unwrap();
        handle[0] = 20;
        handle[2] = 30;

        let backing_store = back_vec.inner.get_ref();
        assert_eq!(backing_store[backing_store.len() - FIB.len()], FIB[0]);
        assert_eq!(handle[0], 20);
        assert_eq!(handle[2], 30);

        handle.flush().await.unwrap();
        let backing_store = back_vec.inner.get_ref();
        assert_eq!(backing_store[backing_store.len() - FIB.len()], 20);
        assert_eq!(backing_store[backing_store.len() - FIB.len() + 2], 30);
        assert_eq!(backing_store[backing_store.len() - FIB.len() + 1], FIB[1]);

        drop(handle);
        assert_eq!(
            backed_entry.load().await.unwrap().as_ref(),
            [20, 1, 30, 5, 7]
        );
    }

    #[tokio::test]
    async fn mutate_option() {
        let mut input: HashMap<String, u128> = HashMap::new();
        input.insert("THIS IS A STRING".to_string(), 55);
        input.insert("THIS IS ALSO A STRING".to_string(), 23413);

        let mut back_vec = CursorVec {
            inner: &mut Cursor::new(Vec::with_capacity(100)),
        };

        // Intentional unsafe access to later peek underlying storage
        let mut backed_entry = BackedEntryAsync::new(&mut back_vec, None);
        backed_entry.write_unload(input.clone()).await.unwrap();

        assert_eq!(&input, backed_entry.load().await.unwrap());
        let mut handle = backed_entry.mut_handle().await.unwrap();
        handle.insert("EXTRA STRING".to_string(), 234137);
        handle.flush().await.unwrap();

        drop(handle);
        assert_eq!(
            backed_entry
                .load()
                .await
                .unwrap()
                .get("EXTRA STRING")
                .unwrap(),
            &234137
        );
    }

    #[should_panic]
    #[tokio::test]
    async fn mutate_panic() {
        const FIB: &[u8] = &[0, 1, 1, 5, 7];
        let mut back_vec = CursorVec {
            inner: &mut Cursor::new(Vec::with_capacity(10)),
        };
        let back_vec_ptr: *mut CursorVec = &mut back_vec;

        // Intentional unsafe access to later peek underlying storage
        let mut backed_entry = unsafe { BackedEntryArrAsync::new(&mut *back_vec_ptr, None) };
        backed_entry.write_unload(FIB).await.unwrap();

        assert_eq!(backed_entry.load().await.unwrap().as_ref(), FIB);

        let backing_store = back_vec.inner.get_ref();
        assert_eq!(&backing_store[backing_store.len() - FIB.len()..], FIB);

        let mut handle = backed_entry.mut_handle().await.unwrap();
        handle[0] = 20;
    }
}
