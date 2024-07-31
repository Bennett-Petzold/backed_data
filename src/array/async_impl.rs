use crate::entry::{
    async_impl::{
        AsyncReadDisk, AsyncWriteDisk, BackedEntryArrAsync, BackedEntryAsync, BackedEntryMutAsync,
    },
    BackedEntryUnload, DiskOverwritable,
};
use async_bincode::tokio::{AsyncBincodeReader, AsyncBincodeWriter};
use derive_getters::Getters;
use futures::{stream, SinkExt, Stream, StreamExt};
use itertools::Itertools;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{ops::Range, pin::pin};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

use super::{
    internal_idx, multiple_internal_idx, sync_impl::VecBackedArray as SyncVecBackedArray,
    BackedArrayEntry, BackedArrayError,
};

/// Asynchronous version of [`BackedArray`].
#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
pub struct BackedArray<K, E> {
    // keys and entries must always have the same length
    // keys must always be sorted min-max
    keys: K,
    //#[serde(bound = "BackedEntryArrAsync<T, Disk>: Serialize + for<'df> Deserialize<'df>")]
    entries: E,
}

pub type VecBackedArray<T, Disk> =
    BackedArray<Vec<Range<usize>>, Vec<BackedEntryArrAsync<T, Disk>>>;

impl<T, Disk: for<'de> Deserialize<'de>> Default for VecBackedArray<T, Disk> {
    fn default() -> Self {
        Self {
            keys: vec![],
            entries: vec![],
        }
    }
}

impl<T, Disk: for<'de> Deserialize<'de>> VecBackedArray<T, Disk> {
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
    ) -> impl Iterator<Item = BackedArrayEntry<'_, BackedEntryAsync<Box<[T]>, Disk>>> {
        self.keys
            .iter()
            .zip(self.entries.iter())
            .map(BackedArrayEntry::from)
    }

    /// Returns all currently loaded backing entries and the range they cover.
    pub fn loaded_array_entries(
        &self,
    ) -> impl Iterator<Item = BackedArrayEntry<'_, BackedEntryAsync<Box<[T]>, Disk>>> {
        self.backed_array_entries().filter(
            |backed: &BackedArrayEntry<'_, BackedEntryAsync<Box<[T]>, Disk>>| {
                backed.entry.is_loaded()
            },
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

#[derive(Debug)]
struct PointerWrapper<T>(T);

unsafe impl<T> Send for PointerWrapper<T> {}
unsafe impl<T> Sync for PointerWrapper<T> {}

impl<T> From<T> for PointerWrapper<T> {
    fn from(value: T) -> Self {
        Self(value)
    }
}

/// Async Read implementations
impl<T: DeserializeOwned + Send + Sync, Disk: AsyncReadDisk + Send + Sync> VecBackedArray<T, Disk>
where
    Disk::ReadDisk: Send + Sync,
{
    /// Async version of [`VecBackedArray::get`].
    pub async fn get(&mut self, idx: usize) -> Result<&T, BackedArrayError> {
        let loc = internal_idx(&self.keys, idx).ok_or(BackedArrayError::OutsideEntryBounds(idx))?;
        Ok(&self.entries[loc.entry_idx]
            .load()
            .await
            .map_err(BackedArrayError::Bincode)?[loc.inside_entry_idx])
    }

    /// Async version of [`VecBackedArray::get_multiple`].
    ///
    /// Preserves ordering and fails on any error within processing.
    pub async fn get_multiple<'a, I>(&'a mut self, idxs: I) -> bincode::Result<Vec<&'a T>>
    where
        I: IntoIterator<Item = usize> + 'a,
    {
        let mut num_items = 0;

        let translated_idxes = multiple_internal_idx(&self.keys, idxs)
            .inspect(|_| {
                num_items += 1;
            })
            .enumerate()
            .sorted_by(|(_, a_loc), (_, b_loc)| Ord::cmp(&a_loc.entry_idx, &b_loc.entry_idx))
            .group_by(|(_, loc)| loc.entry_idx);
        let translated_idxes: Vec<_> = translated_idxes.into_iter().collect();

        if num_items == 0 {
            return Ok(vec![]);
        }

        let num_groups = translated_idxes.len();
        let mut results = Vec::with_capacity(num_items);

        let arr_ptr: *mut _ = &mut self.entries;

        let values = pin!(stream::iter(translated_idxes.into_iter()).map(
            |(key, group)| async move {
                let group = group.into_iter().collect_vec();
                let arr = unsafe { (&mut *arr_ptr)[key].load().await? };
                Ok::<Vec<_>, bincode::Error>(
                    group
                        .into_iter()
                        .map(|(order, loc)| arr.get(loc.inside_entry_idx).map(|val| (order, val)))
                        .collect::<Option<Vec<_>>>()
                        .ok_or(bincode::ErrorKind::Custom("Missing an entry".to_string()))?,
                )
            }
        ));
        let mut values = values.buffer_unordered(num_groups);

        while let Some(value) = values.next().await {
            results.append(&mut value?);
        }

        Ok(results
            .into_iter()
            .sorted_by(|(a_order, _), (b_order, _)| Ord::cmp(a_order, b_order))
            .map(|(_, val)| val)
            .collect())
    }
}

