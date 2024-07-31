use std::{
    io::{Read, Write},
    ops::Range,
};

use bincode::{deserialize_from, serialize_into};
use derive_getters::Getters;
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};

use crate::entry::{
    sync_impl::{BackedEntryMut, ReadDisk, WriteDisk},
    BackedEntry, BackedEntryArr, BackedEntryUnload,
};

use super::{
    internal_idx, multiple_internal_idx, multiple_internal_idx_strict, BackedArrayEntry,
    BackedArrayError,
};

/// Array stored as multiple arrays on disk.
///
/// Associates each access with the appropriate disk storage, loading it into
/// memory and returning the value. Subsequent accesses will use the in-memory
/// store. Use [`Self::clear_memory`] to move the cached sub-arrays back out of
/// memory.
///
/// Use [`Self::save_to_disk`] instead of serialization directly. This clears
/// entries to prevent data duplication on disk.
///
/// For repeated modifications, use [`Self::chunk_mod_iter`] to get perform
/// multiple modifications on a backing block before saving to disk.
/// Getting and overwriting the entries without these handles will write to
/// disk on every single change.
#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
pub struct BackedArray<T, Disk: for<'df> Deserialize<'df>> {
    // keys and entries must always have the same length
    // keys must always be sorted min-max
    keys: Vec<Range<usize>>,
    #[serde(deserialize_with = "entries_deserialize")]
    entries: Vec<BackedEntryArr<T, Disk>>,
}

/// Helper function to make deserialization work.
fn entries_deserialize<'de, D, Backing: Deserialize<'de>>(
    deserializer: D,
) -> Result<Backing, D::Error>
where
    D: Deserializer<'de>,
{
    Deserialize::deserialize(deserializer)
}

impl<T, Disk: for<'de> Deserialize<'de>> Default for BackedArray<T, Disk> {
    fn default() -> Self {
        Self {
            keys: vec![],
            entries: vec![],
        }
    }
}

impl<T, Disk: for<'de> Deserialize<'de>> BackedArray<T, Disk> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Total size of stored data.
    pub fn len(&self) -> usize {
        self.keys.last().unwrap_or(&(0..0)).end
    }

    /// Number of underlying chunks.
    pub fn chunks_len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Move all backing arrays out of memory.
    pub fn clear_memory(&mut self) {
        self.entries.iter_mut().for_each(|entry| entry.unload());
    }

    /// Move the chunk at `idx` out of memory.
    pub fn clear_chunk(&mut self, idx: usize) {
        self.entries[idx].unload();
    }

    /// Returns all backing entries and the range they cover.
    pub fn backed_array_entries(
        &self,
    ) -> impl Iterator<Item = BackedArrayEntry<'_, BackedEntry<Box<[T]>, Disk>>> {
        self.keys
            .iter()
            .zip(self.entries.iter())
            .map(BackedArrayEntry::from)
    }

    /// Returns all currently loaded backing entries and the range they cover.
    pub fn loaded_array_entries(
        &self,
    ) -> impl Iterator<Item = BackedArrayEntry<'_, BackedEntry<Box<[T]>, Disk>>> {
        self.backed_array_entries().filter(
            |backed: &BackedArrayEntry<'_, BackedEntry<Box<[T]>, Disk>>| backed.entry.is_loaded(),
        )
    }

    /// Returns the number of items currently loaded into memory.
    ///
    /// To get memory usage on constant-sized items, multiply this value by
    /// the per-item size.
    pub fn loaded_len(&self) -> usize {
        self.loaded_array_entries()
            .map(|backed| backed.range.len())
            .sum()
    }

    /// Removes all backing stores not needed to hold `idxs` in memory.
    pub fn shrink_to_query(&mut self, idxs: &[usize]) {
        self.keys
            .iter()
            .enumerate()
            .filter(|(_, key)| !idxs.iter().any(|idx| key.contains(idx)))
            .for_each(|(idx, _)| self.entries[idx].unload());
    }
}

impl<T: DeserializeOwned + Clone, Disk: ReadDisk> From<BackedArray<T, Disk>>
    for bincode::Result<Box<[T]>>
{
    fn from(mut val: BackedArray<T, Disk>) -> Self {
        Ok(val
            .entries
            .iter_mut()
            .map(|entry| entry.load())
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flat_map(|entry| entry.iter().cloned())
            .collect())
    }
}

