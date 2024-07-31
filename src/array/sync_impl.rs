use std::{fmt::Debug, iter::FusedIterator, ops::Range};

use derive_getters::Getters;
use serde::{Deserialize, Serialize};

use crate::{
    entry::{sync_impl::BackedEntryMut, BackedEntry, BackedEntryArr},
    utils::BorrowExtender,
};

use super::{
    internal_idx, multiple_internal_idx, multiple_internal_idx_strict, open_mut, open_ref,
    BackedArrayError, BackedEntryContainer, BackedEntryContainerNested,
    BackedEntryContainerNestedAll, BackedEntryContainerNestedRead, BackedEntryContainerNestedWrite,
    Container, ResizingContainer,
};

/// Array stored as multiple arrays on disk.
///
/// Associates each access with the appropriate disk storage, loading it into
/// memory and returning the value. Subsequent accesses will use the in-memory
/// store. Use [`Self::clear_memory`] or [`Self::shrink_to_query`] to move the
/// cached sub-arrays back out of memory.
///
/// For repeated modifications, use [`Self::chunk_mut_iter`] to get perform
/// multiple modifications on a backing block before saving to disk.
/// Getting and overwriting the entries without these handles will write to
/// disk on every single change.
#[derive(Debug, Serialize, Deserialize, Getters)]
pub struct BackedArray<K, E> {
    // keys and entries must always have the same length
    // keys must always be sorted min-max
    keys: K,
    entries: E,
}

impl<K: Clone, E: Clone> Clone for BackedArray<K, E> {
    fn clone(&self) -> Self {
        Self {
            keys: self.keys.clone(),
            entries: self.entries.clone(),
        }
    }
}

pub type VecBackedArray<T, Disk, Coder> =
    BackedArray<Vec<Range<usize>>, Vec<BackedEntryArr<T, Disk, Coder>>>;

impl<K: Default, E: Default> Default for BackedArray<K, E> {
    fn default() -> Self {
        Self {
            keys: K::default(),
            entries: E::default(),
        }
    }
}

impl<K: Default, E: Default> BackedArray<K, E> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<K: Container<Data = Range<usize>>, E> BackedArray<K, E> {
    /// Total size of stored data.
    pub fn len(&self) -> usize {
        self.keys.as_ref().last().unwrap_or(&(0..0)).end
    }
}

impl<K, E: Container> BackedArray<K, E> {
    /// Number of underlying chunks.
    pub fn chunks_len(&self) -> usize {
        self.entries.as_ref().len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.as_ref().is_empty()
    }
}

impl<K, E: BackedEntryContainerNested> BackedArray<K, E> {
    /// Move all backing arrays out of memory.
    pub fn clear_memory(&mut self) {
        self.entries.as_mut().iter_mut().for_each(|entry| {
            let entry = entry.get_mut();
            entry.as_mut().get_mut().unload()
        });
    }

    /// Move the chunk at `idx` out of memory.
    pub fn clear_chunk(&mut self, idx: usize) {
        if let Some(mut val) = self.entries.c_get_mut(idx) {
            let entry = val.as_mut().get_mut();
            entry.as_mut().get_mut().unload()
        }
    }
}

impl<K: Container<Data = Range<usize>>, E: BackedEntryContainerNested> BackedArray<K, E> {
    /// Returns the number of items currently loaded into memory.
    ///
    /// To get memory usage on constant-sized items, multiply this value by
    /// the per-item size.
    pub fn loaded_len(&self) -> usize {
        self.entries
            .as_ref()
            .iter()
            .zip(self.keys.as_ref())
            .filter(|(ent, _)| ent.get_ref().is_loaded())
            .map(|(_, key)| key.len())
            .sum()
    }
}

impl<K: Container<Data = Range<usize>>, E: BackedEntryContainerNested> BackedArray<K, E> {
    /// Removes all backing stores not needed to hold `idxs` in memory.
    pub fn shrink_to_query(&mut self, idxs: &[usize]) {
        self.keys
            .as_ref()
            .iter()
            .enumerate()
            .filter(|(_, key)| !idxs.iter().any(|idx| key.contains(idx)))
            .for_each(|(idx, _)| {
                if let Some(mut v) = self.entries.c_get_mut(idx) {
                    open_mut!(v).unload();
                };
            });
    }
}

