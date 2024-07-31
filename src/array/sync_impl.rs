use std::{
    io::{Read, Seek, Write},
    ops::Range,
};

use anyhow::anyhow;
use bincode::{deserialize_from, serialize_into};
use derive_getters::Getters;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::entry::{
    BackedEntry, BackedEntryArr, BackedEntryMut, BackedEntryUnload, DiskOverwritable,
};

use super::{internal_idx, multiple_internal_idx, multiple_internal_idx_strict, BackedArrayError};

/// Array stored as multiple arrays on disk
///
/// Associates each access with the appropriate disk storage, loading it into
/// memory and returning the value. Subsequent accesses will use the in-memory
/// store. Use [`Self::clear_memory`] to move the cached sub-arrays back out of
/// memory.
///
/// Use [`Self::save_to_disk`] instead of serialization directly. This clears
/// entries to prevent data duplication on disk.
///
/// Modifications beyond appending and removal are not supported, due to
/// complexity.
#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
pub struct BackedArray<T, Disk> {
    // keys and entries must always have the same length
    // keys must always be sorted min-max
    keys: Vec<Range<usize>>,
    entries: Vec<BackedEntryArr<T, Disk>>,
}

impl<T, Disk> Default for BackedArray<T, Disk> {
    fn default() -> Self {
        Self {
            keys: vec![],
            entries: vec![],
        }
    }
}

impl<T, Disk> BackedArray<T, Disk> {
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
}

impl<T: DeserializeOwned + Clone, Disk: Read + Seek> From<BackedArray<T, Disk>>
    for bincode::Result<Box<[T]>>
{
    fn from(mut val: BackedArray<T, Disk>) -> Self {
        Ok(val
            .entries
            .iter_mut()
            .map(|entry| entry.load())
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .cloned()
            .collect())
    }
}

/// Read implementations
impl<T: DeserializeOwned, Disk: Read + Seek> BackedArray<T, Disk> {
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

    /// [`Self::get_multiple`], but returns Errors for invalid idx
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
        self.entries.get_mut(idx).map(|arr| arr.load())
    }
}

/// Write implementations
impl<T: Serialize, Disk: Write> BackedArray<T, Disk> {
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
    /// let file_0 = OpenOptions::new().read(true).write(true).create(true).open(FILENAME_BASE.clone().join("_0")).unwrap();
    /// let file_1 = OpenOptions::new().read(true).write(true).create(true).open(FILENAME_BASE.join("_1")).unwrap();
    /// let mut array: BackedArray<u32, _> = BackedArray::default();
    /// array.append(&values.0, file_0);
    /// array.append(&values.1, file_1);
    ///
    /// assert_eq!(array.get(4).unwrap(), &3);
    /// remove_dir_all(FILENAME_BASE).unwrap();
    /// ```
    pub fn append(&mut self, values: &[T], backing_store: Disk) -> bincode::Result<&mut Self> {
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
    /// let file = OpenOptions::new().read(true).write(true).create(true).open(FILENAME.clone()).unwrap();
    /// array.append_memory(Box::new(values.0), file);
    ///
    /// // Overwrite file, making disk pointer for first array invalid
    /// let file = OpenOptions::new().read(true).write(true).create(true).open(FILENAME.clone()).unwrap();
    /// array.append_memory(Box::new(values.1), file);
    ///
    /// assert_eq!(array.get(0).unwrap(), &0);
    /// remove_file(FILENAME).unwrap();
    /// ```
    pub fn append_memory(
        &mut self,
        values: Box<[T]>,
        backing_store: Disk,
    ) -> bincode::Result<&mut Self> {
        // End of a range is exclusive
        let start_idx = self.keys.last().map(|key_range| key_range.end).unwrap_or(0);
        self.keys.push(start_idx..(start_idx + values.len()));
        let mut entry = BackedEntryArr::new(backing_store);
        entry.write(values)?;
        self.entries.push(entry);
        Ok(self)
    }
}

impl<T: Serialize + DeserializeOwned, Disk: Write + Read + Seek + DiskOverwritable>
    BackedArray<T, Disk>
{
    /// Returns a mutable handle to a given chunk.
    pub fn get_chunk_mut(
        &mut self,
        idx: usize,
    ) -> Option<bincode::Result<BackedEntryMut<Box<[T]>, Disk>>> {
        self.entries.get_mut(idx).map(|arr| arr.mut_handle())
    }
}

impl<T, Disk> BackedArray<T, Disk> {
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

impl<T: Serialize, Disk: Serialize> BackedArray<T, Disk> {
    /// Serializes this to disk after clearing all entries from memory to disk.
    ///
    /// Writing directly from serialize wastes disk space and increases load
    /// time by writing the backed data with this struct.
    pub fn save_to_disk<W: Write>(&mut self, writer: W) -> bincode::Result<()> {
        self.clear_memory();
        serialize_into(writer, self)
    }
}

impl<T: DeserializeOwned, Disk: DeserializeOwned> BackedArray<T, Disk> {
    /// Loads the backed array. Does not load backing arrays, unless this
    /// was saved with data (was not saved with [`Self::save_to_disk`]).
    pub fn load<R: Read>(writer: &mut R) -> bincode::Result<Self> {
        deserialize_from(writer)
    }
}

impl<T: Clone, Disk: Clone> BackedArray<T, Disk> {
    /// Combine `self` and `rhs` into a new [`Self`]
    ///
    /// Appends entries of `self` and `rhs`
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

