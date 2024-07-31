use std::marker::PhantomData;

use async_bincode::tokio::{AsyncBincodeReader, AsyncBincodeWriter};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};

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
    async fn decode(&self, source: &mut Source) -> Result<Self::T, Self::Error> {
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
{
    type Error = bincode::Error;
    type T = T;
    async fn encode(&self, data: &Self::T, target: &mut Target) -> Result<(), Self::Error> {
        let mut bincode_writer = AsyncBincodeWriter::from(target).for_async();
        bincode_writer.send(data).await
    }
}
