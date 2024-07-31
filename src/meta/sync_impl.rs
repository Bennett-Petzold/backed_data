use std::{
    io::{Read, Write},
    ops::{Deref, DerefMut},
};

use bincode::{deserialize_from, serialize_into};
use serde::{de::DeserializeOwned, Serialize};

use crate::{
    array::sync_impl::BackedArray,
    entry::sync_impl::{ReadDisk, WriteDisk},
};

pub trait BackedArrayWrapper<T>:
    Deref<Target = BackedArray<T, Self::Storage>> + DerefMut + Serialize + DeserializeOwned
{
    /// Underlying storage struct
    type Storage: ReadDisk + WriteDisk;

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
    <RHS as BackedArrayWrapper<T>>::BackingError: 'static,
{
    let idxs = 0..rhs.chunks_len();
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
    <RHS as BackedArrayWrapper<T>>::BackingError: Send + Sync + std::error::Error + 'static,
{
    while let Some(val) = rhs.get_chunk(0) {
        let val = val?;
        lhs.append(val)?;
        rhs.remove(0)?;
    }
    Ok(lhs)
}

#[cfg(test)]
mod tests {
    use std::{
        env::temp_dir,
        fs::{create_dir, read_dir, remove_dir_all},
    };

    use itertools::Itertools;

    use crate::{directory::sync_impl::DirectoryBackedArray, zstd::sync_impl::ZstdDirBackedArray};

    use super::*;

    #[test]
    fn appending() {
        let first_sequence = [0, 1, 1, 2];
        let second_sequence = [3, 5, 7, 12];

        let dir = temp_dir().join("appending_test");
        let _ = remove_dir_all(dir.clone());
        let _ = create_dir(dir.clone());

        let dir2 = temp_dir().join("appending_test_second");
        let _ = remove_dir_all(dir2.clone());
        let _ = create_dir(dir2.clone());

        let mut first_store = ZstdDirBackedArray::new(dir.clone(), None).unwrap();
        let mut second_store = DirectoryBackedArray::new(dir2.clone()).unwrap();
        first_store.append(&first_sequence).unwrap();
        second_store.append(&second_sequence).unwrap();

        append_wrapper(&mut first_store, &mut second_store).unwrap();
        assert_eq!(
            first_store
                .item_iter(0)
                .map(|x| x.unwrap().to_owned())
                .collect_vec(),
            [first_sequence, second_sequence].concat()
        );
        assert_eq!(
            read_dir(dir.clone())
                .unwrap()
                .map(|x| { x.unwrap() })
                .collect_vec()
                .len(),
            2
        );

        assert_eq!(
            second_store
                .item_iter(0)
                .map(|x| x.unwrap().to_owned())
                .collect_vec(),
            second_sequence
        );
        assert_eq!(
            read_dir(dir2.clone())
                .unwrap()
                .map(|x| { x.unwrap() })
                .collect_vec()
                .len(),
            1
        );

        second_store.delete().unwrap();
        assert_eq!(
            first_store
                .item_iter(0)
                .map(|x| x.unwrap().to_owned())
                .collect_vec(),
            [first_sequence, second_sequence].concat()
        );
        assert_eq!(
            read_dir(dir.clone())
                .unwrap()
                .map(|x| { x.unwrap() })
                .collect_vec()
                .len(),
            2
        );

        remove_dir_all(dir.clone()).unwrap();
    }

    #[test]
    fn destructive_appending() {
        let first_sequence = [0, 1, 1, 2];
        let second_sequence = [3, 5, 7, 12];

        let dir = temp_dir().join("destructive_appending_test");
        let _ = remove_dir_all(dir.clone());
        let _ = create_dir(dir.clone());

        let mut first_store = ZstdDirBackedArray::new(dir.clone(), None).unwrap();
        let mut second_store = DirectoryBackedArray::new(dir.clone()).unwrap();
        first_store.append(&first_sequence).unwrap();
        second_store.append(&second_sequence).unwrap();

        append_wrapper_destructive(&mut first_store, second_store).unwrap();
        assert_eq!(
            first_store
                .item_iter(0)
                .map(|x| x.unwrap().to_owned())
                .collect_vec(),
            [first_sequence, second_sequence].concat()
        );
        assert_eq!(
            read_dir(dir.clone())
                .unwrap()
                .map(|x| { x.unwrap() })
                .collect_vec()
                .len(),
            2
        );

        remove_dir_all(dir.clone()).unwrap();
    }
}