/// Read implementations
impl<K: Container<Data = Range<usize>>, E: BackedEntryContainerNestedRead> BackedArray<K, E> {
    /// Return a value (potentially loading its backing array).
    ///
    /// Backing arrays stay in memory until freed.
    pub fn get(
        &self,
        idx: usize,
    ) -> Result<<E::Unwrapped as Container>::Ref<'_>, BackedArrayError<E::ReadError>> {
        let loc = internal_idx(self.keys.as_ref(), idx)
            .ok_or(BackedArrayError::OutsideEntryBounds(idx))?;

        let entry_container = BorrowExtender::try_new(&self.entries, |entries| {
            entries
                .c_get(loc.entry_idx)
                .ok_or(BackedArrayError::OutsideEntryBounds(idx))
        })?;
        let entry = BorrowExtender::try_new(entry_container, |entry_container| {
            open_ref!(entry_container)
                .get_ref()
                .load()
                .map_err(BackedArrayError::Coder)
        })?;
        entry
            .c_get(loc.inside_entry_idx)
            .ok_or(BackedArrayError::OutsideEntryBounds(idx))
    }

    /// Produces a vector for all requested indicies.
    ///
    /// Loads all necessary backing arrays into memory until freed.
    /// Invalid indicies will be silently dropped. Use the strict version
    /// ([`Self::get_multiple_strict`]) to get errors on invalid indicies.
    pub fn get_multiple<'a, I>(
        &'a self,
        idxs: I,
    ) -> impl Iterator<Item = <E::Unwrapped as Container>::Ref<'_>>
    where
        I: IntoIterator<Item = usize> + 'a,
    {
        multiple_internal_idx(self.keys.as_ref(), idxs).flat_map(|loc| {
            let entry_container =
                BorrowExtender::maybe_new(&self.entries, |entries| entries.c_get(loc.entry_idx))?;
            let entry = BorrowExtender::try_new(entry_container, |entry_container| {
                open_ref!(entry_container).get_ref().load()
            })
            .ok()?;
            entry.c_get(loc.inside_entry_idx)
        })
    }

    /// [`Self::get_multiple`], but returns Errors for invalid idx.
    pub fn get_multiple_strict<'a, I>(
        &'a self,
        idxs: I,
    ) -> impl Iterator<Item = Result<<E::Unwrapped as Container>::Ref<'_>, BackedArrayError<E::ReadError>>>
    where
        I: IntoIterator<Item = usize> + Clone + 'a,
    {
        multiple_internal_idx_strict(self.keys.as_ref(), idxs.clone())
            .zip(idxs)
            .map(|(loc, idx)| {
                let loc = loc.ok_or(BackedArrayError::OutsideEntryBounds(idx))?;
                let entry_container = BorrowExtender::try_new(&self.entries, |entries| {
                    entries
                        .c_get(loc.entry_idx)
                        .ok_or(BackedArrayError::OutsideEntryBounds(idx))
                })?;
                let entry = BorrowExtender::try_new(entry_container, |entry_container| {
                    open_ref!(entry_container)
                        .get_ref()
                        .load()
                        .map_err(BackedArrayError::Coder)
                })?;
                entry
                    .c_get(loc.inside_entry_idx)
                    .ok_or(BackedArrayError::OutsideEntryBounds(idx))
            })
    }
}

