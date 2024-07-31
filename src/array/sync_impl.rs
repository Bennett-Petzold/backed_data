use std::{
    cell::{OnceCell, UnsafeCell},
    fmt::Debug,
    io::{Read, Write},
    iter::FusedIterator,
    marker::PhantomData,
    ops::Range,
    rc::Rc,
    sync::{Mutex, MutexGuard},
};

use bincode::{deserialize_from, serialize_into};
use derive_getters::Getters;
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};

use crate::{
    entry::{
        disks::{ReadDisk, WriteDisk},
        formats::{Decoder, Encoder},
        sync_impl::{BackedEntryMut, BackedEntryWrite},
        BackedEntry, BackedEntryArr,
    },
    utils::{AsRefMut, BorrowExtender, Once, ToMut, ToRef},
};

use super::{
    internal_idx, multiple_internal_idx, multiple_internal_idx_strict, open_mut, open_ref,
    ArrayLoc, BackedArrayEntry, BackedArrayError, BackedEntryContainer, BackedEntryContainerNested,
    BackedEntryContainerNestedAll, BackedEntryContainerNestedRead, BackedEntryContainerNestedWrite,
    Container, ResizingContainer,
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

impl<K, E: for<'c> BackedEntryContainerNested<'c>> BackedArray<K, E> {
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

/*
impl<K: Container<Data = Range<usize>>, E: for<'c> BackedEntryContainerNested<'c>>
    BackedArray<K, E>
{
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

impl<K: Container<Data = Range<usize>>, E: for<'c> BackedEntryContainerNested<'c>>
    BackedArray<K, E>
{
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
impl<K: Container<Data = Range<usize>>, E: for<'c> BackedEntryContainerNestedRead<'c>>
    BackedArray<K, E>
{
    /// Return a value (potentially loading its backing array).
    ///
    /// Backing arrays stay in memory until freed.
    pub fn get<'a>(&'a self, idx: usize) -> Result<E::DataRef<'a>, BackedArrayError<E::ReadError>> {
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
    ) -> impl Iterator<Item = impl AsRef<E::InnerData> + '_>
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
    ) -> impl Iterator<Item = Result<impl AsRef<E::InnerData> + '_, BackedArrayError<E::ReadError>>>
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

    /// Returns a reference to a particular underlying chunk.
    ///
    /// See [`Self::get_chunk_mut`] for a modifiable handle.
    pub fn get_chunk(&self, idx: usize) -> Option<Result<&E::Unwrapped, E::ReadError>> {
        self.entries
            .as_ref()
            .get(idx)
            .map(|arr| arr.get_ref().load())
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

impl<K: Container<Data = Range<usize>>, E: BackedEntryContainerNestedAll> BackedArray<K, E> {
    /// Returns a mutable handle to a given chunk.
    pub fn get_chunk_mut(
        &mut self,
        idx: usize,
    ) -> Result<
        BackedEntryMut<'_, BackedEntry<E::OnceWrapper, E::Disk, E::Coder>>,
        BackedArrayError<E::ReadError>,
    > {
        let arr = self
            .entries
            .as_mut()
            .get_mut(idx)
            .ok_or(BackedArrayError::OutsideEntryBounds(idx))?;
        let val = arr.get_mut().mut_handle().map_err(BackedArrayError::Coder);
        val
    }
}

impl<
        K: ResizingContainer<Data = Range<usize>>,
        E: for<'c> BackedEntryContainerNested<'c> + ResizingContainer,
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

impl<K: Serialize, E: BackedEntryContainerNested<'_> + Serialize> BackedArray<K, E> {
    /// Serializes this to disk after clearing all entries from memory to disk.
    ///
    /// Writing directly from serialize wastes disk space and increases load
    /// time by writing the backed data with this struct.
    pub fn save_to_disk<W: Write>(&mut self, writer: W) -> bincode::Result<()> {
        self.clear_memory();
        serialize_into(writer, self)
    }
}

impl<K: for<'de> Deserialize<'de>, E: for<'df> Deserialize<'df>> BackedArray<K, E> {
    /// Loads the backed array. Does not load backing arrays, unless this
    /// was saved with data (was not saved with [`Self::save_to_disk`]).
    pub fn load<R: Read>(writer: &mut R) -> bincode::Result<Self> {
        deserialize_from(writer)
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

/// Returns the next valid location, if any.
///
/// Result::Err on a load violation, None when past all entries
fn step_loc<E: BackedEntryContainerNestedRead>(
    entries: &E,
    mut loc: ArrayLoc, // position inside entries
    step_size: usize,
) -> Result<Option<ArrayLoc>, E::ReadError> {
    if let Some(entry) = entries.c_get(loc.entry_idx) {
        let entry = open_ref!(entry);
        let entry_len = entry.load()?.c_len();
        if loc.inside_entry_idx + step_size < entry_len {
            // Can advance inside entry
            loc.inside_entry_idx += step_size;
            Ok(Some(loc))
        } else {
            // Try going to next entry. Further calls will continue until
            // inside_entry_idx is reduced to < entry_len, or all entries
            // are passed.
            loc.entry_idx += 1;
            loc.inside_entry_idx -= entry_len - step_size;

            step_loc(entries, loc, 0)
        }
    } else {
        // Went beyond last entry
        Ok(None)
    }
}

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
        /*
        let remaining = if let Some(entry_loc) = &self.entry_loc {
            let cur_pos =
                entry_loc.inside_entry_idx + self.backed.keys.as_ref()[entry_loc.entry_idx].start;
            self.backed.len() - cur_pos
        } else {
            0
        };
        (remaining, Some(remaining))
        */
        (0, None)
    }
}

