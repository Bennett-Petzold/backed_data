use std::{
    io::{Read, Write},
    marker::PhantomData,
};

use serde::{Deserialize, Serialize};

use super::{Decoder, Encoder};

#[cfg(feature = "serde_json")]
use super::SerdeJsonCoder;

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct SimdJsonCoder<T: ?Sized> {
    _phantom_data: PhantomData<T>,
}

impl<T: ?Sized + for<'de> Deserialize<'de>, Source: Read> Decoder<Source> for SimdJsonCoder<T> {
    type Error = simd_json::Error;
    type T = T;
    fn decode(&self, source: &mut Source) -> Result<Self::T, Self::Error> {
        simd_json::from_reader(source)
    }
}
impl<T: ?Sized + Serialize, Target: Write> Encoder<Target> for SimdJsonCoder<T> {
    type Error = simd_json::Error;
    type T = T;
    fn encode(&self, data: &Self::T, target: &mut Target) -> Result<(), Self::Error> {
        simd_json::to_writer_pretty(target, data)
    }
}

#[cfg(feature = "serde_json")]
impl<T> From<SerdeJsonCoder<T>> for SimdJsonCoder<T> {
    fn from(_value: SerdeJsonCoder<T>) -> Self {
        Self {
            _phantom_data: PhantomData,
        }
    }
}
