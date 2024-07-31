use std::io::{Read, Write};

use serde::{Deserialize, Serialize};

#[cfg(feature = "async")]
use futures::io::{AsyncRead, AsyncWrite};

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
pub trait AsyncDecoder<Source: ?Sized + AsyncRead> {
    type Error: From<std::io::Error>;
    type T: for<'de> Deserialize<'de> + Send + Sync;
    fn decode(
        &self,
        source: &mut Source,
    ) -> impl std::future::Future<Output = Result<Self::T, Self::Error>> + Send;
}

#[cfg(feature = "async")]
pub trait AsyncEncoder<Target: ?Sized + AsyncWrite>: Unpin {
    type Error: From<std::io::Error>;
    type T: Serialize + Send + Sync;
    fn encode(
        &self,
        data: &Self::T,
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

#[cfg(feature = "csv")]
pub mod csv;
#[cfg(feature = "csv")]
pub use csv::CsvCoder;

#[cfg(feature = "async_csv")]
pub mod async_csv;
#[cfg(feature = "async_csv")]
pub use async_csv::AsyncCsvCoder;