/// Write implementations
impl<
        K: ResizingContainer<Data = Range<usize>>,
        E: BackedEntryContainerNestedWrite + ResizingContainer,
    > BackedArray<K, E>
{
    /// Adds new values by writing them to the backing store.
    ///
    /// Does not keep the values in memory.
    ///
    /// # Example
    /// ```rust
    /// #[cfg(feature = "bincode")] {
    /// use backed_data::{
    ///     array::sync_impl::VecBackedArray,
    ///     entry::{
    ///         disks::Plainfile,
    ///         formats::BincodeCoder,
    ///     }
    /// };
    /// use std::fs::{File, create_dir_all, remove_dir_all, OpenOptions};
    ///
    /// let FILENAME_BASE = std::env::temp_dir().join("example_array_append");
    /// let values = ([0, 1, 1],
    ///     [2, 3, 5]);
    ///
    /// create_dir_all(FILENAME_BASE.clone()).unwrap();
    /// let file_0 = FILENAME_BASE.clone().join("_0");
    /// let file_1 = FILENAME_BASE.join("_1");
    /// let mut array: VecBackedArray<u32, Plainfile, _> = VecBackedArray::default();
    /// array.append(values.0, file_0.into(), BincodeCoder {});
    /// array.append(values.1, file_1.into(), BincodeCoder {});
    ///
    /// assert_eq!(array.get(4).unwrap().as_ref(), &3);
    /// remove_dir_all(FILENAME_BASE).unwrap();
    /// }
    /// ```
    pub fn append<U: Into<E::Unwrapped>>(
        &mut self,
        values: U,
        backing_store: E::Disk,
        coder: E::Coder,
    ) -> Result<&mut Self, E::WriteError> {
        let values = values.into();

        // End of a range is exclusive
        let start_idx = self
            .keys
            .as_ref()
            .last()
            .map(|key_range| key_range.end)
            .unwrap_or(0);
        self.keys.c_push(start_idx..(start_idx + values.c_len()));

        let mut entry = BackedEntry::new(backing_store, coder);
        entry.write_unload(values)?;
        self.entries.c_push(entry.into());
        Ok(self)
    }

    /// [`Self::append`], but keeps values in memory.
    ///
    /// # Example
    /// ```rust
    /// use backed_data::{
    ///     array::sync_impl::VecBackedArray,
    ///     entry::{
    ///         disks::Plainfile,
    ///         formats::BincodeCoder,
    ///     },
    /// };
    /// use std::fs::{File, remove_file, OpenOptions};
    ///
    /// let FILENAME = std::env::temp_dir().join("example_array_append_memory");
    /// let values = ([0, 1, 1],
    ///     [2, 3, 5]);
    ///
    /// let mut array: VecBackedArray<u32, Plainfile, _> = VecBackedArray::default();
    /// array.append_memory(values.0, FILENAME.clone().into(), BincodeCoder {});
    ///
    /// // Overwrite file, making disk pointer for first array invalid
    /// array.append_memory(values.1, FILENAME.clone().into(), BincodeCoder {});
    ///
    /// assert_eq!(array.get(0).unwrap().as_ref(), &0);
    /// remove_file(FILENAME).unwrap();
    /// ```
    pub fn append_memory<U: Into<E::Unwrapped>>(
        &mut self,
        values: U,
        backing_store: E::Disk,
        coder: E::Coder,
    ) -> Result<&mut Self, E::WriteError> {
        let values = values.into();

        // End of a range is exclusive
        let start_idx = self
            .keys
            .as_ref()
            .last()
            .map(|key_range| key_range.end)
            .unwrap_or(0);
        self.keys.c_push(start_idx..(start_idx + values.c_len()));
        let mut entry = BackedEntry::new(backing_store, coder);
        entry.write(values)?;
        self.entries.c_push(entry.into());
        Ok(self)
    }
}

impl<
        K: ResizingContainer<Data = Range<usize>>,
        E: BackedEntryContainerNested + ResizingContainer,
    > BackedArray<K, E>
{
    /// Removes an entry with the internal index, shifting ranges.
    ///
    /// The getter functions can be used to indentify the target index.
    pub fn remove(&mut self, entry_idx: usize) -> &mut Self {
        // This split is necessary to solve lifetimes
        let width = self.keys.c_get(entry_idx).map(|entry| entry.as_ref().len());

        if let Some(width) = width {
            self.keys.c_remove(entry_idx);
            self.entries.c_remove(entry_idx);

            // Shift all later ranges downwards
            self.keys
                .as_mut()
                .iter_mut()
                .skip(entry_idx)
                .for_each(|key_range| {
                    key_range.start -= width;
                    key_range.end -= width
                });
        }

        self
    }
}

