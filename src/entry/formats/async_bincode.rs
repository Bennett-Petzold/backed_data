use std::marker::PhantomData;

use async_bincode::futures::{AsyncBincodeReader, AsyncBincodeWriter};
use futures::io::{AsyncRead, AsyncWrite};
use futures::{AsyncWriteExt, SinkExt, StreamExt};
use serde::{Deserialize, Serialize};

use super::{AsyncDecoder, AsyncEncoder};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AsyncBincodeCoder<T: ?Sized> {
    _phantom_data: PhantomData<T>,
}

impl<T: ?Sized> Default for AsyncBincodeCoder<T> {
    fn default() -> Self {
        Self {
            _phantom_data: PhantomData,
        }
    }
}

impl<T, Source> AsyncDecoder<Source> for AsyncBincodeCoder<T>
where
    T: for<'de> Deserialize<'de> + Send + Sync,
    Source: AsyncRead + Send + Sync + Unpin,
{
    type Error = bincode::Error;
    type T = T;
    async fn decode(&self, source: Source) -> Result<Self::T, Self::Error> {
        AsyncBincodeReader::from(source)
            .next()
            .await
            .ok_or(bincode::ErrorKind::Custom(
                "AsyncBincodeReader stream empty".to_string(),
            ))?
    }
}

impl<T, Target> AsyncEncoder<Target> for AsyncBincodeCoder<T>
where
    T: Serialize + Send + Sync + Unpin,
    Target: AsyncWrite + Send + Sync + Unpin,
    for<'a> &'a mut Target: AsyncWrite,
{
    type Error = bincode::Error;
    type T = T;
    async fn encode(&self, data: &Self::T, mut target: Target) -> Result<(), Self::Error> {
        let mut bincode_writer = AsyncBincodeWriter::from(&mut target).for_async();
        bincode_writer.send(data).await?;
        target.flush().await?;
        target.close().await?;
        Ok(())
    }
}
