/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use std::{
    io::{Read, Write},
    marker::PhantomData,
};

use bincode::Options;
use serde::{Deserialize, Serialize};

use super::{Decoder, Encoder};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BincodeCoder<T: ?Sized> {
    _phantom_data: PhantomData<T>,
}

impl<T: ?Sized> Default for BincodeCoder<T> {
    fn default() -> Self {
        Self {
            _phantom_data: PhantomData,
        }
    }
}

impl<T: ?Sized + for<'de> Deserialize<'de>, Source: Read> Decoder<Source> for BincodeCoder<T> {
    type Error = bincode::Error;
    type T = T;
    fn decode(&self, source: &mut Source) -> Result<Self::T, Self::Error> {
        bincode::options()
            .with_limit(u32::MAX as u64)
            .allow_trailing_bytes()
            .deserialize_from(source)
    }
}

impl<T: ?Sized + Serialize, Target: Write> Encoder<Target> for BincodeCoder<T>
where
    for<'a> &'a mut Target: Write,
{
    type Error = bincode::Error;
    type T = T;
    fn encode(&self, data: &Self::T, mut target: Target) -> Result<(), Self::Error> {
        bincode::options()
            .with_limit(u32::MAX as u64)
            .allow_trailing_bytes()
            .serialize_into(&mut target, data)?;
        target.flush()?;
        Ok(())
    }
}
