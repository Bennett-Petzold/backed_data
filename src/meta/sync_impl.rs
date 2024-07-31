use std::{
    io::{Read, Seek, Write},
    ops::{Deref, DerefMut},
};

use bincode::{deserialize_from, serialize_into};
use serde::{de::DeserializeOwned, Serialize};

use crate::array::sync_impl::BackedArray;

pub trait BackedArrayWrapper<T>:
    Deref<Target = BackedArray<T, Self::Storage>> + DerefMut + Serialize + DeserializeOwned
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
    fn remove(&mut self, entry_idx: usize) -> Result<&mut Self, Self::BackingError>;
    fn append(&mut self, values: &[T]) -> bincode::Result<&mut Self>;
    fn append_memory(&mut self, values: Box<[T]>) -> bincode::Result<&mut Self>;

    /// Moves all entries of same-type `rhs` into `self`.
    fn append_array(&mut self, rhs: Self) -> Result<&mut Self, Self::BackingError>;

    /// Removes this store from disk and memory entirely.
    fn delete(self) -> Result<(), Self::BackingError>;
}

/// Copies all values from `rhs` to the `lhs` store.
///
/// Removes values from `rhs` after adding them to the `lhs` backing, in order
/// to minimize memory usage.
pub fn append_wrapper<'a, T, LHS, RHS>(
    lhs: &'a mut LHS,
    rhs: &'a mut RHS,
) -> bincode::Result<&'a mut LHS>
where
    LHS: BackedArrayWrapper<T>,
    RHS: BackedArrayWrapper<T>,
    T: Serialize + DeserializeOwned,
    RHS::Storage: Read + Seek,
    <RHS as BackedArrayWrapper<T>>::BackingError: 'static,
{
    let idxs = 0..rhs.len();
    for idx in idxs {
        lhs.append(rhs.get_chunk(idx).unwrap()?)?;
        rhs.clear_chunk(idx);
    }
    Ok(lhs)
}

/// [`append_wrapper`] that removes the rhs disk representation.
pub fn append_wrapper_destructive<T, LHS, RHS>(
    lhs: &mut LHS,
    mut rhs: RHS,
) -> anyhow::Result<&mut LHS>
where
    LHS: BackedArrayWrapper<T>,
    RHS: BackedArrayWrapper<T>,
    T: Sized + Serialize + DeserializeOwned,
    RHS::Storage: Read + Seek + Send + Sync,
    <RHS as BackedArrayWrapper<T>>::BackingError: Send + Sync + std::error::Error + 'static,
{
    while let Some(val) = rhs.get_chunk(0) {
        let val = val?;
        lhs.append(val)?;
        rhs.remove(0)?;
    }
    Ok(lhs)
}
