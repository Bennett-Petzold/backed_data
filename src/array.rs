use std::ops::Range;

#[derive(Debug)]
pub enum BackedArrayError {
    OutsideEntryBounds(usize),
    Bincode(bincode::Error),
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone)]
struct ArrayLoc {
    pub entry_idx: usize,
    pub inside_entry_idx: usize,
}

fn internal_idx(keys: &[Range<usize>], idx: usize) -> Option<ArrayLoc> {
    keys.iter()
        .enumerate()
        .find(|(_, key_range)| key_range.contains(&idx))
        .map(|(entry_idx, key_range)| ArrayLoc {
            entry_idx,
            inside_entry_idx: (idx - key_range.start),
        })
}

/// Returns the entry location for multiple accesses.
///
/// Silently ignores invalid idx values, shortening the return iterator.
fn multiple_internal_idx<'a, I>(
    keys: &'a [Range<usize>],
    idxs: I,
) -> impl Iterator<Item = ArrayLoc> + 'a
where
    I: IntoIterator<Item = usize> + 'a,
{
    idxs.into_iter().flat_map(|idx| internal_idx(keys, idx))
}

/// [`Self::multiple_internal_idx`], but returns None for invalid idx
fn multiple_internal_idx_strict<'a, I>(
    keys: &'a [Range<usize>],
    idxs: I,
) -> impl Iterator<Item = Option<ArrayLoc>> + 'a
where
    I: IntoIterator<Item = usize> + 'a,
{
    idxs.into_iter().map(|idx| internal_idx(keys, idx))
}

pub mod sync_impl {
    use std::{
        io::{Read, Seek, Write},
        iter::Skip,
        ops::Range,
        ptr::{from_mut, from_ref},
    };

    use anyhow::anyhow;
    use bincode::{deserialize_from, serialize_into};
    use derive_getters::Getters;
    use serde::{de::DeserializeOwned, Deserialize, Serialize};

    use crate::entry::{BackedEntryArr, BackedEntryUnload};

