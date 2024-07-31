#[cfg(feature = "async")]
pub mod async_impl;
pub mod container;
pub mod sync_impl;

use std::ops::Range;

use container::Container;
use derive_getters::Getters;
use serde::{Deserialize, Serialize};

use crate::entry::BackedEntryArr;

#[derive(Debug)]
pub enum BackedArrayError<T> {
    OutsideEntryBounds(usize),
    Coder(T),
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone)]
struct ArrayLoc {
    pub entry_idx: usize,
    pub inside_entry_idx: usize,
}

#[derive(Debug, Getters)]
pub struct BackedArrayEntry<'a, T> {
    range: &'a Range<usize>,
    entry: &'a T,
}

impl<'a, T> From<(&'a Range<usize>, &'a T)> for BackedArrayEntry<'a, T> {
    fn from(value: (&'a Range<usize>, &'a T)) -> Self {
        Self {
            range: value.0,
            entry: value.1,
        }
    }
}

fn internal_idx<'a, K: IntoIterator<Item = &'a Range<usize>>>(
    keys: K,
    idx: usize,
) -> Option<ArrayLoc> {
    keys.into_iter()
        .enumerate()
        .find(|(_, key_range)| key_range.contains(&idx))
        .map(|(entry_idx, key_range)| ArrayLoc {
            entry_idx,
            inside_entry_idx: (idx - key_range.start),
        })
}

/// Array stored as multiple arrays on disk.
///
/// Associates each access with the appropriate disk storage, loading it into
/// memory and returning the value. Subsequent accesses will use the in-memory
/// store. Use [`Self::clear_memory`] or [`Self::shrink_to_query`] to move the
/// cached sub-arrays back out of memory.
///
/// For repeated modifications, use [`Self::chunk_mut_iter`] to get perform
/// multiple modifications on a backing block before saving to disk.
/// Getting and overwriting the entries without these handles will write to
/// disk on every single change.
#[derive(Debug, Serialize, Deserialize, Getters)]
pub struct BackedArray<K, E> {
    // keys and entries must always have the same length
    // keys must always be sorted min-max
    pub(self) keys: K,
    pub(self) entries: E,
}

pub type VecBackedArray<T, Disk, Coder> =
    BackedArray<Vec<Range<usize>>, Vec<BackedEntryArr<T, Disk, Coder>>>;

impl<K: Clone, E: Clone> Clone for BackedArray<K, E> {
    fn clone(&self) -> Self {
        Self {
            keys: self.keys.clone(),
            entries: self.entries.clone(),
        }
    }
}

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
        self.keys.c_ref().as_ref().last().unwrap_or(&(0..0)).end
    }
}

impl<K, E: Container> BackedArray<K, E> {
    /// Number of underlying chunks.
    pub fn chunks_len(&self) -> usize {
        self.entries.c_ref().as_ref().len()
    }

    /// Access to the underlying chunks, without loading data.
    pub fn raw_chunks(&mut self) -> impl Iterator<Item: AsMut<E::Data>> + '_ {
        self.entries.mut_iter()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.c_ref().as_ref().is_empty()
    }
}