impl<K: ResizingContainer<Data = Range<usize>>, E: ResizingContainer> BackedArray<K, E> {
    /// Move entries in `rhs` to [`self`].
    pub fn merge(&mut self, mut rhs: Self) -> &mut Self {
        let offset = self.keys.as_ref().last().unwrap_or(&(0..0)).end;
        self.keys.extend(
            rhs.keys
                .as_ref()
                .iter()
                .map(|range| (range.start + offset)..(range.end + offset)),
        );
        self.entries.c_append(&mut rhs.entries);
        self
    }
}

// ---------- Iterator Returns ---------- //

/// Iterates over a backed array, returning each item in order.
///
/// To keep an accurate size count, failed reads will not be retried.
/// This will keep each disk loaded after pulling data from it.
/// Stepping by > 1 with `nth` implementation may skip loading a disk.
#[derive(Debug)]
pub struct BackedArrayIter<'a, K, E> {
    backed: &'a BackedArray<K, E>,
    pos: usize,
}

impl<'a, K, E> BackedArrayIter<'a, K, E> {
    fn new(backed: &'a BackedArray<K, E>) -> Self {
        Self { backed, pos: 0 }
    }
}

impl<'a, K: Container<Data = Range<usize>>, E: BackedEntryContainerNestedRead> Iterator
    for BackedArrayIter<'a, K, E>
{
    type Item = Result<<E::Unwrapped as Container>::Ref<'a>, E::ReadError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.nth(0)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.pos += n;
        match self.backed.get(self.pos) {
            Ok(val) => {
                self.pos += 1;
                Some(Ok(val))
            }
            Err(e) => match e {
                BackedArrayError::OutsideEntryBounds(_) => None,
                BackedArrayError::Coder(c) => {
                    self.pos += 1;
                    Some(Err(c))
                }
            },
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.backed.len() - self.pos;
        (remaining, Some(remaining))
    }
}

impl<K: Container<Data = Range<usize>>, E: BackedEntryContainerNestedRead> FusedIterator
    for BackedArrayIter<'_, K, E>
{
}

impl<K: Container<Data = Range<usize>>, E: BackedEntryContainerNestedRead> BackedArray<K, E> {
    /// Provides a [`BackedArrayIter`] over each backed item.
    pub fn item_iter(&self) -> BackedArrayIter<'_, K, E> {
        BackedArrayIter::new(self)
    }

    /// Returns underlying chunks in order.
    ///
    /// This will load each chunk before providing it.
    /// All chunks loaded earlier in the iteration will remain loaded.
    pub fn chunk_iter(&mut self) -> impl Iterator<Item = Result<&E::Unwrapped, E::ReadError>> {
        self.entries.as_ref().iter().map(|arr| arr.get_ref().load())
    }
}

impl<K: Container<Data = Range<usize>>, E: BackedEntryContainerNestedAll> BackedArray<K, E> {
    /// Provides mutable handles to underlying chunks, using [`BackedEntryMut`].
    ///
    /// See [`Self::chunk_iter`] for the immutable iterator.
    pub fn chunk_mut_iter(
        &mut self,
    ) -> impl Iterator<
        Item = Result<
            BackedEntryMut<'_, BackedEntry<E::OnceWrapper, E::Disk, E::Coder>>,
            E::ReadError,
        >,
    > {
        self.entries
            .as_mut()
            .iter_mut()
            .map(|arr| arr.get_mut().mut_handle())
    }
}

impl<K: ResizingContainer<Data = Range<usize>>, E: ResizingContainer> BackedArray<K, E> {
    /// Construct a backed array from entry range, backing storage pairs.
    ///
    /// If ranges do not correspond to the entry arrays, the resulting
    /// [`BackedArray`] will be invalid.
    pub fn from_pairs<I, U>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (Range<usize>, U)>,
        U: Into<E::Data>,
    {
        let (keys, entries): (_, Vec<_>) = pairs.into_iter().unzip();
        let entries = entries.into_iter().map(U::into).collect();
        Self { keys, entries }
    }
}

#[cfg(test)]
#[cfg(feature = "bincode")]
mod tests {
    use std::{io::Cursor, sync::Mutex};

    use itertools::Itertools;

    use crate::{entry::formats::BincodeCoder, test_utils::CursorVec};

    use super::*;

