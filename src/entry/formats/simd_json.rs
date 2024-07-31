use std::io::{Read, Write};

use serde::{Deserialize, Serialize};

use super::{Decoder, Encoder};

#[cfg(feature = "simd_json")]
use super::SerdeJsonCoder;

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
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

#[cfg(feature = "serde_json")]
impl From<SerdeJsonCoder> for SimdJsonCoder {
    fn from(_value: SerdeJsonCoder) -> Self {
        Self {}
    }
}