/// Read implementations
impl<T: DeserializeOwned, Disk: ReadDisk> BackedArray<T, Disk> {
    /// Return a value (potentially loading its backing array).
    ///
    /// Backing arrays stay in memory until freed.
    pub fn get(&mut self, idx: usize) -> Result<&T, BackedArrayError> {
        let loc = internal_idx(&self.keys, idx).ok_or(BackedArrayError::OutsideEntryBounds(idx))?;
        Ok(&self.entries[loc.entry_idx]
            .load()
            .map_err(BackedArrayError::Bincode)?[loc.inside_entry_idx])
    }

    /// Produces a vector for all requested indicies.
    ///
    /// Loads all necessary backing arrays into memory until freed.
    /// Invalid indicies will be silently dropped. Use the strict version
    /// ([`Self::get_multiple_strict`]) to get errors on invalid indicies.
    pub fn get_multiple<'a, I>(&'a mut self, idxs: I) -> Vec<bincode::Result<&'a T>>
    where
        I: IntoIterator<Item = usize> + 'a,
    {
        // Unsafe needed to mutate array entries with load inside the iterator.
        // The code is safe: iterators execute sequentially, so loads to the
        // same entry will not interleave, and the resultant borrowing follows
        // the usual rules.
        let entries_ptr = self.entries.as_mut_ptr();
        multiple_internal_idx(&self.keys, idxs)
            .map(|loc| unsafe {
                Ok(&(*entries_ptr.add(loc.entry_idx)).load()?[loc.inside_entry_idx])
            })
            .collect()
    }

    /// [`Self::get_multiple`], but returns Errors for invalid idx.
    pub fn get_multiple_strict<'a, I>(&'a mut self, idxs: I) -> Vec<Result<&'a T, BackedArrayError>>
    where
        I: IntoIterator<Item = usize> + 'a,
    {
        // See [`Self::get_multiple_entries`] note.
        let entries_ptr = self.entries.as_mut_ptr();
        multiple_internal_idx_strict(&self.keys, idxs)
            .enumerate()
            .map(|(idx, loc)| {
                let loc = loc.ok_or(BackedArrayError::OutsideEntryBounds(idx))?;
                unsafe {
                    Ok(&(*entries_ptr.add(loc.entry_idx))
                        .load()
                        .map_err(BackedArrayError::Bincode)?[loc.inside_entry_idx])
                }
            })
            .collect()
    }

    /// Returns a reference to a particular underlying chunk.
    ///
    /// See [`Self::get_chunk_mut`] for a modifiable handle.
    pub fn get_chunk(&mut self, idx: usize) -> Option<bincode::Result<&[T]>> {
        self.entries
            .get_mut(idx)
            .map(|arr| arr.load().map(|entry| entry.as_ref()))
    }
}

/// Write implementations
impl<T: Serialize, Disk: WriteDisk> BackedArray<T, Disk> {
    /// Adds new values by writing them to the backing store.
    ///
    /// Does not keep the values in memory.
    ///
    /// # Example
    /// ```rust
    /// use backed_array::array::sync_impl::BackedArray;
    /// use std::fs::{File, create_dir_all, remove_dir_all, OpenOptions};
    ///
    /// let FILENAME_BASE = std::env::temp_dir().join("example_array_append");
    /// let values = ([0, 1, 1],
    ///     [2, 3, 5]);
    ///
    /// create_dir_all(FILENAME_BASE.clone()).unwrap();
    /// let file_0 = FILENAME_BASE.clone().join("_0");
    /// let file_1 = FILENAME_BASE.join("_1");
    /// let mut array: BackedArray<u32, _> = BackedArray::default();
    /// array.append(values.0, file_0);
    /// array.append(values.1, file_1);
    ///
    /// assert_eq!(array.get(4).unwrap(), &3);
    /// remove_dir_all(FILENAME_BASE).unwrap();
    /// ```
    pub fn append<U: Into<Box<[T]>>>(
        &mut self,
        values: U,
        backing_store: Disk,
    ) -> bincode::Result<&mut Self> {
        let values = values.into();

        // End of a range is exclusive
        let start_idx = self.keys.last().map(|key_range| key_range.end).unwrap_or(0);
        self.keys.push(start_idx..(start_idx + values.len()));
        let mut entry = BackedEntryArr::new(backing_store);
        entry.write_unload(values)?;
        self.entries.push(entry);
        Ok(self)
    }

