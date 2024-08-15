use serde::{Deserialize, Serialize};

/// A format decoder that can be used synchronously.
pub trait Decoder<Source: ?Sized> {
    type Error: From<std::io::Error>;
    type T: for<'de> Deserialize<'de> + ?Sized;

    /// Return data with a known format from storage.
    fn decode(&self, source: &mut Source) -> Result<Self::T, Self::Error>;
}

/// A format encoder that can be used synchronously.
///
/// # Implementation
/// It is the responsibility of trait implementors to call
/// [`WriteExt::flush`] to ensure encodings write out to disk. Any implementor
/// that does not to call these may fail to put all data on disk, making the
/// encoding invalid and causing read failure.
pub trait Encoder<Target: ?Sized> {
    type Error: From<std::io::Error>;
    type T: Serialize + ?Sized;

    /// Fully write out formatted data to a target disk.
    fn encode(&self, data: &Self::T, target: Target) -> Result<(), Self::Error>;
}

/// A format decoder that can be used asynchronously.
#[cfg(feature = "async")]
pub trait AsyncDecoder<Source: ?Sized> {
    type Error: From<std::io::Error>;
    type T: for<'de> Deserialize<'de> + Send + Sync;

    /// Return data with a known format from storage.
    fn decode(
        &self,
        source: Source,
    ) -> impl std::future::Future<Output = Result<Self::T, Self::Error>> + Send;
}

/// A format encoder that can be used asynchronously.
///
/// # Implementation
/// It is the responsibility of trait implementors to call
/// [`AsyncWriteExt::flush`] and [`AsyncWriteExt::close`] to ensure encodings
/// write out to disk. Any implementor that does not to call these may fail to
/// put all data on disk, making the encoding invalid and causing read failure.
#[cfg(feature = "async")]
pub trait AsyncEncoder<Target: ?Sized> {
    type Error: From<std::io::Error>;
    type T: ?Sized + Serialize + Send + Sync;

    /// Fully write out formatted data to a target disk.
    fn encode(
        &self,
        data: &Self::T,
        target: Target,
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
