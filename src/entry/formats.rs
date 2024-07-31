use std::io::{Read, Write};

use serde::{Deserialize, Serialize};

pub trait Decoder<Source: Read> {
    type Error: From<std::io::Error>;
    fn decode<T: for<'de> Deserialize<'de>>(&self, source: &mut Source) -> Result<T, Self::Error>;
}

pub trait Encoder<Target: Write> {
    type Error: From<std::io::Error>;
    fn encode<T: Serialize>(&self, data: &T, target: &mut Target) -> Result<(), Self::Error>;
}

#[cfg(feature = "bincode")]
pub use bincode_formats::*;
#[cfg(feature = "bincode")]
mod bincode_formats {
    use super::*;

    use bincode::Options;

    #[derive(Debug, Default, Clone, Copy)]
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
            simd_json::from_reader(source).map_err(|e| e.into())
        }
    }
    impl<Target: Write> Encoder<Target> for SimdJsonCoder {
        type Error = simd_json::Error;
        fn encode<T: Serialize>(&self, data: &T, target: &mut Target) -> Result<(), Self::Error> {
            simd_json::to_writer_pretty(target, data).map_err(|e| e.into())
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