    /// Copy entries in `rhs` to `self`
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

impl<T, Disk> BackedArray<T, Disk> {
    /// Moves all entries of `rhs` into `self`
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

/// Iterator over all underlying chunks.
///
/// Keeps all loaded chunks in memory while alive.
///
/// # Arguments
/// * `pos`: Current position in chunks
/// * `backed_array`: The underlying array
pub struct BackedEntryChunkIter<'a, T, Disk> {
    pos: usize,
    backed_array: &'a mut BackedArray<T, Disk>,
}

impl<'a, T: DeserializeOwned, Disk: Read + Seek> Iterator for BackedEntryChunkIter<'a, T, Disk> {
    type Item = bincode::Result<&'a [T]>;

    fn next(&mut self) -> Option<Self::Item> {
        // No GAT support, so we make do with pointers
        // The lifetimes work out, but the compiler doesn't know that
        let val_ptr: bincode::Result<*const [T]> = self
            .backed_array
            .entries
            .get_mut(self.pos)
            .map(move |x| x.load().map(|y| y as *const [T]))?;
        self.pos += 1;
        Some(val_ptr.map(|x| unsafe { &*x }))
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.pos += n;
        self.next()
    }
}

/// Iterator over that gives modifiable handles to all underlying chunks.
///
/// Keeps all loaded chunks in memory while alive.
///
/// # Arguments
/// * `pos`: Current position in chunks
/// * `backed_array`: The underlying array
pub struct BackedEntryChunkModIter<'a, T, Disk> {
    pos: usize,
    backed_array: &'a mut BackedArray<T, Disk>,
}

impl<'a, T: Serialize + DeserializeOwned, Disk: Write + Read + Seek + DiskOverwritable> Iterator
    for BackedEntryChunkModIter<'a, T, Disk>
{
    type Item = bincode::Result<BackedEntryMut<'a, Box<[T]>, Disk>>;

    fn next(&mut self) -> Option<Self::Item> {
        // No GAT support, so we make do with pointers
        // The lifetimes work out, but the compiler doesn't know that
        let entry_ptr = self
            .backed_array
            .entries
            .get_mut(self.pos)
            .map(move |x| x as *mut BackedEntry<Box<[T]>, Disk>)?;
        let val = unsafe { (*entry_ptr).mut_handle() };
        self.pos += 1;
        Some(val)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.pos += n;
        self.next()
    }
}

/// Iterator that clones all chunks.
///
/// Each chunk is loaded for cloning, then immediately unloaded.
///
/// # Arguments
/// * `pos`: Current position in chunks
/// * `backed_array`: The underlying array
pub struct BackedEntryDupIter<'a, T, Disk> {
    pos: usize,
    backed_array: &'a mut BackedArray<T, Disk>,
}

impl<'a, T: DeserializeOwned + Clone, Disk: Read + Seek> Iterator
    for BackedEntryDupIter<'a, T, Disk>
{
    type Item = bincode::Result<Box<[T]>>;

    fn next(&mut self) -> Option<Self::Item> {
        let chunk = self
            .backed_array
            .get_chunk(self.pos)
            .map(|res| res.map(|val| val.into()));

        if let Some(ref res) = chunk {
            if res.is_ok() {
                self.pos += 1;
            }
        }

        chunk
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.pos += n;
        self.next()
    }
}

/// Iterator over all stored items.
///
/// Keeps all loaded values in memory while alive.
///
/// # Arguments
/// * `inner_pos`: Current position inside chunk
/// * `backed_array`: The underlying array
/// * `chunk_iter`: [`BackedEntryChunkIter`] used to load backing values
pub struct BackedEntryItemIter<'a, T, Disk> {
    inner_pos: usize,
    backed_array: Option<bincode::Result<&'a [T]>>,
    chunk_iter: BackedEntryChunkIter<'a, T, Disk>,
}