impl<K: Container<Data = Range<usize>>, E: BackedEntryContainerNestedRead> FusedIterator
    for BackedArrayIter<'_, K, E>
{
}

impl<K: Container<Data = Range<usize>>, E: for<'c> BackedEntryContainerNested<'c>>
    BackedArray<K, E>
{
    /// Provides a [`BackedArrayIter`] over backed data.
    ///
    /// # Arguments
    /// * `offset`: Starting position
    pub fn item_iter(&self) -> BackedArrayIter<'_, K, E> {
        BackedArrayIter::new(self)
    }

    /*
    /// Returns underlying chunks in order.
    ///
    /// All chunks loaded earlier in the iteration will remain loaded.
    ///
    /// # Arguments
    /// * `offset`: Starting chunk (skips loading offset - 1 chunks)
    pub fn chunk_iter(
        &mut self,
        offset: usize,
    ) -> impl Iterator<Item = bincode::Result<impl AsRef<[T]> + '_>> {
        self.entries.as_mut().iter_mut().skip(offset).map(|entry| {
            let entry: *mut _ = entry.get_mut().as_mut();
            unsafe { &mut *entry }.load().map(|entry| entry)
        })
    }
    */
}
*/

/*
impl<
        T: Serialize + DeserializeOwned,
        Disk: ReadDisk + WriteDisk,
        Coder,
        K: Container<Data = Range<usize>>,
        E: Container<Data = BackedEntryArr<Coder>>,
    > BackedArray<K, E>
{
    /// Provides mutable handles to underlying chunks, using [`BackedEntryMut`].
    ///
    /// See [`Self::chunk_iter`] for the immutable iterator.
    pub fn chunk_mod_iter(
        &mut self,
        offset: usize,
    ) -> impl Iterator<
        Item = bincode::Result<
            BackedEntryMut<Box<[T]>, Disk, impl AsRefMut<BackedEntry<Box<[T]>, Disk>> + '_>,
        >,
    > {
        self.entries.as_mut().iter_mut().skip(offset).map(|entry| {
            let entry: *mut _ = entry.get_mut().as_mut();
            unsafe { &mut *entry }.mut_handle()
        })
    }
}

impl<
        T: DeserializeOwned + Clone,
        Disk: ReadDisk,
        Coder,
        K: Container<Data = Range<usize>>,
        E: Container<Data = BackedEntryArr<Coder>>,
    > BackedArray<K, E>
{
    /// Returns an iterator that outputs clones of chunk data.
    ///
    /// See [`Self::chunk_iter`] for a version that produces references.
    pub fn dup_iter(
        &mut self,
        offset: usize,
    ) -> impl Iterator<Item = bincode::Result<Vec<T>>> + '_ {
        self.entries.as_mut().iter_mut().skip(offset).map(|entry| {
            let mut entry = entry.get_mut();
            let entry = entry.as_mut();
            let val = entry.load()?.to_vec();
            entry.unload();
            Ok(val)
        })
    }
}

impl<

        Coder,
        K: ResizingContainer<Data = Range<usize>>,
        E: ResizingContainer<Data = BackedEntryArr<Coder>>,
    > BackedArray<K, E>
{
    /// Construct a backed array from entry range, backing storage pairs.
    ///
    /// Not recommended for use outside of VecBackedArray wrappers.
    /// If ranges do not correspond to the entry arrays, the resulting
    /// [`VecBackedArray`] will be invalid.
    pub fn from_pairs<I>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (Range<usize>, BackedEntry<Box<[T]>, Disk>)>,
    {
        let (keys, entries): (_, Vec<_>) = pairs.into_iter().unzip();
        let entries = entries.into_iter().map(|ent| E::Data::wrap(ent)).collect();
        Self {
            keys,
            entries,
            _phantom: (PhantomData, PhantomData),
        }
    }
}

impl<

        Coder,
        K: Container<Data = Range<usize>>,
        E: ResizingContainer<Data = BackedEntryArr<Coder>>,
    > BackedArray<K, E>
{
    /// Replaces the underlying entry disk.
    ///
    /// Not recommended for use outside of VecBackedArray wrappers.
    /// If ranges do not correspond to the entry arrays, the resulting
    /// [`VecBackedArray`] will be invalid.
    pub fn replace_disk<OtherDisk>(self) -> BackedArray< OtherDisk, K, E>
    where
        OtherDisk: for<'de> Deserialize<'de>,
        Disk: Into<OtherDisk>,
        BackedEntryArr< OtherDisk>: Into<BackedEntryArr< Disk>>,
    {
        BackedArray::< OtherDisk, K, E> {
            keys: self.keys,
            entries: self
                .entries
                .into_iter()
                .map(|x| E::Data::wrap(x.inner().replace_disk().into()))
                .collect(),
            _phantom: (PhantomData, PhantomData),
        }
    }
}
*/

/*
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

        let mut backed = VecBackedArray::new();
        backed.append(INPUT_0, back_vector_0).unwrap();
        backed.append_memory(INPUT_1, back_vector_1).unwrap();

        assert_eq!(backed.get(0).unwrap(), &0);
        assert_eq!(backed.get(4).unwrap(), &3);
        assert_eq!((backed.get(0).unwrap(), backed.get(4).unwrap()), (&0, &3));
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

        let mut backed = VecBackedArray::new();
        backed.append(INPU back_vector).unwrap();

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

        let mut backed = VecBackedArray::new();
        backed.append(INPUT_0, back_vector_0).unwrap();
        backed.append(INPUT_1, back_vector_1).unwrap();

        let collected = backed.chunk_iter(0).collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(collected[0].as_ref(), INPUT_0);
        assert_eq!(collected[1].as_ref(), INPUT_1);
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

        let mut backed = VecBackedArray::new();
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

        let mut backed = VecBackedArray::new();
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
        let mut backed = VecBackedArray::new();
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

        let mut backed = VecBackedArray::new();
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
*/
