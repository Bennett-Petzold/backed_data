use std::{
    ops::{Deref, DerefMut, Range},
    sync::{Arc, Mutex},
};

use crate::entry::{
    sync_impl::{BackedEntryMut, BackedEntryRead, BackedEntryWrite},
    BackedEntry,
};

use super::{
    container::{
        open_mut, BackedEntryContainer, BackedEntryContainerNested,
        BackedEntryContainerNestedWrite, Container, ResizingContainer,
    },
    BackedArray,
};

pub mod generic;
pub mod slice;

impl<K, E: BackedEntryContainerNested> BackedArray<K, E> {
    /// Move all backing arrays out of memory.
    pub fn clear_memory(&mut self) {
        self.entries
            .c_mut()
            .as_mut()
            .iter_mut()
            .for_each(|entry| entry.get_mut().unload());
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
    /// assert_eq!(*array.get(4).unwrap(), 3);
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
    ///     assert_eq!(*array.get(0).unwrap(), 0);
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

/// Handle to a backed value that flushes on drop.
///
/// This automatically flushes when it is the last reference remaining.
pub struct CountedHandle<'a, E: BackedEntryWrite + BackedEntryRead, V> {
    handle: Arc<Mutex<Result<BackedEntryMut<'a, E>, E::ReadError>>>,
    value: V,
}

impl<'a, E: BackedEntryWrite + BackedEntryRead, V> CountedHandle<'a, E, V> {
    pub fn new(handle: Arc<Mutex<Result<BackedEntryMut<'a, E>, E::ReadError>>>, value: V) -> Self {
        Self { handle, value }
    }
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
        if let Some(handle) = Arc::get_mut(&mut self.handle) {
            let mut h = handle.get_mut().unwrap();
            if let Ok(h) = h.deref_mut() {
                h.flush()
                    .map_err(|_| "Failed to drop handle in CountedHandle!")
                    .unwrap();
            }
        }
    }
}
