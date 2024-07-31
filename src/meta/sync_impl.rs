use std::{
    io::{Read, Write},
    ops::{Deref, DerefMut},
};

use bincode::{deserialize_from, serialize_into};
use serde::{de::DeserializeOwned, Serialize};

use crate::array::sync_impl::BackedArray;

pub trait BackedArrayWrapper<T>:
    Sized + Deref<Target = BackedArray<T, Self::Storage>> + DerefMut + Serialize + DeserializeOwned
{
    /// Underlying storage struct
    type Storage;

    // Serial handling wrappers with default implementations
    /// Wraps [`BackedArray::save_to_disk`] to include its own metadata.
    fn save_to_disk<W: Write>(&mut self, writer: W) -> bincode::Result<()> {
        self.deref_mut().clear_memory();
        serialize_into(writer, self)
    }
    /// Wraps [`BackedArray::load`] to include its own metadata.
    fn load<R: Read>(reader: R) -> bincode::Result<Self> {
        deserialize_from(reader)
    }

    type BackingError;
    // Functionality wrappers
    fn remove(&mut self, entry_idx: usize) -> Result<&Self, Self::BackingError>;
    fn append(&mut self, values: &[T]) -> bincode::Result<&Self>;
    fn append_memory(&mut self, values: Box<[T]>) -> bincode::Result<&Self>;

    /// Moves all entries of `rhs` into `self`
    fn append_array(&mut self, rhs: Self) -> Result<&Self, Self::BackingError>;
}