    /// [`Self::append`], but keeps values in memory.
    ///
    /// # Example
    /// ```rust
    /// use backed_array::array::sync_impl::BackedArray;
    /// use std::fs::{File, remove_file, OpenOptions};
    ///
    /// let FILENAME = std::env::temp_dir().join("example_array_append_memory");
    /// let values = ([0, 1, 1],
    ///     [2, 3, 5]);
    ///
    /// let mut array: BackedArray<u32, _> = BackedArray::default();
    /// array.append_memory(values.0, FILENAME.clone());
    ///
    /// // Overwrite file, making disk pointer for first array invalid
    /// array.append_memory(values.1, FILENAME.clone());
    ///
    /// assert_eq!(array.get(0).unwrap(), &0);
    /// remove_file(FILENAME).unwrap();
    /// ```
    pub fn append_memory<U: Into<Box<[T]>>>(
        &mut self,
        values: U,
        backing_store: Disk,
    ) -> bincode::Result<&mut Self> {
        let values = values.into();

        // End of a range is exclusive
        let start_idx = self.keys.last().map(|key_range| key_range.end).unwrap_or(0);
        self.keys.push(start_idx..(start_idx + values.len()));
        let mut entry = BackedEntryArr::new(backing_store);
        entry.write(values)?;
        self.entries.push(entry);
        Ok(self)
    }
}

impl<T: Serialize + DeserializeOwned, Disk: WriteDisk + ReadDisk> BackedArray<T, Disk> {
    /// Returns a mutable handle to a given chunk.
    pub fn get_chunk_mut(
        &mut self,
        idx: usize,
    ) -> Option<bincode::Result<BackedEntryMut<Box<[T]>, Disk>>> {
        self.entries.get_mut(idx).map(|arr| arr.mut_handle())
    }
}

impl<T, Disk: for<'de> Deserialize<'de>> BackedArray<T, Disk> {
    /// Removes an entry with the internal index, shifting ranges.
    ///
    /// The getter functions can be used to indentify the target index.
    pub fn remove(&mut self, entry_idx: usize) -> &mut Self {
        let width = self.keys[entry_idx].len();
        self.keys.remove(entry_idx);
        self.entries.remove(entry_idx);

        // Shift all later ranges downwards
        self.keys[entry_idx..].iter_mut().for_each(|key_range| {
            key_range.start -= width;
            key_range.end -= width
        });

        self
    }

    pub fn get_disks(&self) -> Vec<&Disk> {
        self.entries.iter().map(|entry| entry.get_disk()).collect()
    }

    /// Get a mutable handle to disks.
    ///
    /// Used primarily to update paths, when old values are out of date.
    pub fn get_disks_mut(&mut self) -> Vec<&mut Disk> {
        self.entries
            .iter_mut()
            .map(|entry| entry.get_disk_mut())
            .collect()
    }
}

impl<T: Serialize, Disk: WriteDisk> BackedArray<T, Disk> {
    /// Serializes this to disk after clearing all entries from memory to disk.
    ///
    /// Writing directly from serialize wastes disk space and increases load
    /// time by writing the backed data with this struct.
    pub fn save_to_disk<W: Write>(&mut self, writer: W) -> bincode::Result<()> {
        self.clear_memory();
        serialize_into(writer, self)
    }
}

impl<T: Serialize, Disk: ReadDisk> BackedArray<T, Disk> {
    /// Loads the backed array. Does not load backing arrays, unless this
    /// was saved with data (was not saved with [`Self::save_to_disk`]).
    pub fn load<R: Read>(writer: &mut R) -> bincode::Result<Self> {
        deserialize_from(writer)
    }
}

