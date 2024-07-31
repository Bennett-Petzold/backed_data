use std::io::{Read, Write};

use serde::{Deserialize, Serialize};

#[cfg(feature = "async")]
use tokio::io::{AsyncRead, AsyncWrite};

pub trait Decoder<Source: ?Sized + Read> {
    type Error: From<std::io::Error>;
    type T: for<'de> Deserialize<'de> + ?Sized;
    fn decode(&self, source: &mut Source) -> Result<Self::T, Self::Error>;
}

pub trait Encoder<Target: ?Sized + Write> {
    type Error: From<std::io::Error>;
    type T: Serialize + ?Sized;
    fn encode(&self, data: &Self::T, target: &mut Target) -> Result<(), Self::Error>;
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
mod bincode;
#[cfg(feature = "bincode")]
pub use bincode::*;

#[cfg(feature = "async_bincode")]
mod async_bincode;
#[cfg(feature = "async_bincode")]
pub use async_bincode::*;

#[cfg(feature = "serde_json")]
mod serde_json;
#[cfg(feature = "serde_json")]
pub use serde_json::*;

#[cfg(feature = "simd_json")]
mod simd_json;
#[cfg(feature = "simd_json")]
pub use simd_json::*;

#[cfg(any(feature = "csv", feature = "async_csv"))]
mod csv;
#[cfg(any(feature = "csv", feature = "async_csv"))]
pub use csv::*;

#[cfg(feature = "csv")]
mod async_csv;
