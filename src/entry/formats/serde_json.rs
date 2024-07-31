use std::io::{Read, Write};

use serde::{Deserialize, Serialize};

use super::{Decoder, Encoder};

#[cfg(feature = "simd_json")]
use super::SimdJsonCoder;

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

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
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

#[cfg(feature = "simd_json")]
impl From<SimdJsonCoder> for SerdeJsonCoder {
    fn from(_value: SimdJsonCoder) -> Self {
        Self {}
    }
}