#[cfg(feature = "async")]
impl<T: Serialize, Disk: AsyncWriteDisk> VecBackedArray<T, Disk> {
    /// Async version of [`super::sync_impl::VecBackedArray::append`].
    pub async fn append<U: Into<Box<[T]>>>(
        &mut self,
        values: U,
        backing_store: Disk,
    ) -> bincode::Result<&mut Self> {
        let values = values.into();

        // End of a range is exclusive
        let start_idx = self.keys.last().map(|key_range| key_range.end).unwrap_or(0);
        self.keys.push(start_idx..(start_idx + values.len()));
        let mut entry = BackedEntryArrAsync::new(backing_store, None);
        entry.write_unload(values).await?;
        self.entries.push(entry);
        Ok(self)
    }

    /// Async version of [`super::sync_impl::append_memory`].
    pub async fn append_memory<U: Into<Box<[T]>>>(
        &mut self,
        values: U,
        backing_store: Disk,
    ) -> bincode::Result<&mut Self> {
        let values = values.into();

        // End of a range is exclusive
        let start_idx = self.keys.last().map(|key_range| key_range.end).unwrap_or(0);
        self.keys.push(start_idx..(start_idx + values.len()));
        let mut entry = BackedEntryArrAsync::new(backing_store, None);
        entry.write(values).await?;
        self.entries.push(entry);
        Ok(self)
    }
}

