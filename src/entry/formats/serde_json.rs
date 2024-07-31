use std::{
    io::{Read, Write},
    marker::PhantomData,
};

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
pub struct SerdeJsonCoder<T: ?Sized> {
    _phantom_data: PhantomData<T>,
}

impl<T: ?Sized + for<'de> serde::Deserialize<'de>, Source: Read> Decoder<Source>
    for SerdeJsonCoder<T>
{
    type Error = SerdeJsonErr;
    type T = T;
    fn decode(&self, source: &mut Source) -> Result<Self::T, Self::Error> {
        serde_json::from_reader(source).map_err(|e| e.into())
    }
}
impl<T: ?Sized + Serialize, Target: Write> Encoder<Target> for SerdeJsonCoder<T> {
    type Error = SerdeJsonErr;
    type T = T;
    fn encode(&self, data: &Self::T, target: &mut Target) -> Result<(), Self::Error> {
        serde_json::to_writer_pretty(target, data).map_err(|e| e.into())
    }
}

#[cfg(feature = "simd_json")]
impl<T> From<SimdJsonCoder<T>> for SerdeJsonCoder<T> {
    fn from(_value: SimdJsonCoder<T>) -> Self {
        Self {
            _phantom_data: PhantomData,
        }
    }
}