    #[test]
    fn multiple_retrieve() {
        let mut back_vector_0 = &mut Cursor::new(Vec::with_capacity(3));
        let back_vector_0 = CursorVec {
            inner: Mutex::new(&mut back_vector_0),
        };
        let mut back_vector_1 = &mut Cursor::new(Vec::with_capacity(3));
        let back_vector_1 = CursorVec {
            inner: Mutex::new(&mut back_vector_1),
        };

        const INPUT_0: [u8; 3] = [0, 1, 1];
        const INPUT_1: [u8; 3] = [2, 3, 5];

        let mut backed = VecBackedArray::new();
        backed
            .append(INPUT_0, back_vector_0, BincodeCoder {})
            .unwrap();
        backed
            .append_memory(INPUT_1, back_vector_1, BincodeCoder {})
            .unwrap();

        assert_eq!(backed.get(0).unwrap().as_ref(), &0);
        assert_eq!(backed.get(4).unwrap().as_ref(), &3);
        assert_eq!(
            (
                backed.get(0).unwrap().as_ref(),
                backed.get(4).unwrap().as_ref()
            ),
            (&0, &3)
        );

        assert_eq!(
            backed.get_multiple([0, 2, 4, 5]).collect_vec(),
            [&0, &1, &3, &5]
        );
        assert_eq!(
            backed.get_multiple([5, 2, 0, 5]).collect_vec(),
            [&5, &1, &0, &5]
        );
        assert_eq!(
            backed
                .get_multiple_strict([5, 2, 0, 5])
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            [&5, &1, &0, &5]
        );
    }

    #[test]
    fn out_of_bounds_access() {
        let mut back_vector = &mut Cursor::new(Vec::with_capacity(3));
        let back_vector = CursorVec {
            inner: Mutex::new(&mut back_vector),
        };

        const INPUT: &[u8] = &[0, 1, 1];

        let mut backed = VecBackedArray::new();
        backed.append(INPUT, back_vector, BincodeCoder {}).unwrap();

        assert!(backed.get(0).is_ok());
        assert!(backed.get(10).is_err());
        assert!(backed
            .get_multiple_strict([0, 10])
            .collect::<Result<Vec<_>, _>>()
            .is_err());
        assert_eq!(backed.get_multiple([0, 10]).count(), 1);
        assert!(backed.get_multiple([20, 10]).next().is_none());
    }

    #[test]
    fn chunk_iteration() {
        let mut back_vector_0 = &mut Cursor::new(Vec::with_capacity(3));
        let back_vector_0 = CursorVec {
            inner: Mutex::new(&mut back_vector_0),
        };
        let mut back_vector_1 = &mut Cursor::new(Vec::with_capacity(3));
        let back_vector_1 = CursorVec {
            inner: Mutex::new(&mut back_vector_1),
        };

        const INPUT_0: &[u8] = &[0, 1, 1];
        const INPUT_1: &[u8] = &[2, 5, 7];

        let mut backed = VecBackedArray::new();
        backed
            .append(INPUT_0, back_vector_0, BincodeCoder {})
            .unwrap();
        backed
            .append(INPUT_1, back_vector_1, BincodeCoder {})
            .unwrap();

        let collected = backed.chunk_iter().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(collected[0].as_ref(), INPUT_0);
        assert_eq!(collected[1].as_ref(), INPUT_1);
        assert_eq!(collected.len(), 2);
    }

    #[test]
    fn item_iteration() {
        let mut back_vector_0 = &mut Cursor::new(Vec::with_capacity(3));
        let back_vector_0 = CursorVec {
            inner: Mutex::new(&mut back_vector_0),
        };
        let mut back_vector_1 = &mut Cursor::new(Vec::with_capacity(3));
        let back_vector_1 = CursorVec {
            inner: Mutex::new(&mut back_vector_1),
        };

        const INPUT_0: &[u8] = &[0, 1, 1];
        const INPUT_1: &[u8] = &[2, 5, 7];

        let mut backed = VecBackedArray::new();
        backed
            .append(INPUT_0, back_vector_0, BincodeCoder {})
            .unwrap();
        backed
            .append(INPUT_1, back_vector_1, BincodeCoder {})
            .unwrap();
        let collected = backed.item_iter().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(collected[5], &7);
        assert_eq!(collected[2], &1);
        assert_eq!(collected.len(), 6);
    }

