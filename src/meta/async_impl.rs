use std::ops::{Deref, DerefMut};

use async_bincode::tokio::{AsyncBincodeReader, AsyncBincodeWriter};
use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use serde::{de::DeserializeOwned, Serialize};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

use crate::array::async_impl::BackedArray;

#[async_trait]
pub trait BackedArrayWrapper<T>:
    Deref<Target = BackedArray<T, Self::Storage>>
    + DerefMut
    + Serialize
    + DeserializeOwned
    + Sync
    + Send
{
    // Underlying storage struct
    type Storage;

    // Serial handling wrappers
    /// Wraps [`BackedArray::save_to_disk`] to include its own metadata.
    async fn save_to_disk<W: AsyncWrite + Unpin + Sync + Send>(
        &mut self,
        writer: &mut W,
    ) -> bincode::Result<()> {
        self.clear_memory();
        let mut bincode_writer = AsyncBincodeWriter::from(writer).for_async();
        bincode_writer.send(&self).await?;
        bincode_writer.get_mut().flush().await?;
        Ok(())
    }
    /// Wraps [`BackedArray::load`] to include its own metadata.
    async fn load<R: AsyncRead + Unpin + Sync + Send>(writer: &mut R) -> bincode::Result<Self> {
        AsyncBincodeReader::from(writer)
            .next()
            .await
            .ok_or(bincode::ErrorKind::Custom(
                "AsyncBincodeReader stream empty".to_string(),
            ))?
    }

    // Functionality wrappers
    type BackingError;

    /// Wraps [`BackedArray::remove`] to delete the file
    async fn remove(&mut self, entry_idx: usize) -> Result<&Self, Self::BackingError>;
    /// Wraps [`BackedArray::append`] to create backing storage
    async fn append(&mut self, values: &[T]) -> bincode::Result<&Self>;
    /// Wraps [`BackedArray::append_memory`] to create backing storage
    async fn append_memory(&mut self, values: Box<[T]>) -> bincode::Result<&Self>;

    /// Moves all entries of `rhs` into `self`
    async fn append_array(&mut self, rhs: Self) -> Result<&Self, Self::BackingError>;
}
