use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

use itertools::Either;
use serde::{Deserialize, Serialize};
use tokio::{io::AsyncWriteExt, sync::OnceCell};

use crate::utils::Once;

use super::{
    disks::{AsyncReadDisk, AsyncWriteDisk, ReadDisk, WriteDisk},
    formats::{AsyncDecoder, AsyncEncoder, Decoder, Encoder},
    BackedEntry, BackedEntryAsync, BackedEntryTrait,
};

impl<T: Serialize, Disk: AsyncWriteDisk, Coder: AsyncEncoder<Disk::WriteDisk>>
    BackedEntryAsync<T, Disk, Coder>
{
    /// See [`Self::update`].
    pub async fn a_update(&mut self) -> Result<(), Coder::Error> {
        if let Some(val) = self.value.get() {
            let mut disk = self.disk.async_write_disk().await?;
            self.coder.encode(val, &mut disk).await?;

            // Make sure buffer is emptied
            disk.flush().await?;
            disk.shutdown().await?;
        }
        Ok(())
    }

    /// See [`Self::write`].
    pub async fn a_write(&mut self, new_value: T) -> Result<(), Coder::Error> {
        let mut disk = self.disk.async_write_disk().await?;
        self.coder.encode(&new_value, &mut disk).await?;

        // Make sure buffer is emptied
        disk.flush().await?;
        disk.shutdown().await?;

        // Drop previous value and write in new.
        // value.set() only works when uninitialized.
        self.value = OnceCell::new();
        let _ = self.value.set(new_value);
        Ok(())
    }
}

impl<T: for<'de> Deserialize<'de>, Disk: AsyncReadDisk, Coder: AsyncDecoder<Disk::ReadDisk>>
    BackedEntryAsync<T, Disk, Coder>
{
    /// See [`Self::load`].
    pub async fn a_load(&self) -> Result<&T, Coder::Error> {
        let value = match self.value.get() {
            Some(x) => x,
            None => {
                let mut disk = self.disk.async_read_disk().await?;
                let val = self.coder.decode(&mut disk).await?;
                self.value.get_or_init(|| async { val }).await
            }
        };
        Ok(value)
    }
}

impl<T: Serialize, Disk: AsyncWriteDisk, Coder: AsyncEncoder<Disk::WriteDisk>>
    BackedEntryAsync<T, Disk, Coder>
{
    /// [`Self::write_unload`].
    pub async fn a_write_unload<U: Into<T>>(&mut self, new_value: U) -> Result<(), Coder::Error> {
        self.unload();
        let mut disk = self.disk.async_write_disk().await?;
        self.coder.encode(&new_value.into(), &mut disk).await?;
        // Make sure buffer is emptied
        disk.flush().await?;
        disk.shutdown().await?;
        Ok(())
    }
}

pub trait OnceCellWrap {
    type T;

    fn get_cell(self) -> OnceCell<Self::T>;
    fn get_cell_ref(&self) -> &OnceCell<Self::T>;
    fn get_cell_mut(&mut self) -> &mut OnceCell<Self::T>;
}

impl<T> OnceCellWrap for OnceCell<T> {
    type T = T;

    fn get_cell(self) -> OnceCell<Self::T> {
        self
    }
    fn get_cell_ref(&self) -> &OnceCell<Self::T> {
        self
    }
    fn get_cell_mut(&mut self) -> &mut OnceCell<Self::T> {
        self
    }
}

/// [`BackedEntryTrait`] that can be asynchronously written to.
pub trait BackedEntryAsyncWrite:
    BackedEntryTrait<
    T: OnceCellWrap<T: Serialize>,
    Disk: AsyncWriteDisk,
    Coder: AsyncEncoder<
        <<Self as BackedEntryTrait>::Disk as AsyncWriteDisk>::WriteDisk,
        Error = Self::WriteError,
    >,
>
{
    type WriteError;
    fn get_inner_mut(
        &mut self,
    ) -> &mut BackedEntryAsync<<Self::T as OnceCellWrap>::T, Self::Disk, Self::Coder>;
}