impl<'a, T: DeserializeOwned + 'a, Disk: Read + Seek> BackedEntryItemIter<'a, T, Disk> {
    fn new(inner_pos: usize, mut chunk_iter: BackedEntryChunkIter<'a, T, Disk>) -> Self {
        let backed_array = chunk_iter.next();
        Self {
            inner_pos,
            backed_array,
            chunk_iter,
        }
    }
}

impl<'a, T: DeserializeOwned + 'a, Disk: Read + Seek> Iterator
    for BackedEntryItemIter<'a, T, Disk>
{
    type Item = anyhow::Result<&'a T>;

    fn next(&mut self) -> Option<Self::Item> {
        // No GAT support, so we make do with pointers
        // The lifetimes work out, but the compiler doesn't know that
        // Also need to perform a secondary borrows inside a map
        let self_ptr: *mut Self = self;
        let val_ptr = self
            .backed_array
            .as_ref()
            .map(|entry| {
                entry.as_ref().map(|loaded_entry| {
                    loaded_entry
                        .get(self.inner_pos)
                        .map(|x| {
                            self.inner_pos += 1;
                            Ok(x)
                        })
                        .or_else(|| {
                            self.inner_pos = 0;

                            // Mutating the backed entry,
                            // but the borrowed value is being discarded
                            unsafe {
                                (*self_ptr).backed_array = self.chunk_iter.next();
                            };

                            // Mutating self to call method again
                            unsafe { (*self_ptr).next() }
                        })
                })
            })
            .transpose()
            .map(|x| {
                x.flatten()
                    .map(|val| val.map(|checked_val| checked_val as *const T))
            })
            .transpose();
        let val_ptr = val_ptr.map(move |y| match y {
            Ok(y_inner) => y_inner,
            Err(y_inner) => Err(anyhow!("{:?}", y_inner)),
        });

        // Deref to pointer to make lifetimes resolve
        val_ptr.map(|y| y.map(|x| unsafe { &*x }))
    }
}

// Iterator returns
impl<T: DeserializeOwned, Disk: Read + Seek> BackedArray<T, Disk> {
    /// Return a [`BackedEntryItemIter`] that outputs items in order.
    ///
    /// # Arguments
    /// * `offset`: Starting position
    pub fn item_iter(&mut self, offset: usize) -> BackedEntryItemIter<T, Disk> {
        // Defaults to force a None return from iterator
        let mut outer_pos = usize::MAX;
        let mut inner_pos = usize::MAX;

        if let Some(loc) = internal_idx(&self.keys, offset) {
            outer_pos = loc.entry_idx;
            inner_pos = loc.inside_entry_idx;
        };

        BackedEntryItemIter::new(inner_pos, self.chunk_iter(outer_pos))
    }

    /// Default for [`BackedEntryItemIter`].
    ///
    /// Starts at entry 0.
    pub fn item_iter_default(&mut self) -> BackedEntryItemIter<T, Disk> {
        self.item_iter(0)
    }

    /// Return a [`BackedEntryChunkIter`] that outputs underlying chunks
    /// in order.
    ///
    /// # Arguments
    /// * `offset`: Starting chunk
    pub fn chunk_iter(&mut self, offset: usize) -> BackedEntryChunkIter<T, Disk> {
        BackedEntryChunkIter {
            pos: offset,
            backed_array: self,
        }
    }

    /// Default for [`BackedEntryChunkIter`].
    ///
    /// Starts at chunk 0.
    pub fn chunk_iter_default(&mut self) -> BackedEntryChunkIter<T, Disk> {
        self.chunk_iter(0)
    }
}

impl<T: Serialize + DeserializeOwned, Disk: Write + Read + Seek + DiskOverwritable>
    BackedArray<T, Disk>
{
    /// Return a [`BackedEntryChunkModIter`] that provides mutable handles to
    /// underlying chunks, using [`BackedEntryMut`].
    ///
    /// See [`Self::chunk_iter`] for the immutable iterator.
    pub fn chunk_mod_iter(&mut self, offset: usize) -> BackedEntryChunkModIter<T, Disk> {
        BackedEntryChunkModIter {
            pos: offset,
            backed_array: self,
        }
    }

    /// Default for [`BackedEntryChunkModIter`].
    ///
    /// Starts at chunk 0.
    pub fn chunk_mod_iter_default(&mut self) -> BackedEntryChunkModIter<T, Disk> {
        self.chunk_mod_iter(0)
    }
}