    use super::{
        internal_idx, multiple_internal_idx, multiple_internal_idx_strict, BackedArrayError,
    };

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
    }

    impl<T, Disk> BackedArray<T, Disk> {
        /// Move all backing arrays out of memory
        pub fn clear_memory(&mut self) {
            self.entries.iter_mut().for_each(|entry| entry.unload());
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
            let loc =
                internal_idx(&self.keys, idx).ok_or(BackedArrayError::OutsideEntryBounds(idx))?;
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
        pub fn get_multiple_strict<'a, I>(
            &'a mut self,
            idxs: I,
        ) -> Vec<Result<&'a T, BackedArrayError>>
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
        pub fn append(&mut self, values: &[T], backing_store: Disk) -> bincode::Result<&Self> {
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
        ) -> bincode::Result<&Self> {
            // End of a range is exclusive
            let start_idx = self.keys.last().map(|key_range| key_range.end).unwrap_or(0);
            self.keys.push(start_idx..(start_idx + values.len()));
            let mut entry = BackedEntryArr::new(backing_store);
            entry.write(values)?;
            self.entries.push(entry);
            Ok(self)
        }
    }

    impl<T, Disk> BackedArray<T, Disk> {
        /// Removes an entry with the internal index, shifting ranges.
        ///
        /// The getter functions can be used to indentify the target index.
        pub fn remove(&mut self, entry_idx: usize) -> &Self {
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
        pub fn merge(&mut self, rhs: &Self) -> &Self {
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
        pub fn append_array(&mut self, mut rhs: Self) -> &Self {
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

    /// Iterator over all stored items.
    ///
    /// Keeps all loaded values in memory while alive.
    ///
    /// # Arguments
    /// * `pos`: Current position in chunks
    /// * `backed_entry`: The underlying array
    pub struct BackedEntryChunkIter<'a, T, Disk> {
        pos: usize,
        backed_entry: &'a mut BackedArray<T, Disk>,
    }

    impl<'a, T: DeserializeOwned, Disk: Read + Seek> Iterator for BackedEntryChunkIter<'a, T, Disk> {
        type Item = bincode::Result<&'a [T]>;

        fn next(&mut self) -> Option<Self::Item> {
            // No GAT support, so we make do with pointers
            // The lifetimes work out, but the compiler doesn't know that
            let val_ptr: Option<bincode::Result<*const [T]>> = self
                .backed_entry
                .entries
                .get_mut(self.pos)
                .map(move |x| x.load().map(|y| y as *const [T]));
            val_ptr.map(|y| y.map(|x| unsafe { &*x }))
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
    /// * `backed_entry`: The underlying array
    /// * `chunk_iter`: [`BackedEntryChunkIter`] used to load backing values
    pub struct BackedEntryItemIter<'a, T, Disk> {
        inner_pos: usize,
        backed_entry: Option<bincode::Result<&'a [T]>>,
        chunk_iter: BackedEntryChunkIter<'a, T, Disk>,
    }

    impl<'a, T: DeserializeOwned + 'a, Disk: Read + Seek> BackedEntryItemIter<'a, T, Disk> {
        fn new(inner_pos: usize, mut chunk_iter: BackedEntryChunkIter<'a, T, Disk>) -> Self {
            let backed_entry = chunk_iter.next();
            Self {
                inner_pos,
                backed_entry,
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
                .backed_entry
                .as_ref()
                .map(|entry| {
                    entry.as_ref().map(|loaded_entry| {
                        loaded_entry.get(self.inner_pos).map(Ok).or_else(|| {
                            self.inner_pos = 0;

                            // Mutating the backed entry,
                            // but the borrowed value is being discarded
                            unsafe {
                                (*self_ptr).backed_entry = self.chunk_iter.next();
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
        /// Return a [`BackedEntryItemIter`] that returns items in order.
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

        /// Default for [`BackedEntryItemIter`]
        pub fn item_iter_default(&mut self) -> BackedEntryItemIter<T, Disk> {
            self.item_iter(0)
        }

        /// Return a [`BackedEntryChunkIter`] that returns underlying chunks
        /// in order.
        ///
        /// # Arguments
        /// * `offset`: Starting chunk
        pub fn chunk_iter(&mut self, offset: usize) -> BackedEntryChunkIter<T, Disk> {
            BackedEntryChunkIter {
                pos: offset,
                backed_entry: self,
            }
        }

        /// Default for [`BackedEntryChunkIter`]
        pub fn chunk_iter_default(&mut self) -> BackedEntryChunkIter<T, Disk> {
            self.chunk_iter(0)
        }
    }

    #[cfg(test)]
    mod tests {
        use std::io::Cursor;

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
    }
}

#[cfg(feature = "async")]
pub mod async_impl {
    use crate::entry::{async_impl::BackedEntryArrAsync, BackedEntryUnload};
    use async_bincode::tokio::{AsyncBincodeReader, AsyncBincodeWriter};
    use derive_getters::Getters;
    use futures::{SinkExt, StreamExt};
    use itertools::Itertools;
    use serde::{de::DeserializeOwned, Deserialize, Serialize};
    use std::ops::Range;
    use tokio::{
        io::{AsyncRead, AsyncSeek, AsyncWrite, AsyncWriteExt},
        task::JoinSet,
    };

    use super::{internal_idx, multiple_internal_idx, BackedArrayError};

    #[derive(Debug, Clone, Serialize, Deserialize, Getters)]
    pub struct BackedArray<T, Disk> {
        // keys and entries must always have the same length
        // keys must always be sorted min-max
        keys: Vec<Range<usize>>,
        entries: Vec<BackedEntryArrAsync<T, Disk>>,
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
    }

    impl<T, Disk> BackedArray<T, Disk> {
        /// Move all backing arrays out of memory
        pub fn clear_memory(&mut self) {
            self.entries.iter_mut().for_each(|entry| entry.unload());
        }
    }

    /// Async Read implementations
    impl<
            T: DeserializeOwned + Send + Sync + 'static,
            Disk: AsyncRead + AsyncSeek + Unpin + Send + Sync + 'static,
        > BackedArray<T, Disk>
    {
        /// Async version of [`BackedArray::get`].
        pub async fn get(&mut self, idx: usize) -> Result<&T, BackedArrayError> {
            let loc =
                internal_idx(&self.keys, idx).ok_or(BackedArrayError::OutsideEntryBounds(idx))?;
            Ok(&self.entries[loc.entry_idx]
                .load()
                .await
                .map_err(BackedArrayError::Bincode)?[loc.inside_entry_idx])
        }

        /// Async version of [`BackedArray::get_multiple`].
        ///
        /// Preserves ordering and fails on any error within processing.
        pub async fn get_multiple<'a, I>(&'a mut self, idxs: I) -> bincode::Result<Vec<&'a T>>
        where
            I: IntoIterator<Item = usize> + 'a,
        {
            let mut load_futures: JoinSet<Result<Vec<(usize, &'static T)>, bincode::Error>> =
                JoinSet::new();
            let mut total_size = 0;
            let translated_idxes = multiple_internal_idx(&self.keys, idxs)
                .map(|x| {
                    total_size += 1;
                    x
                })
                .enumerate()
                .sorted_by(|(_, a_loc), (_, b_loc)| Ord::cmp(&a_loc.entry_idx, &b_loc.entry_idx))
                .group_by(|(_, loc)| loc.entry_idx);

            // Can't use the wrapper pattern with <T, Disk> generics inline.
            let entries_ptr = self.entries.as_mut_ptr() as usize;

            for (key, group) in translated_idxes.into_iter() {
                let group = group.into_iter().collect_vec();

                // Grab mutable handle to guaranteed unique key
                let entry =
                    unsafe { &mut *(entries_ptr as *mut BackedEntryArrAsync<T, Disk>).add(key) };
                load_futures.spawn(async move {
                    let arr = entry.load().await?;

                    Ok(group
                        .into_iter()
                        .map(|(ordering, loc)| Some((ordering, arr.get(loc.inside_entry_idx)?)))
                        .collect::<Option<Vec<_>>>()
                        .ok_or(bincode::ErrorKind::Custom("Missing an entry".to_string()))?)
                });
            }

            let mut results = Vec::with_capacity(total_size);

            while let Some(loaded) = load_futures.join_next().await {
                results.append(
                    &mut loaded.map_err(|e| bincode::ErrorKind::Custom(format!("{:?}", e)))??,
                )
            }

            Ok(results
                .into_iter()
                .sorted_by(|(a_idx, _), (b_idx, _)| Ord::cmp(a_idx, b_idx))
                .map(|(_, val)| val)
                .collect())
        }
    }

    #[cfg(feature = "async")]
    impl<T: Serialize, Disk: AsyncWrite + Unpin> BackedArray<T, Disk> {
        /// Async version of [`Self::append`]
        pub async fn append(
            &mut self,
            values: &[T],
            backing_store: Disk,
        ) -> bincode::Result<&Self> {
            // End of a range is exclusive
            let start_idx = self.keys.last().map(|key_range| key_range.end).unwrap_or(0);
            self.keys.push(start_idx..(start_idx + values.len()));
            let mut entry = BackedEntryArrAsync::new(backing_store, None);
            entry.write_unload(values).await?;
            self.entries.push(entry);
            Ok(self)
        }

        /// Async version of [`Self::append_memory`]
        pub async fn append_memory(
            &mut self,
            values: Box<[T]>,
            backing_store: Disk,
        ) -> bincode::Result<&Self> {
            // End of a range is exclusive
            let start_idx = self.keys.last().map(|key_range| key_range.end).unwrap_or(0);
            self.keys.push(start_idx..(start_idx + values.len()));
            let mut entry = BackedEntryArrAsync::new(backing_store, None);
            entry.write(values).await?;
            self.entries.push(entry);
            Ok(self)
        }
    }

    impl<T, Disk> BackedArray<T, Disk> {
        /// Removes an entry with the internal index, shifting ranges.
        ///
        /// The getter functions can be used to indentify the target index.
        pub fn remove(&mut self, entry_idx: usize) -> &Self {
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
        /// Async version of [`Self::save_to_disk`]
        pub async fn save_to_disk<W: AsyncWrite + Unpin>(
            &mut self,
            writer: &mut W,
        ) -> bincode::Result<()> {
            self.clear_memory();
            let mut bincode_writer = AsyncBincodeWriter::from(writer).for_async();
            bincode_writer.send(&self).await?;
            bincode_writer.get_mut().flush().await?;
            Ok(())
        }
    }

    impl<T: DeserializeOwned, Disk: DeserializeOwned> BackedArray<T, Disk> {
        /// Async version of [`Self::load`]
        pub async fn load<R: AsyncRead + Unpin>(writer: &mut R) -> bincode::Result<Self> {
            AsyncBincodeReader::from(writer)
                .next()
                .await
                .ok_or(bincode::ErrorKind::Custom(
                    "AsyncBincodeReader stream empty".to_string(),
                ))?
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
        pub fn merge(&mut self, rhs: &Self) -> &Self {
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
        pub fn append_array(&mut self, mut rhs: Self) -> &Self {
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

    #[cfg(test)]
    mod tests {
        use std::io::Cursor;

        use super::*;

        #[tokio::test]
        async fn write() {
            let mut back_vector = vec![0];

            const INPUT: [u8; 3] = [2, 3, 5];

            let mut backed = BackedArray::new();
            backed.append(&INPUT, &mut back_vector).await.unwrap();
            assert_eq!(back_vector[back_vector.len() - 3..], [2, 3, 5]);
            assert_eq!(back_vector[back_vector.len() - 4], 3);
        }

        #[tokio::test]
        async fn multiple_retrieve() {
            let back_vector_1 = Cursor::new(Vec::with_capacity(3));
            let back_vector_2 = Cursor::new(Vec::with_capacity(3));

            const INPUT_1: [u8; 3] = [0, 1, 1];
            const INPUT_2: [u8; 3] = [2, 3, 5];

            let mut backed = BackedArray::new();
            backed.append(&INPUT_1, back_vector_1).await.unwrap();
            backed
                .append_memory(Box::new(INPUT_2), back_vector_2)
                .await
                .unwrap();

            assert_eq!(backed.get(0).await.unwrap(), &0);
            assert_eq!(backed.get(4).await.unwrap(), &3);
            assert_eq!(
                backed.get_multiple([0, 2, 4, 5]).await.unwrap(),
                [&0, &1, &3, &5]
            );
            assert_eq!(
                backed.get_multiple([5, 2, 0, 5]).await.unwrap(),
                [&5, &1, &0, &5]
            );
            assert_eq!(
                backed.get_multiple([5, 2, 0, 5]).await.unwrap(),
                [&5, &1, &0, &5]
            );
        }
    }
}