    #[test]
    fn mod_iteration() {
        const FIB: &[u8] = &[0, 1, 1, 5, 7];
        const INPUT_1: &[u8] = &[2, 5, 7];

        let mut back_vec_0 = Cursor::new(Vec::new());
        let mut back_vec_0 = CursorVec {
            inner: Mutex::new(&mut back_vec_0),
        };
        let back_vec_ptr_0: *mut CursorVec = &mut back_vec_0;
        let mut back_vec_1 = Cursor::new(Vec::new());
        let mut back_vec_1 = CursorVec {
            inner: Mutex::new(&mut back_vec_1),
        };
        let back_vec_ptr_1: *mut CursorVec = &mut back_vec_1;

        let backing_store_0 = back_vec_0.get_ref();
        let backing_store_1 = back_vec_1.get_ref();

        // Intentional unsafe access to later peek underlying storage
        let mut backed = VecBackedArray::new();
        unsafe {
            backed
                .append(FIB, &mut *back_vec_ptr_0, BincodeCoder {})
                .unwrap();
        }
        assert_eq!(&backing_store_0[backing_store_0.len() - FIB.len()..], FIB);

        // Intentional unsafe access to later peek underlying storage
        unsafe {
            backed
                .append(INPUT_1, &mut *back_vec_ptr_1, BincodeCoder {})
                .unwrap();
        }
        assert_eq!(
            &backing_store_1[backing_store_1.len() - INPUT_1.len()..],
            INPUT_1
        );

        let handle = backed.chunk_mut_iter();

        let mut handle_vec = handle.collect::<Result<Vec<_>, _>>().unwrap();
        handle_vec[0][0] = 20;
        handle_vec[0][2] = 30;
        handle_vec[1][0] = 40;

        handle_vec[0].flush().unwrap();
        handle_vec[1].flush().unwrap();
        assert_eq!(backing_store_0[backing_store_0.len() - FIB.len()], 20);
        assert_eq!(backing_store_0[backing_store_0.len() - FIB.len() + 2], 30);
        assert_eq!(
            backing_store_0[backing_store_0.len() - FIB.len() + 1],
            FIB[1]
        );
        assert_eq!(backing_store_1[backing_store_1.len() - INPUT_1.len()], 40);
        assert_eq!(
            backing_store_1[backing_store_1.len() - INPUT_1.len() + 1],
            INPUT_1[1]
        );

        drop(handle_vec);
        assert_eq!(
            backed.item_iter().collect::<Result<Vec<_>, _>>().unwrap(),
            [&20, &1, &30, &5, &7, &40, &5, &7]
        );
    }

    #[test]
    fn length_checking() {
        let mut back_vector_0 = &mut Cursor::new(Vec::with_capacity(3));
        let back_vector_0 = CursorVec {
            inner: Mutex::new(&mut back_vector_0),
        };
        let mut back_vector_1 = &mut Cursor::new(Vec::with_capacity(3));
        let back_vector_1 = CursorVec {
            inner: Mutex::new(&mut back_vector_1),
        };

        const INPUT_0: &[u8] = &[0, 1, 1];
        const INPUT_1: &[u8] = &[2, 5, 7];

        let mut backed = VecBackedArray::new();
        backed
            .append(INPUT_0, back_vector_0, BincodeCoder {})
            .unwrap();
        backed
            .append_memory(INPUT_1, back_vector_1, BincodeCoder {})
            .unwrap();

        assert_eq!(backed.len(), 6);
        assert_eq!(backed.loaded_len(), 3);
        backed.shrink_to_query(&[0]);
        assert_eq!(backed.loaded_len(), 0);
        backed.get(0).unwrap();
        assert_eq!(backed.loaded_len(), 3);
        backed.clear_chunk(1);
        assert_eq!(backed.loaded_len(), 3);
        backed.clear_chunk(0);
        assert_eq!(backed.loaded_len(), 0);
        backed.get_multiple([0, 4]).collect_vec();
        assert_eq!(backed.loaded_len(), 6);
        backed.shrink_to_query(&[4]);
        assert_eq!(backed.loaded_len(), 3);
        backed.clear_memory();
        assert_eq!(backed.loaded_len(), 0);
    }
}
