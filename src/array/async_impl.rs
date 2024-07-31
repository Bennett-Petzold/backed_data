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
            results
                .append(&mut loaded.map_err(|e| bincode::ErrorKind::Custom(format!("{:?}", e)))??)
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
