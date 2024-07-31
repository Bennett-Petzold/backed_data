use std::{
    fmt::Debug,
    iter::{FusedIterator, Peekable},
    ops::{Deref, DerefMut, Range},
    sync::{Arc, Mutex},
};

use crate::{
    entry::{
        sync_impl::{BackedEntryMut, BackedEntryRead, BackedEntryWrite},
        BackedEntry,
    },
    utils::{BorrowExtender, BorrowExtenderMut},
};

use super::{
    container::{
        open_mut, open_ref, BackedEntryContainer, BackedEntryContainerNested,
        BackedEntryContainerNestedAll, BackedEntryContainerNestedRead,
        BackedEntryContainerNestedWrite, Container, ResizingContainer,
    },
    internal_idx, multiple_internal_idx_strict, BackedArray, BackedArrayError,
};

impl<K, E: BackedEntryContainerNested> BackedArray<K, E> {
    /// Move all backing arrays out of memory.
    pub fn clear_memory(&mut self) {
        self.entries.c_mut().as_mut().iter_mut().for_each(|entry| {
            let entry = entry.get_mut();
            BackedEntryContainer::get_mut(entry).unload()
        });
    }

    /// Move the chunk at `idx` out of memory.
    pub fn clear_chunk(&mut self, idx: usize) {
        if let Some(mut val) = self.entries.c_get_mut(idx) {
            let entry = val.as_mut().get_mut();
            BackedEntryContainer::get_mut(entry).unload()
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
            .c_ref()
            .as_ref()
            .iter()
            .zip(self.keys.c_ref().as_ref())
            .filter(|(ent, _)| ent.get_ref().is_loaded())
            .map(|(_, key)| key.len())
            .sum()
    }
}

impl<K: Container<Data = Range<usize>>, E: BackedEntryContainerNested> BackedArray<K, E> {
    /// Removes all backing stores not needed to hold `idxs` in memory.
    pub fn shrink_to_query(&mut self, idxs: &[usize]) {
        self.keys
            .c_ref()
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
        let loc = internal_idx(self.keys.c_ref().as_ref(), idx)
            .ok_or(BackedArrayError::OutsideEntryBounds(idx))?;

        let entry_container = BorrowExtender::try_new(&self.entries, |entries| {
            entries
                .c_get(loc.entry_idx)
                .ok_or(BackedArrayError::OutsideEntryBounds(idx))
        })?;
        let entry = BorrowExtender::try_new(entry_container, |entry_container| {
            BackedEntryContainer::get_ref(open_ref!(entry_container))
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
    /// Returns Errors for invalid idx and load issues.
    pub fn get_multiple<'a, I>(
        &'a self,
        idxs: I,
    ) -> impl Iterator<Item = Result<<E::Unwrapped as Container>::Ref<'_>, BackedArrayError<E::ReadError>>>
    where
        I: IntoIterator<Item = usize> + Clone + 'a,
    {
        multiple_internal_idx_strict(self.keys.c_ref(), idxs.clone())
            .zip(idxs)
            .map(|(loc, idx)| {
                let loc = loc.ok_or(BackedArrayError::OutsideEntryBounds(idx))?;
                let entry_container = BorrowExtender::try_new(&self.entries, |entries| {
                    entries
                        .c_get(loc.entry_idx)
                        .ok_or(BackedArrayError::OutsideEntryBounds(idx))
                })?;
                let entry = BorrowExtender::try_new(entry_container, |entry_container| {
                    BackedEntryContainer::get_ref(open_ref!(entry_container))
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
    ///     array::VecBackedArray,
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
            .c_ref()
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
    /// #[cfg(feature = "bincode")] {
    ///     use backed_data::{
    ///         array::VecBackedArray,
    ///         entry::{
    ///             disks::Plainfile,
    ///             formats::BincodeCoder,
    ///         },
    ///     };
    ///     use std::fs::{File, remove_file, OpenOptions};
    ///
    ///     let FILENAME = std::env::temp_dir().join("example_array_append_memory");
    ///     let values = ([0, 1, 1],
    ///         [2, 3, 5]);
    ///
    ///     let mut array: VecBackedArray<u32, Plainfile, _> = VecBackedArray::default();
    ///     array.append_memory(values.0, FILENAME.clone().into(), BincodeCoder {});
    ///
    ///     // Overwrite file, making disk pointer for first array invalid
    ///     array.append_memory(values.1, FILENAME.clone().into(), BincodeCoder {});
    ///
    ///     assert_eq!(array.get(0).unwrap().as_ref(), &0);
    ///     remove_file(FILENAME).unwrap();
    /// }
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
            .c_ref()
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
    /// The getter functions can be used to identify the target index.
    pub fn remove(&mut self, entry_idx: usize) -> &mut Self {
        // This split is necessary to solve lifetimes
        let width = self.keys.c_get(entry_idx).map(|entry| entry.as_ref().len());

        if let Some(width) = width {
            self.keys.c_remove(entry_idx);
            self.entries.c_remove(entry_idx);

            // Shift all later ranges downwards
            self.keys
                .c_mut()
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
        let offset = self.keys.c_ref().as_ref().last().unwrap_or(&(0..0)).end;
        self.keys.extend(
            rhs.keys
                .c_ref()
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

/// Handle to a backed value that flushes on drop.
///
/// This automatically flushes when it is the last reference remaining.
pub struct CountedHandle<'a, E: BackedEntryWrite + BackedEntryRead, V> {
    handle: Arc<Mutex<Result<BackedEntryMut<'a, E>, E::ReadError>>>,
    value: V,
}

impl<E: BackedEntryWrite + BackedEntryRead, V> CountedHandle<'_, E, V> {
    pub fn flush(&self) -> Result<(), E::WriteError> {
        let mut h = self.handle.lock().unwrap();
        if let Ok(h) = h.deref_mut() {
            h.flush().map(|_| {})
        } else {
            Ok(())
        }
    }
}

impl<E: BackedEntryWrite + BackedEntryRead, V> Deref for CountedHandle<'_, E, V> {
    type Target = V;
    fn deref(&self) -> &V {
        &self.value
    }
}

impl<E: BackedEntryWrite + BackedEntryRead, V> DerefMut for CountedHandle<'_, E, V> {
    fn deref_mut(&mut self) -> &mut V {
        &mut self.value
    }
}

impl<E: BackedEntryWrite + BackedEntryRead, V> Drop for CountedHandle<'_, E, V> {
    fn drop(&mut self) {
        if Arc::strong_count(&self.handle) == 1 {
            let mut h = self.handle.lock().unwrap();
            if let Ok(h) = h.deref_mut() {
                h.flush()
                    .map_err(|_| "Failed to drop handle in CountedHandle!")
                    .unwrap();
            }
        }
    }
}

/// Iterates over a backed array, returning each item in order.
///
/// To keep an accurate size count, failed reads will not be retried.
/// This will keep each disk loaded after pulling data from it.
/// Stepping by > 1 with `nth` implementation may skip loading a disk.
pub struct BackedArrayIterMut<'a, K: Container + 'a, E: BackedEntryContainerNestedAll + 'a> {
    pos: usize,
    len: usize,
    keys: BorrowExtender<<K as Container>::RefSlice<'a>, Peekable<std::slice::Iter<'a, K::Data>>>,
    entries: BorrowExtenderMut<<E as Container>::MutSlice<'a>, std::slice::IterMut<'a, E::Data>>,
    // TODO: Rewrite this to be a box of Once or MaybeUninit type
    // Problem with once types is that either a generic needs to be introduced,
    // or this iterator needs to choose between cell/lock tradeoffs.
    #[allow(clippy::type_complexity)]
    handles: Vec<
        Arc<
            Mutex<
                Result<
                    BackedEntryMut<'a, BackedEntry<E::OnceWrapper, E::Disk, E::Coder>>,
                    E::ReadError,
                >,
            >,
        >,
    >,
}

impl<'a, K: Container<Data = Range<usize>>, E: BackedEntryContainerNestedAll>
    BackedArrayIterMut<'a, K, E>
{
    fn new(backed: &'a mut BackedArray<K, E>) -> Self {
        let len = backed.keys.c_ref().as_ref().last().unwrap_or(&(0..0)).end;
        let mut handles = Vec::with_capacity(backed.chunks_len());
        let keys = BorrowExtender::new(backed.keys.c_ref(), |keys| {
            let keys_ptr: *const _ = keys;
            unsafe { &*keys_ptr }.as_ref().iter().peekable()
        });
        let mut entries = BorrowExtenderMut::new(backed.entries.c_mut(), |ent| {
            let ent_ptr: *mut _ = ent;
            unsafe { &mut *ent_ptr }.as_mut().iter_mut()
        });

        if let Some(entry) = entries.by_ref().next() {
            handles.push(Arc::new(Mutex::new(entry.get_mut().mut_handle())));
        }

        Self {
            pos: 0,
            len,
            keys,
            handles,
            entries,
        }
    }

    /// Flush all opened handles.
    pub fn flush(&mut self) -> Result<&mut Self, E::WriteError> {
        for h in self.handles.iter_mut() {
            let mut h = h.lock().unwrap();
            if let Ok(h) = h.deref_mut() {
                h.flush()?;
            }
        }
        Ok(self)
    }
}

impl<K: Container, E: BackedEntryContainerNestedAll> Drop for BackedArrayIterMut<'_, K, E> {
    fn drop(&mut self) {
        for h in self.handles.iter_mut() {
            if Arc::strong_count(h) == 1 {
                let mut h = h.lock().unwrap();
                if let Ok(h) = h.deref_mut() {
                    h.flush()
                        .map_err(|_| "Failed to drop handle in BackedArrayIterMut!")
                        .unwrap();
                }
            }
        }
    }
}

impl<'a, K: Container<Data = Range<usize>> + 'a, E: BackedEntryContainerNestedAll + 'a> Iterator
    for BackedArrayIterMut<'a, K, E>
where
    E::Unwrapped: 'a,
    E::ReadError: 'a,
    E::OnceWrapper: 'a,
{
    type Item = Result<
        CountedHandle<
            'a,
            BackedEntry<E::OnceWrapper, E::Disk, E::Coder>,
            <E::Unwrapped as Container>::Mut<'a>,
        >,
        &'a E::ReadError,
    >;

    fn next(&mut self) -> Option<Self::Item> {
        self.nth(0)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.pos += n;

        // # of steps forward taken by keys
        let mut step_count = 0;

        // Remove all key positions prior to `pos`.
        // This iterator only goes forward, so keys are not reused once
        // passed.
        while let Some(key) = self.keys.by_ref().peek() {
            if key.end > self.pos {
                // Update to latest entry handle
                break;
            }
            let _ = self.keys.by_ref().next();
            step_count += 1;
        }

        let inner_pos = self.pos - self.keys.peek()?.start;
        self.pos += 1; // Position advances, even on errors

        if step_count > 0 {
            let entry = self.entries.nth(step_count - 1).unwrap().get_mut();
            self.handles.push(Arc::new(Mutex::new(entry.mut_handle())));
        }

        // This actually extends lifetimes too long, beyond lifetime of struct
        let handle_ptr: *mut _ = self.handles.last_mut().unwrap();
        let handle = unsafe { &mut *handle_ptr };
        let mut handle_locked = handle.lock().unwrap();

        match &mut *handle_locked {
            Ok(h) => {
                let h: *mut _ = h;
                unsafe { &mut *h }.c_get_mut(inner_pos).map(|h| {
                    Ok(CountedHandle {
                        handle: handle.clone(),
                        value: h,
                    })
                })
            }
            Err(e) => {
                let e: *const _ = e;
                Some(Err(unsafe { &*e }))
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.len - self.pos;
        (remaining, Some(remaining))
    }
}

impl<'a, K: Container<Data = Range<usize>> + 'a, E: BackedEntryContainerNestedAll + 'a>
    FusedIterator for BackedArrayIterMut<'a, K, E>
where
    E::Unwrapped: 'a,
    E::ReadError: 'a,
    E::OnceWrapper: 'a,
{
}

impl<K: Container<Data = Range<usize>>, E: BackedEntryContainerNestedRead> BackedArray<K, E> {
    /// Provides a [`BackedArrayIter`] over each backed item.
    pub fn iter(&self) -> BackedArrayIter<'_, K, E> {
        BackedArrayIter::new(self)
    }

    /// Returns underlying chunks in order.
    ///
    /// This will load each chunk before providing it.
    /// All chunks loaded earlier in the iteration will remain loaded.
    pub fn chunk_iter(
        &self,
    ) -> impl Iterator<Item = Result<impl AsRef<&E::Unwrapped>, E::ReadError>> {
        self.entries.ref_iter().map(|arr| {
            BorrowExtender::new(arr, |arr| {
                let arr_ptr: *const _ = arr;
                unsafe { &*arr_ptr }.as_ref().get_ref().load()
            })
            .open_result()
        })
    }
}

impl<K: Container<Data = Range<usize>>, E: BackedEntryContainerNestedAll> BackedArray<K, E> {
    /// Returns [`BackedArrayIterMut`], which automatically tracks and forces
    /// flushes when mutated values are dropped. Both the iterator and its
    /// handles also provide manually callable flush methods.
    pub fn iter_mut<'a>(&'a mut self) -> BackedArrayIterMut<'a, K, E>
    where
        E::Unwrapped: 'a,
        E::ReadError: 'a,
    {
        BackedArrayIterMut::new(self)
    }

    /// Provides mutable handles to underlying chunks, using [`BackedEntryMut`].
    ///
    /// See [`Self::chunk_iter`] for the immutable iterator.
    pub fn chunk_mut_iter(
        &mut self,
    ) -> impl Iterator<
        Item = Result<
            impl AsMut<BackedEntryMut<'_, BackedEntry<E::OnceWrapper, E::Disk, E::Coder>>>,
            E::ReadError,
        >,
    > {
        self.entries.mut_iter().map(|arr| {
            let arr = BorrowExtenderMut::new(arr, |arr| {
                let arr: *mut _ = arr.as_mut();
                unsafe { &mut *arr }
            });
            let arr = BorrowExtenderMut::new(arr, |arr| {
                let arr: *mut _ = arr.get_mut().get_mut();
                unsafe { &mut *arr }
            });
            BorrowExtenderMut::new(arr, |arr| {
                let arr: *mut _ = arr.get_mut();
                unsafe { &mut *arr }.mut_handle()
            })
            .open_result()
        })
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

    use crate::{entry::formats::BincodeCoder, test_utils::cursor_vec, test_utils::CursorVec};

    use super::super::*;

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
            backed
                .get_multiple([0, 2, 4, 5])
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            [&0, &1, &3, &5]
        );
        assert_eq!(
            backed
                .get_multiple([5, 2, 0, 5])
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
            .get_multiple([0, 10])
            .collect::<Result<Vec<_>, _>>()
            .is_err());
        assert_eq!(
            backed.get_multiple([0, 10]).filter(|x| x.is_ok()).count(),
            1
        );
        assert_eq!(
            backed.get_multiple([20, 10]).filter(|x| x.is_ok()).count(),
            0
        );
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
        assert_eq!(collected[0].as_ref().as_ref(), INPUT_0);
        assert_eq!(collected[1].as_ref().as_ref(), INPUT_1);
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
        let collected = backed.iter().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(collected[5], &7);
        assert_eq!(collected[2], &1);
        assert_eq!(collected.len(), 6);
    }

    #[test]
    fn mod_iteration() {
        const FIB: &[u8] = &[0, 1, 1, 5, 7];
        const INPUT_1: &[u8] = &[2, 5, 7];

        cursor_vec!(back_vec_0, backing_store_0);
        cursor_vec!(back_vec_1, backing_store_1);

        // Intentional unsafe access to later peek underlying storage
        let mut backed = VecBackedArray::new();
        unsafe {
            backed
                .append(FIB, &mut *back_vec_0.get(), BincodeCoder {})
                .unwrap();
        }
        #[cfg(not(miri))]
        assert_eq!(&backing_store_0[backing_store_0.len() - FIB.len()..], FIB);

        // Intentional unsafe access to later peek underlying storage
        unsafe {
            backed
                .append(INPUT_1, &mut *back_vec_1.get(), BincodeCoder {})
                .unwrap();
        }
        #[cfg(not(miri))]
        assert_eq!(
            &backing_store_1[backing_store_1.len() - INPUT_1.len()..],
            INPUT_1
        );

        let handle = backed.chunk_mut_iter();

        let mut handle_vec = handle.collect::<Result<Vec<_>, _>>().unwrap();
        handle_vec[0].as_mut()[0] = 20;
        handle_vec[0].as_mut()[2] = 30;
        handle_vec[1].as_mut()[0] = 40;

        handle_vec[0].as_mut().flush().unwrap();
        handle_vec[1].as_mut().flush().unwrap();
        #[cfg(not(miri))]
        {
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
        }

        drop(handle_vec);
        assert_eq!(
            backed.iter().collect::<Result<Vec<_>, _>>().unwrap(),
            [&20, &1, &30, &5, &7, &40, &5, &7]
        );
    }

    #[test]
    fn item_mod_iter() {
        const FIB: &[u8] = &[0, 1, 1, 2, 3, 5];
        const INPUT_1: &[u8] = &[4, 6, 7];

        let after_mod = INPUT_1
            .iter()
            .chain(FIB.iter().skip(INPUT_1.len()))
            .cloned()
            .collect_vec();
        let after_second_mod = INPUT_1.iter().chain(INPUT_1).collect_vec();
        let after_third_mod = INPUT_1.iter().chain(INPUT_1).chain(INPUT_1).collect_vec();

        cursor_vec!(back_vec, back_peek);
        cursor_vec!(back_vec_2, _back_peek_3);

        let mut backed = VecBackedArray::new();
        backed
            .append(FIB, unsafe { &mut *back_vec.get() }, BincodeCoder {})
            .unwrap();

        // Read checks
        assert_eq!(backed.iter_mut().size_hint(), (FIB.len(), Some(FIB.len())));
        assert_eq!(backed.iter_mut().count(), FIB.len());
        assert_eq!(backed.iter_mut().map(|x| **x.unwrap()).collect_vec(), FIB);

        let mut backed_iter = backed.iter_mut();
        for val in INPUT_1 {
            let mut entry = backed_iter.next().unwrap().unwrap();
            **entry = *val;
        }

        // Backed storage is not modified
        #[cfg(not(miri))]
        assert_eq!(&back_peek[back_peek.len() - FIB.len()..], FIB);

        backed_iter.flush().unwrap();

        // Backed storage is now modified
        #[cfg(not(miri))]
        assert_eq!(back_peek[back_peek.len() - after_mod.len()..], after_mod);

        assert_eq!(
            backed_iter.map(|ent| **ent.unwrap()).collect_vec(),
            FIB.iter().skip(INPUT_1.len()).cloned().collect_vec()
        );

        assert_eq!(
            backed.iter_mut().map(|ent| **ent.unwrap()).collect_vec(),
            after_mod
        );

        let mut backed_iter = backed.iter_mut();
        let first = **backed_iter.next().unwrap().unwrap();
        let second = **backed_iter.next().unwrap().unwrap();
        let third = **backed_iter.next().unwrap().unwrap();
        assert_eq!([first, second, third], INPUT_1);
        for val in INPUT_1 {
            let mut entry = backed_iter.next().unwrap().unwrap();
            **entry = *val;
        }

        #[cfg(not(miri))]
        {
            // USAGE FOR TESTING ONLY
            // This breaks handle guarantees about flush on drop.
            // Also leaks memory.
            std::mem::forget(backed_iter);

            // Backed storage is not modified
            assert_eq!(back_peek[back_peek.len() - after_mod.len()..], after_mod);
        }
        #[cfg(miri)]
        drop(backed_iter);

        assert_eq!(
            backed.iter_mut().map(|x| **x.unwrap()).collect_vec(),
            after_second_mod.iter().map(|x| **x).collect_vec(),
        );

        // Backed storage is updated after drop
        #[cfg(not(miri))]
        assert_eq!(
            back_peek[back_peek.len() - after_mod.len()..],
            after_second_mod.iter().map(|x| **x).collect_vec(),
        );

        backed
            .append(INPUT_1, unsafe { &mut *back_vec_2.get() }, BincodeCoder {})
            .unwrap();

        // Correctly crosses multiple storage disks
        assert_eq!(
            backed.iter_mut().map(|x| **x.unwrap()).collect_vec(),
            after_third_mod.iter().map(|x| **x).collect_vec(),
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
