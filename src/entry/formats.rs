use std::io::{Read, Write};

use serde::{Deserialize, Serialize};

#[cfg(feature = "async")]
use tokio::io::{AsyncRead, AsyncWrite};

pub trait Decoder<Source: Read> {
    type Error: From<std::io::Error>;
    fn decode<T: for<'de> Deserialize<'de>>(&self, source: &mut Source) -> Result<T, Self::Error>;
}

pub trait Encoder<Target: Write> {
    type Error: From<std::io::Error>;
    fn encode<T: Serialize>(&self, data: &T, target: &mut Target) -> Result<(), Self::Error>;
}

#[cfg(feature = "async")]
pub trait AsyncDecoder<Source: AsyncRead> {
    type Error: From<std::io::Error>;
    fn decode<T: for<'de> Deserialize<'de> + Send + Sync>(
        &self,
        source: &mut Source,
    ) -> impl std::future::Future<Output = Result<T, Self::Error>> + Send;
}

#[cfg(feature = "async")]
pub trait AsyncEncoder<Target: AsyncWrite>: Unpin {
    type Error: From<std::io::Error>;
    fn encode<T: Serialize + Send + Sync>(
        &self,
        data: &T,
        target: &mut Target,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send;
}

#[cfg(feature = "bincode")]
pub use bincode_formats::*;
#[cfg(feature = "bincode")]
mod bincode_formats {
    use super::*;

    use bincode::Options;

    #[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
    pub struct BincodeCoder {}

    impl<Source: Read> Decoder<Source> for BincodeCoder {
        type Error = bincode::Error;
        fn decode<T: for<'de> serde::Deserialize<'de>>(
            &self,
            source: &mut Source,
        ) -> Result<T, Self::Error> {
            bincode::options()
                .with_limit(u32::MAX as u64)
                .allow_trailing_bytes()
                .deserialize_from(source)
        }
    }

    impl<Target: Write> Encoder<Target> for BincodeCoder {
        type Error = bincode::Error;
        fn encode<T: Serialize>(&self, data: &T, target: &mut Target) -> Result<(), Self::Error> {
            bincode::options()
                .with_limit(u32::MAX as u64)
                .allow_trailing_bytes()
                .serialize_into(target, data)
        }
    }
}

#[cfg(feature = "async_bincode")]
pub use bincode_formats_async::*;
#[cfg(feature = "async_bincode")]
mod bincode_formats_async {
    use async_bincode::tokio::{AsyncBincodeReader, AsyncBincodeWriter};
    use futures::{SinkExt, StreamExt};
    use tokio::io::AsyncWriteExt;

    use super::*;

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
            bincode_writer.send(data).await?;
            bincode_writer.get_mut().flush().await?;
            bincode_writer.get_mut().shutdown().await?;
            Ok(())
        }
    }
}

#[cfg(feature = "serde_json")]
pub use serde_json_formats::*;
#[cfg(feature = "serde_json")]
mod serde_json_formats {
    #[derive(Debug)]
    pub enum SerdeJsonErr {
        SerdeJson(serde_json::Error),
        IO(std::io::Error),
    }

    impl From<serde_json::Error> for SerdeJsonErr {
        fn from(value: serde_json::Error) -> Self {
            Self::SerdeJson(value)
        }
    }
    impl From<std::io::Error> for SerdeJsonErr {
        fn from(value: std::io::Error) -> Self {
            Self::IO(value)
        }
    }

    use super::*;

    #[derive(Debug, Default, Clone, Copy)]
    pub struct SerdeJsonCoder {}

    impl<Source: Read> Decoder<Source> for SerdeJsonCoder {
        type Error = SerdeJsonErr;
        fn decode<T: for<'de> serde::Deserialize<'de>>(
            &self,
            source: &mut Source,
        ) -> Result<T, Self::Error> {
            serde_json::from_reader(source).map_err(|e| e.into())
        }
    }
    impl<Target: Write> Encoder<Target> for SerdeJsonCoder {
        type Error = SerdeJsonErr;
        fn encode<T: Serialize>(&self, data: &T, target: &mut Target) -> Result<(), Self::Error> {
            serde_json::to_writer_pretty(target, data).map_err(|e| e.into())
        }
    }
}

#[cfg(feature = "simd_json")]
pub use simd_json_formats::*;
#[cfg(feature = "simd_json")]
mod simd_json_formats {
    use super::*;

    #[derive(Debug, Default, Clone, Copy)]
    pub struct SimdJsonCoder {}

    impl<Source: Read> Decoder<Source> for SimdJsonCoder {
        type Error = simd_json::Error;
        fn decode<T: for<'de> serde::Deserialize<'de>>(
            &self,
            source: &mut Source,
        ) -> Result<T, Self::Error> {
            simd_json::from_reader(source)
        }
    }
    impl<Target: Write> Encoder<Target> for SimdJsonCoder {
        type Error = simd_json::Error;
        fn encode<T: Serialize>(&self, data: &T, target: &mut Target) -> Result<(), Self::Error> {
            simd_json::to_writer_pretty(target, data)
        }
    }
}

#[cfg(all(feature = "serde_json", feature = "simd_json"))]
impl From<SerdeJsonCoder> for SimdJsonCoder {
    fn from(_value: SerdeJsonCoder) -> Self {
        Self {}
    }
}

#[cfg(all(feature = "serde_json", feature = "simd_json"))]
impl From<SimdJsonCoder> for SerdeJsonCoder {
    fn from(_value: SimdJsonCoder) -> Self {
        Self {}
    }
}
