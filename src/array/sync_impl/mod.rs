use std::{
    ops::{Deref, DerefMut},
    sync::{Arc, Mutex},
};

use crate::entry::{
    disks::WriteDisk,
    formats::Encoder,
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

#[cfg(feature = "unsafe_array")]
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

impl<K: Container<Data = usize>, E: BackedEntryContainerNested> BackedArray<K, E> {
    /// Returns the number of items currently loaded into memory.
    ///
    /// To get memory usage on constant-sized items, multiply this value by
    /// the per-item size.
    pub fn loaded_len(&self) -> usize {
        self.entries
            .c_ref()
            .as_ref()
            .iter()
            .zip(
                self.key_starts
                    .c_ref()
                    .as_ref()
                    .iter()
                    .zip(self.key_ends.c_ref().as_ref()),
            )
            .filter(|(ent, _)| ent.get_ref().is_loaded())
            .map(|(_, (start, end))| end - start)
            .sum()
    }
}

impl<K: Container<Data = usize>, E: BackedEntryContainerNested> BackedArray<K, E> {
    /// Removes all backing stores not needed to hold `idxs` in memory.
    pub fn shrink_to_query(&mut self, idxs: &[usize]) {
        self.key_starts
            .c_ref()
            .as_ref()
            .iter()
            .zip(self.key_ends.c_ref().as_ref())
            .enumerate()
            .filter(|(_, (start, end))| !idxs.iter().any(|idx| (**start..**end).contains(idx)))
            .for_each(|(idx, _)| {
                if let Some(mut v) = self.entries.c_get_mut(idx) {
                    open_mut!(v).unload();
                };
            });
    }
}

/// Write implementations
impl<
        K: ResizingContainer<Data = usize>,
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
    /// array.append(values.0, file_0.into(), BincodeCoder::default());
    /// array.append(values.1, file_1.into(), BincodeCoder::default());
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
        let start_idx = *self.key_ends.c_ref().as_ref().last().unwrap_or(&0);
        self.key_starts.c_push(start_idx);
        self.key_ends.c_push(start_idx + values.c_len());

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
    ///     array.append_memory(values.0, FILENAME.clone().into(), BincodeCoder::default());
    ///
    ///     // Overwrite file, making disk pointer for first array invalid
    ///     array.append_memory(values.1, FILENAME.clone().into(), BincodeCoder::default());
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
        let start_idx = *self.key_ends.c_ref().as_ref().last().unwrap_or(&0);
        self.key_starts.c_push(start_idx);
        self.key_ends.c_push(start_idx + values.c_len());
        let mut entry = BackedEntry::new(backing_store, coder);
        entry.write(values)?;
        self.entries.c_push(entry.into());
        Ok(self)
    }
}

impl<K: ResizingContainer<Data = usize>, E: BackedEntryContainerNested + ResizingContainer>
    BackedArray<K, E>
{
    /// Removes an entry with the internal index, shifting ranges.
    ///
    /// The getter functions can be used to identify the target index.
    pub fn remove(&mut self, entry_idx: usize) -> &mut Self {
        // Need a fn for nice ? syntax
        let width = || {
            let start = self.key_starts.c_get(entry_idx)?;
            let start = start.as_ref();
            let end = self.key_ends.c_get(entry_idx)?;
            let end = end.as_ref();
            Some(*end - *start)
        };
        let width = width();

        if let Some(width) = width {
            self.entries.c_remove(entry_idx);

            // Shift all later ranges downwards
            let key_remove = |this: &mut K| {
                this.c_remove(entry_idx);
                this.c_mut()
                    .as_mut()
                    .iter_mut()
                    .skip(entry_idx)
                    .for_each(|key| {
                        *key -= width;
                    });
            };
            key_remove(&mut self.key_starts);
            key_remove(&mut self.key_ends);
        }

        self
    }
}

impl<K: ResizingContainer<Data = usize>, E: ResizingContainer> BackedArray<K, E> {
    /// Move entries in `rhs` to [`self`].
    pub fn merge(&mut self, mut rhs: Self) -> &mut Self {
        let offset = *self.key_ends.c_ref().as_ref().last().unwrap_or(&0);

        let key_add =
            |this: &mut K, rhs: K| this.extend(rhs.c_ref().as_ref().iter().map(|x| x + offset));

        key_add(&mut self.key_starts, rhs.key_starts);
        key_add(&mut self.key_ends, rhs.key_ends);
        self.entries.c_append(&mut rhs.entries);
        self
    }
}

impl<K: Container<Data = usize>, E: BackedEntryContainerNestedWrite> BackedArray<K, E>
where
    K: From<Vec<K::Data>>,
    E: From<Vec<BackedEntry<E::OnceWrapper, E::Disk, E::Coder>>>,
    E::Disk: Clone,
    E::Coder: Clone,
{
    /// Builds [`self`] from an iterator of containers, keeping them all loaded
    /// in memory.
    pub fn from_containers<C, I>(
        iter: I,
        disk: &E::Disk,
        coder: &E::Coder,
    ) -> Result<Self, <E::Coder as Encoder<<E::Disk as WriteDisk>::WriteDisk>>::Error>
    where
        C: Into<E::Unwrapped>,
        I: IntoIterator<Item = C>,
    {
        let iter = iter.into_iter();

        let mut start = 0;
        let (keys, entries): (Vec<_>, Vec<BackedEntry<E::OnceWrapper, _, _>>) = iter
            .map(|x| x.into())
            .map(|x| {
                let len = x.c_len();
                let mut entry = BackedEntry::new(disk.clone(), coder.clone());
                entry.write(x)?;
                let ret = ((start, start + len), entry);
                start += len;
                Ok(ret)
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .unzip();
        let (key_starts, key_ends): (Vec<_>, Vec<_>) = keys.into_iter().unzip();

        Ok(Self {
            key_starts: key_starts.into(),
            key_ends: key_ends.into(),
            entries: entries.into(),
        })
    }
}

impl<K: Container<Data = usize>, E: BackedEntryContainerNestedWrite> BackedArray<K, E>
where
    K: From<Vec<K::Data>>,
    E: From<Vec<E::Data>>,
    E::Disk: Clone,
    E::Coder: Clone,
{
    /// Builds [`self`] from existing backings.
    ///
    /// # Parameters
    /// * `iter`: Iterator of (len, backing) pairs.
    ///
    /// # Safety
    /// Each len argument MUST be correct, otherwise this container is
    /// incorrectly defined and may crash from an incorrect range address.
    pub unsafe fn from_backing<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = (usize, E::Data)>,
    {
        let mut range_start = 0;
        let (keys, entries): (Vec<_>, Vec<_>) = iter.into_iter().unzip();
        let (key_starts, key_ends): (Vec<_>, Vec<_>) = keys
            .into_iter()
            .map(|k| {
                let prev_start = range_start;
                range_start += k;
                (k, prev_start)
            })
            .unzip();
        Self {
            key_starts: key_starts.into(),
            key_ends: key_ends.into(),
            entries: entries.into(),
        }
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