impl<
        U: Serialize,
        E: BackedEntryTrait<
            T = OnceCell<U>,
            Disk: AsyncWriteDisk,
            Coder: AsyncEncoder<<E::Disk as AsyncWriteDisk>::WriteDisk>,
        >,
    > BackedEntryAsyncWrite for E
{
    type WriteError = <E::Coder as AsyncEncoder<<E::Disk as AsyncWriteDisk>::WriteDisk>>::Error;
    fn get_inner_mut(&mut self) -> &mut BackedEntryAsync<U, Self::Disk, Self::Coder> {
        BackedEntryTrait::get_mut(self)
    }
}

/// [`BackedEntryTrait`] that can be read asynchronously.
pub trait BackedEntryAsyncRead:
    BackedEntryTrait<
    T: OnceCellWrap<T: for<'de> Deserialize<'de>>,
    Disk: AsyncReadDisk,
    Coder: AsyncDecoder<
        <<Self as BackedEntryTrait>::Disk as AsyncReadDisk>::ReadDisk,
        Error = Self::ReadError,
    >,
>
{
    type ReadError;
    fn get_inner_ref(
        &self,
    ) -> &BackedEntryAsync<<Self::T as OnceCellWrap>::T, Self::Disk, Self::Coder>;
}

impl<
        U: for<'de> Deserialize<'de>,
        E: BackedEntryTrait<
            T = OnceCell<U>,
            Disk: AsyncReadDisk,
            Coder: AsyncDecoder<<E::Disk as AsyncReadDisk>::ReadDisk>,
        >,
    > BackedEntryAsyncRead for E
{
    type ReadError = <E::Coder as AsyncDecoder<<E::Disk as AsyncReadDisk>::ReadDisk>>::Error;
    fn get_inner_ref(&self) -> &BackedEntryAsync<U, Self::Disk, Self::Coder> {
        BackedEntryTrait::get_ref(self)
    }
}

/// Gives mutable handle to a backed entry.
///
/// Modifying by [`BackedEntryAsync::a_write`] writes the entire value to the
/// underlying storage on every modification. This allows for multiple values
/// values to be written before syncing with disk.
///
/// Call [`Self::flush`] to sync changes with underlying storage before
/// dropping. Otherwise, this causes a panic.
pub struct BackedEntryAsyncMut<'a, E> {
    entry: &'a mut E,
    modified: bool,
}

impl<E: BackedEntryAsyncRead> Deref for BackedEntryAsyncMut<'_, E> {
    type Target = E::T;

    fn deref(&self) -> &Self::Target {
        //self.entry.get_inner_ref().value.get().unwrap()
        todo!()
    }
}

impl<E: BackedEntryAsyncRead + BackedEntryAsyncWrite> DerefMut for BackedEntryAsyncMut<'_, E> {
    /// [`DerefMut::deref_mut`] that sets a modified flag.
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.modified = true;
        //self.entry.get_inner_mut().value.get_mut().unwrap()
        todo!()
    }
}

impl<E: BackedEntryAsyncWrite> BackedEntryAsyncMut<'_, E> {
    /// Returns true if the memory version is desynced from the disk version
    #[allow(dead_code)]
    pub fn is_modified(&self) -> bool {
        self.modified
    }

    /// Saves modifications to disk, unsetting the modified flag if sucessful.
    pub async fn flush(&mut self) -> Result<&mut Self, E::WriteError> {
        self.entry.get_inner_mut().a_update().await?;
        self.modified = false;
        Ok(self)
    }
}

impl<E> Drop for BackedEntryAsyncMut<'_, E> {
    /// [`Drop::drop`] panics if the handle is dropped while modified.
    /// Flush before dropping to avoid a panic.
    fn drop(&mut self) {
        if self.modified {
            panic!("BackedEntryAsyncMut dropped while modified.");
        }
    }
}

impl<'a, E: BackedEntryAsyncRead> BackedEntryAsyncMut<'a, E> {
    /// Returns [`BackedEntryAsyncMut`] to allow efficient in-memory modifications.
    ///
    /// Make sure to call [`BackedEntryAsyncMut::flush`] to sync with disk before
    /// dropping. Unlike the sync implementation, this will always panic if not
    /// synced instead of attempting a recovery.
    pub async fn mut_handle(backed: &'a mut E) -> Result<Self, E::ReadError> {
        backed.get_inner_ref().a_load().await?;
        Ok(BackedEntryAsyncMut {
            entry: backed,
            modified: false,
        })
    }
}