impl<T: Clone, Disk: for<'de> Deserialize<'de> + Clone> BackedArray<T, Disk> {
    /// Combine `self` and `rhs` into a new [`Self`].
    ///
    /// Appends entries of `self` and `rhs`.
    pub fn join(&self, rhs: &Self) -> Self {
        let offset = self.keys.last().unwrap_or(&(0..0)).end;
        let other_keys = rhs
            .keys
            .iter()
            .map(|range| (range.start + offset)..(range.end + offset));
        Self {
            keys: self.keys.iter().cloned().chain(other_keys).collect(),
            entries: self
                .entries
                .iter()
                .chain(rhs.entries.iter())
                .cloned()
                .collect(),
        }
    }

    /// Copy entries in `rhs` to `self`.
    pub fn merge(&mut self, rhs: &Self) -> &mut Self {
        let offset = self.keys.last().unwrap_or(&(0..0)).end;
        self.keys.extend(
            rhs.keys
                .iter()
                .map(|range| (range.start + offset)..(range.end + offset)),
        );
        self.entries.extend_from_slice(&rhs.entries);
        self
    }
}

impl<T, Disk: for<'de> Deserialize<'de>> BackedArray<T, Disk> {
    /// Moves all entries of `rhs` into `self`.
    pub fn append_array(&mut self, mut rhs: Self) -> &mut Self {
        let offset = self.keys.last().unwrap_or(&(0..0)).end;
        rhs.keys.iter_mut().for_each(|range| {
            range.start += offset;
            range.end += offset;
        });
        self.keys.append(&mut rhs.keys);
        self.entries.append(&mut rhs.entries);
        self
    }
}

// Iterator returns
impl<T: DeserializeOwned, Disk: ReadDisk> BackedArray<T, Disk> {
    /// Outputs items in order.
    ///
    /// Excluding chunks before the initial offset, all chunks will load in
    /// and remain loaded during iteration.
    ///
    /// # Arguments
    /// * `offset`: Starting position
    pub fn item_iter(&mut self, offset: usize) -> impl Iterator<Item = bincode::Result<&T>> {
        // Defaults to force a None return from iterator
        let mut outer_pos = usize::MAX;
        let mut inner_pos = usize::MAX;

        if let Some(loc) = internal_idx(&self.keys, offset) {
            outer_pos = loc.entry_idx;
            inner_pos = loc.inside_entry_idx;
        };

        self.chunk_iter(outer_pos)
            .flat_map(|chunk| {
                let chunk: Box<dyn Iterator<Item = bincode::Result<&T>>> = match chunk {
                    Ok(chunk_ok) => Box::new(chunk_ok.iter().map(Ok)),
                    Err(chunk_err) => Box::new([Err(chunk_err)].into_iter()),
                };
                chunk
            })
            .skip(inner_pos)
    }

    /// Returns underlying chunks in order.
    ///
    /// All chunks loaded earlier in the iteration will remain loaded.
    ///
    /// # Arguments
    /// * `offset`: Starting chunk (skips loading offset - 1 chunks)
    pub fn chunk_iter(&mut self, offset: usize) -> impl Iterator<Item = bincode::Result<&[T]>> {
        self.entries
            .iter_mut()
            .skip(offset)
            .map(|entry| entry.load().map(|entry| entry.as_ref()))
    }
}

impl<T: Serialize + DeserializeOwned, Disk: ReadDisk + WriteDisk> BackedArray<T, Disk> {
    /// Provides mutable handles to underlying chunks, using [`BackedEntryMut`].
    ///
    /// See [`Self::chunk_iter`] for the immutable iterator.
    pub fn chunk_mod_iter(
        &mut self,
        offset: usize,
    ) -> impl Iterator<Item = bincode::Result<BackedEntryMut<Box<[T]>, Disk>>> {
        self.entries
            .iter_mut()
            .skip(offset)
            .map(|entry| entry.mut_handle())
    }
}

impl<T: DeserializeOwned + Clone, Disk: ReadDisk> BackedArray<T, Disk> {
    /// Returns an iterator that outputs clones of chunk data.
    ///
    /// See [`Self::chunk_iter`] for a version that produces references.
    pub fn dup_iter(
        &mut self,
        offset: usize,
    ) -> impl Iterator<Item = bincode::Result<Vec<T>>> + '_ {
        self.entries.iter_mut().skip(offset).map(|entry| {
            let val = entry.load()?.to_vec();
            entry.unload();
            Ok(val)
        })
    }
}