impl<T: DeserializeOwned + Clone, Disk: Read + Seek> BackedArray<T, Disk> {
    /// Returns a [`BackedEntryDupIter`] that outputs clones of chunk data.
    ///
    /// See [`Self::chunk_iter`] for a version that produces references.
    pub fn dup_iter(&mut self, offset: usize) -> BackedEntryDupIter<T, Disk> {
        BackedEntryDupIter {
            pos: offset,
            backed_array: self,
        }
    }

    /// Default for [`BackedEntryDupIter`].
    ///
    /// Starts at chunk 0.
    pub fn dup_iter_default(&mut self) -> BackedEntryDupIter<T, Disk> {
        self.dup_iter(0)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::test_utils::CursorVec;

    use super::*;

    #[test]
    fn multiple_retrieve() {
        let back_vector_1 = Cursor::new(Vec::with_capacity(3));
        let back_vector_2 = Cursor::new(Vec::with_capacity(3));

        const INPUT_1: [u8; 3] = [0, 1, 1];
        const INPUT_2: [u8; 3] = [2, 3, 5];

        let mut backed = BackedArray::new();
        backed.append(&INPUT_1, back_vector_1).unwrap();
        backed
            .append_memory(Box::new(INPUT_2), back_vector_2)
            .unwrap();

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
        let back_vector = Cursor::new(Vec::with_capacity(3));

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
        let back_vector_0 = Cursor::new(Vec::with_capacity(3));
        let back_vector_1 = Cursor::new(Vec::with_capacity(3));

        const INPUT_0: &[u8] = &[0, 1, 1];
        const INPUT_1: &[u8] = &[2, 5, 7];

        let mut backed = BackedArray::new();
        backed.append(INPUT_0, back_vector_0).unwrap();
        backed.append(INPUT_1, back_vector_1).unwrap();

        let collected = backed
            .chunk_iter_default()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(collected[0], INPUT_0);
        assert_eq!(collected[1], INPUT_1);
        assert_eq!(collected.len(), 2);
    }

    #[test]
    fn item_iteration() {
        let back_vector_0 = Cursor::new(Vec::with_capacity(3));
        let back_vector_1 = Cursor::new(Vec::with_capacity(3));

        const INPUT_0: &[u8] = &[0, 1, 1];
        const INPUT_1: &[u8] = &[2, 5, 7];

        let mut backed = BackedArray::new();
        backed.append(INPUT_0, back_vector_0).unwrap();
        backed.append(INPUT_1, back_vector_1).unwrap();
        let collected = backed
            .item_iter_default()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(collected[5], &7);
        assert_eq!(collected[2], &1);
        assert_eq!(collected.len(), 6);
    }

    #[test]
    fn dup_iteration() {
        let back_vector_0 = Cursor::new(Vec::with_capacity(3));
        let back_vector_1 = Cursor::new(Vec::with_capacity(3));

        const INPUT_0: &[u8] = &[0, 1, 1];
        const INPUT_1: &[u8] = &[2, 5, 7];

        let mut backed = BackedArray::new();
        backed.append(INPUT_0, back_vector_0).unwrap();
        backed.append(INPUT_1, back_vector_1).unwrap();

        let collected = backed
            .dup_iter_default()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(collected[0].as_ref(), INPUT_0);
        assert_eq!(collected[1].as_ref(), INPUT_1);
        assert_eq!(collected.len(), 2);
    }

    #[test]
    fn mod_iteration() {
        const FIB: &[u8] = &[0, 1, 1, 5, 7];
        const INPUT_1: &[u8] = &[2, 5, 7];

        let mut back_vec_0 = CursorVec(Cursor::new(Vec::with_capacity(10)));
        let back_vec_ptr_0: *mut CursorVec = &mut back_vec_0;
        let mut back_vec_1 = CursorVec(Cursor::new(Vec::with_capacity(10)));
        let back_vec_ptr_1: *mut CursorVec = &mut back_vec_1;

        let backing_store_0 = back_vec_0.0.get_ref();
        let backing_store_1 = back_vec_1.0.get_ref();

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

        let handle = backed.chunk_mod_iter_default();

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
            backed
                .item_iter_default()
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            [&20, &1, &30, &5, &7, &40, &5, &7]
        );
    }
}