impl<
        T: Serialize + for<'de> Deserialize<'de>,
        Disk: AsyncWriteDisk + AsyncReadDisk,
        Coder: AsyncEncoder<Disk::WriteDisk> + AsyncDecoder<Disk::ReadDisk>,
    > BackedEntryAsync<T, Disk, Coder>
{
    /// Convenience wrapper for [`BackedEntryAsyncMut::mut_handle`]
    pub async fn a_mut_handle(
        &mut self,
    ) -> Result<BackedEntryAsyncMut<Self>, <Coder as AsyncDecoder<Disk::ReadDisk>>::Error> {
        BackedEntryAsyncMut::mut_handle(self).await
    }
}

impl<
        T: for<'de> Deserialize<'de> + Serialize,
        Disk: AsyncReadDisk,
        Coder: AsyncDecoder<Disk::ReadDisk>,
    > BackedEntryAsync<T, Disk, Coder>
{
    /// See [`Self::change_backing`].
    pub async fn a_change_backing<OtherDisk, OtherCoder>(
        self,
        disk: OtherDisk,
        coder: OtherCoder,
    ) -> Result<BackedEntryAsync<T, OtherDisk, OtherCoder>, Either<Coder::Error, OtherCoder::Error>>
    where
        OtherDisk: AsyncWriteDisk,
        OtherCoder: AsyncEncoder<OtherDisk::WriteDisk>,
    {
        self.a_load().await.map_err(Either::Left)?;
        let mut other = BackedEntryAsync::<T, OtherDisk, OtherCoder> {
            value: self.value,
            disk,
            coder,
        };
        other.a_update().await.map_err(Either::Right)?;
        Ok(other)
    }

    /// Converts [`self`] into a synchronous backing.
    ///
    /// Makes a blocking runtime call to update disk after conversion,
    /// so will crash if not run within a Tokio runtime.
    pub async fn to_sync<OtherDisk, OtherCoder, U>(
        self,
        disk: OtherDisk,
        coder: OtherCoder,
    ) -> Result<BackedEntry<U, OtherDisk, OtherCoder>, Either<Coder::Error, OtherCoder::Error>>
    where
        <Coder as AsyncDecoder<<Disk as AsyncReadDisk>::ReadDisk>>::Error: Send + Sync,
        U: Once<Inner = T> + Send + Sync + 'static,
        OtherDisk: WriteDisk + Send + Sync + 'static,
        OtherCoder: Encoder<OtherDisk::WriteDisk, Error: Send + Sync> + Send + Sync + 'static,
    {
        self.a_load().await.map_err(Either::Left)?;

        let value = U::new();
        let _ = value.set(self.value.into_inner().unwrap());

        let mut other = BackedEntry::<U, OtherDisk, OtherCoder> { value, disk, coder };
        let other = tokio::task::spawn_blocking(move || {
            other.update()?;
            Ok::<_, OtherCoder::Error>(other)
        })
        .await
        .unwrap()
        .map_err(Either::Right)?;
        Ok(other)
    }
}

impl<
        T: Serialize + for<'de> Deserialize<'de> + Sync + Send + 'static,
        Disk: AsyncWriteDisk,
        Coder: AsyncEncoder<Disk::WriteDisk>,
    > BackedEntryAsync<T, Disk, Coder>
{
    /// Converts from a synchronous backing into [`self`].
    ///
    /// Makes a blocking runtime call to load from disk before conversion,
    /// so will crash if not run within a Tokio runtime.
    pub async fn from_sync<OtherDisk, OtherCoder, U>(
        other: BackedEntry<U, OtherDisk, OtherCoder>,
        disk: Disk,
        coder: Coder,
    ) -> Result<Self, Either<OtherCoder::Error, Coder::Error>>
    where
        U: Once<Inner = T> + Send + Sync + 'static,
        OtherDisk: ReadDisk + Send + Sync + 'static,
        OtherCoder: Decoder<OtherDisk::ReadDisk, Error: Send + Sync> + Send + Sync + 'static,
    {
        let other = Arc::new(other);
        let other_clone = other.clone();
        tokio::task::spawn_blocking(move || other_clone.load().map(|_| ()))
            .await
            .unwrap()
            .map_err(Either::Left)?;
        let other = Arc::into_inner(other).unwrap();

        let mut this = Self {
            value: OnceCell::new_with(other.value.into_inner()),
            disk,
            coder,
        };

        this.a_update().await.map_err(Either::Right)?;
        Ok(this)
    }
}
