use std::{
    fs::File,
    io::{BufReader, BufWriter, Read, Write},
    marker::PhantomData,
    ops::{Deref, DerefMut},
    path::PathBuf,
    sync::OnceLock,
};

use itertools::Either;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::{io::AsyncWriteExt, sync::OnceCell};

use crate::utils::{Once, ToMut};

use super::{
    disks::{AsyncReadDisk, AsyncWriteDisk, ReadDisk, WriteDisk},
    formats::{AsyncDecoder, AsyncEncoder},
    BackedEntryAsync,
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

    /// See [`Self::change_disk`].
    pub async fn a_change_disk<OtherDisk>(
        self,
        disk: OtherDisk,
    ) -> Result<
        BackedEntryAsync<T, OtherDisk, Coder>,
        Either<
            <Coder as AsyncDecoder<<Disk as AsyncReadDisk>::ReadDisk>>::Error,
            <Coder as AsyncEncoder<<OtherDisk as AsyncWriteDisk>::WriteDisk>>::Error,
        >,
    >
    where
        OtherDisk: AsyncWriteDisk,
        Coder: AsyncEncoder<OtherDisk::WriteDisk>,
    {
        self.a_load().await.map_err(Either::Left)?;
        let mut other = BackedEntryAsync::<T, OtherDisk, Coder> {
            value: self.value,
            disk,
            coder: self.coder,
        };
        other.a_update().await.map_err(Either::Right)?;
        Ok(other)
    }

    /// See [`Self::change_encoder`].
    pub async fn a_change_encoder<OtherCoder>(
        self,
        coder: OtherCoder,
    ) -> Result<
        BackedEntryAsync<T, Disk, OtherCoder>,
        Either<
            <Coder as AsyncDecoder<<Disk as AsyncReadDisk>::ReadDisk>>::Error,
            <OtherCoder as AsyncEncoder<<Disk as AsyncWriteDisk>::WriteDisk>>::Error,
        >,
    >
    where
        Disk: AsyncWriteDisk,
        OtherCoder: AsyncEncoder<Disk::WriteDisk>,
    {
        self.a_load().await.map_err(Either::Left)?;
        let mut other = BackedEntryAsync::<T, Disk, OtherCoder> {
            value: self.value,
            disk: self.disk,
            coder,
        };
        other.a_update().await.map_err(Either::Right)?;
        Ok(other)
    }
}
