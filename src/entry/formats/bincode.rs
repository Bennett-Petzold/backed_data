use std::io::{Read, Write};

use bincode::Options;
use serde::{Deserialize, Serialize};

use super::{Decoder, Encoder};

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
