use crate::entry::{
    async_impl::{AsyncReadDisk, AsyncWriteDisk, BackedEntryArrAsync},
    BackedEntryUnload,
};
use async_bincode::tokio::{AsyncBincodeReader, AsyncBincodeWriter};
use derive_getters::Getters;
use futures::{stream, SinkExt, StreamExt};
use itertools::Itertools;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{ops::Range, pin::pin};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

use super::{internal_idx, multiple_internal_idx, BackedArrayError};

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
pub struct BackedArray<T, Disk: for<'df> Deserialize<'df>> {
    // keys and entries must always have the same length
    // keys must always be sorted min-max
    keys: Vec<Range<usize>>,
    #[serde(bound = "BackedEntryArrAsync<T, Disk>: Serialize + for<'df> Deserialize<'df>")]
    entries: Vec<BackedEntryArrAsync<T, Disk>>,
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
}

impl<T, Disk: for<'de> Deserialize<'de>> BackedArray<T, Disk> {
    /// Move all backing arrays out of memory
    pub fn clear_memory(&mut self) {
        self.entries.iter_mut().for_each(|entry| entry.unload());
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
impl<T: DeserializeOwned + Send + Sync, Disk: AsyncReadDisk + Send + Sync> BackedArray<T, Disk>
where
    Disk::ReadDisk: Send + Sync,
{
    /// Async version of [`BackedArray::get`].
    pub async fn get(&mut self, idx: usize) -> Result<&T, BackedArrayError> {
        let loc = internal_idx(&self.keys, idx).ok_or(BackedArrayError::OutsideEntryBounds(idx))?;
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
        let mut num_items = 0;

        let translated_idxes = multiple_internal_idx(&self.keys, idxs)
            .map(|x| {
                num_items += 1;
                x
            })
            .enumerate()
            .sorted_by(|(_, a_loc), (_, b_loc)| Ord::cmp(&a_loc.entry_idx, &b_loc.entry_idx))
            .group_by(|(_, loc)| loc.entry_idx);
        let translated_idxes: Vec<_> = translated_idxes.into_iter().collect();

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
impl<T: Serialize, Disk: AsyncWriteDisk> BackedArray<T, Disk> {
    /// Async version of [`Self::append`]
    pub async fn append(
        &mut self,
        values: &[T],
        backing_store: Disk,
    ) -> bincode::Result<&mut Self> {
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
    ) -> bincode::Result<&mut Self> {
        // End of a range is exclusive
        let start_idx = self.keys.last().map(|key_range| key_range.end).unwrap_or(0);
        self.keys.push(start_idx..(start_idx + values.len()));
        let mut entry = BackedEntryArrAsync::new(backing_store, None);
        entry.write(values).await?;
        self.entries.push(entry);
        Ok(self)
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

impl<T: Serialize, Disk: for<'de> Deserialize<'de> + Serialize> BackedArray<T, Disk> {
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

impl<T: DeserializeOwned, Disk: for<'de> Deserialize<'de> + Serialize> BackedArray<T, Disk> {
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

impl<T: Clone, Disk: for<'de> Deserialize<'de> + Clone> BackedArray<T, Disk> {
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

impl<T, Disk: for<'de> Deserialize<'de>> BackedArray<T, Disk> {
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

        let mut backed = BackedArray::new();
        backed.append(&INPUT, &mut back_vector_wrap).await.unwrap();
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

        let mut backed = BackedArray::new();
        backed.append(&INPUT_0, back_vector_wrap_0).await.unwrap();
        backed
            .append_memory(Box::new(INPUT_1), back_vector_wrap_1)
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