impl<T, Disk: for<'de> Deserialize<'de>> VecBackedArray<T, Disk> {
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

impl<T: Serialize, Disk: for<'de> Deserialize<'de> + Serialize> VecBackedArray<T, Disk> {
    /// Async version of [`super::sync_impl::save_to_disk`]
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

impl<T: DeserializeOwned, Disk: for<'de> Deserialize<'de> + Serialize> VecBackedArray<T, Disk> {
    /// Async version of [`super::sync_impl::load`]
    pub async fn load<R: AsyncRead + Unpin>(writer: &mut R) -> bincode::Result<Self> {
        AsyncBincodeReader::from(writer)
            .next()
            .await
            .ok_or(bincode::ErrorKind::Custom(
                "AsyncBincodeReader stream empty".to_string(),
            ))?
    }
}

impl<T: Clone, Disk: for<'de> Deserialize<'de> + Clone> VecBackedArray<T, Disk> {
    /// Combine `self` and `rhs` into a new [`super::sync_impl`]
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

impl<T, Disk: for<'de> Deserialize<'de>> VecBackedArray<T, Disk> {
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

impl<T: Serialize + Clone, Disk: AsyncWriteDisk + Clone> VecBackedArray<T, Disk> {
    /// Moves all entries of `rhs` into `self`
    pub async fn append_sync_array(
        &mut self,
        rhs: SyncVecBackedArray<T, Disk>,
    ) -> bincode::Result<&mut Self> {
        let offset = self.keys.last().unwrap_or(&(0..0)).end;
        let mut rhs_keys = rhs.keys().clone();
        rhs_keys.iter_mut().for_each(|range| {
            range.start += offset;
            range.end += offset;
        });

        let mut rhs_entries = Vec::with_capacity(rhs.entries().len());
        let mut rhs_stream = pin!(stream::iter(rhs.entries().iter().clone()).then(|entry| {
            let entry = (*entry).clone();
            async move { BackedEntryAsync::from_sync_entry(entry).await }
        }));
        while let Some(val) = rhs_stream.next().await {
            rhs_entries.push(val?);
        }

        self.keys.append(&mut rhs_keys);
        self.entries.extend(rhs_entries);
        Ok(self)
    }
}

impl<T: Serialize, Disk: AsyncWriteDisk> VecBackedArray<T, Disk> {
    /// Converts into a sync array
    pub async fn to_sync_array(self) -> bincode::Result<SyncVecBackedArray<T, Disk>> {
        let mut entry_vec = Vec::with_capacity(self.entries.len());
        for entry in self.entries {
            entry_vec.push(entry.into_sync_entry().await?);
        }

        Ok(SyncVecBackedArray::from_pairs(
            self.keys.iter().cloned().zip(entry_vec),
        ))
    }
}

// Iterator returns
impl<T: DeserializeOwned, Disk: AsyncReadDisk> VecBackedArray<T, Disk> {
    /// Outputs items in order.
    ///
    /// Excluding chunks before the initial offset, all chunks will load in
    /// and remain loaded during iteration.
    ///
    /// # Arguments
    /// * `offset`: Starting position
    pub fn item_stream(&mut self, offset: usize) -> impl Stream<Item = bincode::Result<&T>> {
        // Defaults to force a None return from iterator
        let mut outer_pos = usize::MAX;
        let mut inner_pos = usize::MAX;

        if let Some(loc) = internal_idx(&self.keys, offset) {
            outer_pos = loc.entry_idx;
            inner_pos = loc.inside_entry_idx;
        };

        self.chunk_stream(outer_pos)
            .flat_map(|chunk| {
                let chunk: Box<dyn Stream<Item = bincode::Result<&T>> + Unpin> = match chunk {
                    Ok(chunk_ok) => Box::new(stream::iter(chunk_ok.iter().map(Ok))),
                    Err(chunk_err) => Box::new(stream::iter([Err(chunk_err)])),
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
    pub fn chunk_stream(&mut self, offset: usize) -> impl Stream<Item = bincode::Result<&[T]>> {
        stream::iter(self.entries.iter_mut().skip(offset))
            .then(|entry| async { entry.load().await.map(|entry| entry.as_ref()) })
    }
}

impl<T: Serialize + DeserializeOwned, Disk: AsyncReadDisk + AsyncWriteDisk + DiskOverwritable>
    VecBackedArray<T, Disk>
{
    /// Provides mutable handles to underlying chunks, using [`BackedEntryMutAsync`].
    ///
    /// See [`super::sync_impl::chunk_stream`] for the immutable iterator.
    pub fn chunk_mod_stream(
        &mut self,
        offset: usize,
    ) -> impl Stream<Item = bincode::Result<BackedEntryMutAsync<Box<[T]>, Disk>>> {
        stream::iter(self.entries.iter_mut().skip(offset))
            .then(|entry| async { entry.mut_handle().await })
    }
}

impl<T: DeserializeOwned + Clone, Disk: AsyncReadDisk> VecBackedArray<T, Disk> {
    /// Returns clones of chunk data.
    ///
    /// See [`super::sync_impl::chunk_stream`] for a version that produces references.
    pub fn dup_stream(
        &mut self,
        offset: usize,
    ) -> impl Stream<Item = bincode::Result<Vec<T>>> + '_ {
        stream::iter(self.entries.iter_mut().skip(offset)).then(|entry| async {
            let val = entry.load().await?.to_vec();
            entry.unload();
            Ok(val)
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::test_utils::CursorVec;

    use super::*;

    #[tokio::test]
    async fn write() {
        let mut back_vector = Cursor::new(Vec::new());
        let back_vector_ptr: *mut _ = &mut back_vector;

        let mut back_vector_wrap = CursorVec {
            inner: unsafe { &mut *back_vector_ptr },
        };

        const INPUT: [u8; 3] = [2, 3, 5];

        let mut backed = VecBackedArray::new();
        backed.append(INPUT, &mut back_vector_wrap).await.unwrap();
        assert_eq!(
            back_vector.get_ref()[back_vector.get_ref().len() - 3..],
            [2, 3, 5]
        );
        assert_eq!(back_vector.get_ref()[back_vector.get_ref().len() - 4], 3);
    }

    #[tokio::test]
    async fn multiple_retrieve() {
        let mut back_vector_0 = Cursor::new(Vec::with_capacity(3));
        let mut back_vector_1 = Cursor::new(Vec::with_capacity(3));

        let back_vector_wrap_0 = CursorVec {
            inner: &mut back_vector_0,
        };
        let back_vector_wrap_1 = CursorVec {
            inner: &mut back_vector_1,
        };

        const INPUT_0: [u8; 3] = [0, 1, 1];
        const INPUT_1: [u8; 3] = [2, 3, 5];

        let mut backed = VecBackedArray::new();
        backed.append(INPUT_0, back_vector_wrap_0).await.unwrap();
        backed
            .append_memory(INPUT_1, back_vector_wrap_1)
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

    #[tokio::test]
    async fn reconvert() {
        let mut back_vector = Cursor::new(Vec::new());
        let back_vector_ptr: *mut _ = &mut back_vector;

        let mut back_vector_wrap = CursorVec {
            inner: unsafe { &mut *back_vector_ptr },
        };

        const INPUT: [u8; 3] = [2, 3, 5];

        let mut backed = VecBackedArray::new();
        backed.append(INPUT, &mut back_vector_wrap).await.unwrap();

        let back_vec_prev = back_vector.get_ref().clone();

        backed
            .chunk_mod_stream(0)
            .for_each(|chunk| async {
                chunk.unwrap().conv_to_sync().await.unwrap();
            })
            .await;

        assert_eq!(
            back_vector.get_ref()[back_vector.get_ref().len() - INPUT.len()..],
            [2, 3, 5]
        );
        println!("Sync version: {:?}", back_vector.get_ref());

        assert_ne!(back_vector.get_ref(), &back_vec_prev);

        backed
            .chunk_mod_stream(0)
            .for_each(|chunk| async {
                chunk.unwrap().conv_to_async().await.unwrap();
            })
            .await;

        assert_eq!(backed.get(1).await.unwrap(), &3);

        backed.clear_memory();

        assert_eq!(back_vector.get_ref(), &back_vec_prev);

        println!("Async version: {:?}", back_vector.get_ref());

        assert_eq!(backed.get(1).await.unwrap(), &3);
    }

    #[tokio::test]
    async fn cross_write() {
        let mut back_vector = Cursor::new(Vec::new());
        let back_vector_ptr: *mut _ = &mut back_vector;

        let mut back_vector_wrap = CursorVec {
            inner: unsafe { &mut *back_vector_ptr },
        };

        const INPUT: [u8; 3] = [2, 3, 5];

        let mut backed = VecBackedArray::new();
        backed.append(INPUT, &mut back_vector_wrap).await.unwrap();

        let back_vec_prev = back_vector.get_ref().clone();

        backed
            .chunk_mod_stream(0)
            .for_each(|chunk| async {
                chunk.unwrap().conv_to_sync().await.unwrap();
            })
            .await;

        assert_eq!(
            back_vector.get_ref()[back_vector.get_ref().len() - INPUT.len()..],
            [2, 3, 5]
        );
        println!("Sync version: {:?}", back_vector.get_ref());

        assert_ne!(back_vector.get_ref(), &back_vec_prev);

        let mut backed = backed.to_sync_array().await.unwrap();
        backed.clear_memory();

        assert_eq!(
            backed.chunk_iter(0).collect::<Result<Vec<_>, _>>().unwrap(),
            [&[2, 3, 5]]
        );

        assert_eq!(
            backed.item_iter(0).collect::<Result<Vec<_>, _>>().unwrap(),
            [&2, &3, &5]
        );
    }
}
