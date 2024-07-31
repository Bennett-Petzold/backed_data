use async_bincode::tokio::{AsyncBincodeReader, AsyncBincodeWriter};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};

use super::{AsyncDecoder, AsyncEncoder};

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct AsyncBincodeCoder {}

impl<Source: AsyncRead + Send + Sync + Unpin> AsyncDecoder<Source> for AsyncBincodeCoder {
    type Error = bincode::Error;
    async fn decode<T: for<'de> Deserialize<'de> + Send + Sync>(
        &self,
        source: &mut Source,
    ) -> Result<T, Self::Error> {
        AsyncBincodeReader::from(source)
            .next()
            .await
            .ok_or(bincode::ErrorKind::Custom(
                "AsyncBincodeReader stream empty".to_string(),
            ))?
    }
}

impl<Target: AsyncWrite + Send + Sync + Unpin> AsyncEncoder<Target> for AsyncBincodeCoder {
    type Error = bincode::Error;
    async fn encode<T: Serialize + Send + Sync>(
        &self,
        data: &T,
        target: &mut Target,
    ) -> Result<(), Self::Error> {
        let mut bincode_writer = AsyncBincodeWriter::from(target).for_async();
        bincode_writer.send(data).await
    }
}