impl<T, Disk: for<'de> Deserialize<'de>> BackedArray<T, Disk> {
    /// Construct a backed array from entry range, backing storage pairs.
    ///
    /// Not recommended for use outside of BackedArray wrappers.
    /// If ranges do not correspond to the entry arrays, the resulting
    /// [`BackedArray`] will be invalid.
    pub fn from_pairs<I>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (Range<usize>, BackedEntry<Box<[T]>, Disk>)>,
    {
        let (keys, entries) = pairs.into_iter().unzip();
        Self { keys, entries }
    }
}

impl<T, Disk: for<'de> Deserialize<'de>> BackedArray<T, Disk> {
    /// Replaces the underlying entry disk.
    ///
    /// Not recommended for use outside of BackedArray wrappers.
    /// If ranges do not correspond to the entry arrays, the resulting
    /// [`BackedArray`] will be invalid.
    pub fn replace_disk<OtherDisk>(self) -> BackedArray<T, OtherDisk>
    where
        OtherDisk: for<'de> Deserialize<'de>,
        Disk: Into<OtherDisk>,
    {
        BackedArray::<T, OtherDisk> {
            keys: self.keys,
            entries: self.entries.into_iter().map(|x| x.replace_disk()).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::test_utils::CursorVec;

    use super::*;

    #[test]
    fn multiple_retrieve() {
        let back_vector_0 = CursorVec {
            inner: &mut Cursor::new(Vec::with_capacity(3)),
        };
        let back_vector_1 = CursorVec {
            inner: &mut Cursor::new(Vec::with_capacity(3)),
        };

        const INPUT_0: [u8; 3] = [0, 1, 1];
        const INPUT_1: [u8; 3] = [2, 3, 5];

        let mut backed = BackedArray::new();
        backed.append(INPUT_0, back_vector_0).unwrap();
        backed.append_memory(INPUT_1, back_vector_1).unwrap();

        assert_eq!(backed.get(0).unwrap(), &0);
        assert_eq!(backed.get(4).unwrap(), &3);
        assert_eq!(
            backed
                .get_multiple([0, 2, 4, 5])
                .into_iter()
                .collect::<bincode::Result<Vec<_>>>()
                .unwrap(),
            [&0, &1, &3, &5]
        );
        assert_eq!(
            backed
                .get_multiple([5, 2, 0, 5])
                .into_iter()
                .collect::<bincode::Result<Vec<_>>>()
                .unwrap(),
            [&5, &1, &0, &5]
        );
        assert_eq!(
            backed
                .get_multiple_strict([5, 2, 0, 5])
                .into_iter()
                .collect::<Result<Vec<_>, BackedArrayError>>()
                .unwrap(),
            [&5, &1, &0, &5]
        );
    }

    #[test]
    fn out_of_bounds_access() {
        let back_vector = CursorVec {
            inner: &mut Cursor::new(Vec::with_capacity(3)),
        };

        const INPUT: &[u8] = &[0, 1, 1];

        let mut backed = BackedArray::new();
        backed.append(INPUT, back_vector).unwrap();

        assert!(backed.get(0).is_ok());
        assert!(backed.get(10).is_err());
        assert!(backed
            .get_multiple_strict([0, 10])
            .into_iter()
            .collect::<Result<Vec<_>, BackedArrayError>>()
            .is_err());
        assert_eq!(
            backed
                .get_multiple([0, 10])
                .into_iter()
                .collect::<bincode::Result<Vec<_>>>()
                .unwrap()
                .len(),
            1
        );
        assert!(backed
            .get_multiple([20, 10])
            .into_iter()
            .collect::<bincode::Result<Vec<_>>>()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn chunk_iteration() {
        let back_vector_0 = CursorVec {
            inner: &mut Cursor::new(Vec::with_capacity(3)),
        };
        let back_vector_1 = CursorVec {
            inner: &mut Cursor::new(Vec::with_capacity(3)),
        };

        const INPUT_0: &[u8] = &[0, 1, 1];
        const INPUT_1: &[u8] = &[2, 5, 7];

        let mut backed = BackedArray::new();
        backed.append(INPUT_0, back_vector_0).unwrap();
        backed.append(INPUT_1, back_vector_1).unwrap();

        let collected = backed.chunk_iter(0).collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(collected[0], INPUT_0);
        assert_eq!(collected[1], INPUT_1);
        assert_eq!(collected.len(), 2);
    }

    #[test]
    fn item_iteration() {
        let back_vector_0 = CursorVec {
            inner: &mut Cursor::new(Vec::with_capacity(3)),
        };
        let back_vector_1 = CursorVec {
            inner: &mut Cursor::new(Vec::with_capacity(3)),
        };

        const INPUT_0: &[u8] = &[0, 1, 1];
        const INPUT_1: &[u8] = &[2, 5, 7];

        let mut backed = BackedArray::new();
        backed.append(INPUT_0, back_vector_0).unwrap();
        backed.append(INPUT_1, back_vector_1).unwrap();
        let collected = backed.item_iter(0).collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(collected[5], &7);
        assert_eq!(collected[2], &1);
        assert_eq!(collected.len(), 6);
    }

    #[test]
    fn dup_iteration() {
        let back_vector_0 = CursorVec {
            inner: &mut Cursor::new(Vec::with_capacity(3)),
        };
        let back_vector_1 = CursorVec {
            inner: &mut Cursor::new(Vec::with_capacity(3)),
        };

        const INPUT_0: &[u8] = &[0, 1, 1];
        const INPUT_1: &[u8] = &[2, 5, 7];

        let mut backed = BackedArray::new();
        backed.append(INPUT_0, back_vector_0).unwrap();
        backed.append(INPUT_1, back_vector_1).unwrap();

        let collected = backed.dup_iter(0).collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(collected[0], INPUT_0);
        assert_eq!(collected[1], INPUT_1);
        assert_eq!(collected.len(), 2);
    }

    #[test]
    fn mod_iteration() {
        const FIB: &[u8] = &[0, 1, 1, 5, 7];
        const INPUT_1: &[u8] = &[2, 5, 7];

        let mut back_vec_0 = CursorVec {
            inner: &mut Cursor::new(Vec::with_capacity(10)),
        };
        let back_vec_ptr_0: *mut CursorVec = &mut back_vec_0;
        let mut back_vec_1 = CursorVec {
            inner: &mut Cursor::new(Vec::with_capacity(10)),
        };
        let back_vec_ptr_1: *mut CursorVec = &mut back_vec_1;

        let backing_store_0 = back_vec_0.inner.get_ref();
        let backing_store_1 = back_vec_1.inner.get_ref();

        // Intentional unsafe access to later peek underlying storage
        let mut backed = BackedArray::new();
        unsafe {
            backed.append(FIB, &mut *back_vec_ptr_0).unwrap();
        }
        assert_eq!(&backing_store_0[backing_store_0.len() - FIB.len()..], FIB);

        // Intentional unsafe access to later peek underlying storage
        unsafe {
            backed.append(INPUT_1, &mut *back_vec_ptr_1).unwrap();
        }
        assert_eq!(
            &backing_store_1[backing_store_1.len() - INPUT_1.len()..],
            INPUT_1
        );

        let handle = backed.chunk_mod_iter(0);

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
            backed.item_iter(0).collect::<Result<Vec<_>, _>>().unwrap(),
            [&20, &1, &30, &5, &7, &40, &5, &7]
        );
    }

    #[test]
    fn length_checking() {
        let back_vector_0 = CursorVec {
            inner: &mut Cursor::new(Vec::with_capacity(3)),
        };
        let back_vector_1 = CursorVec {
            inner: &mut Cursor::new(Vec::with_capacity(3)),
        };

        const INPUT_0: &[u8] = &[0, 1, 1];
        const INPUT_1: &[u8] = &[2, 5, 7];

        let mut backed = BackedArray::new();
        backed.append(INPUT_0, back_vector_0).unwrap();
        backed.append_memory(INPUT_1, back_vector_1).unwrap();

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
        backed
            .get_multiple([0, 4])
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(backed.loaded_len(), 6);
        backed.shrink_to_query(&[4]);
        assert_eq!(backed.loaded_len(), 3);
        backed.clear_memory();
        assert_eq!(backed.loaded_len(), 0);
    }
}
