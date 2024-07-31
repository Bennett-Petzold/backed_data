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
    //
    // keys are split into _starts and _ends for better data locality
    // when searching (index searches are based only on comparing starts).
    pub(self) key_starts: K,
    pub(self) key_ends: K,
    pub(self) entries: E,
}

pub type VecBackedArray<T, Disk, Coder> =
    BackedArray<Vec<usize>, Vec<BackedEntryArr<T, Disk, Coder>>>;

impl<K: Clone, E: Clone> Clone for BackedArray<K, E> {
    fn clone(&self) -> Self {
        Self {
            key_starts: self.key_starts.clone(),
            key_ends: self.key_ends.clone(),
            entries: self.entries.clone(),
        }
    }
}

impl<K: Default, E: Default> Default for BackedArray<K, E> {
    fn default() -> Self {
        Self {
            key_starts: K::default(),
            key_ends: K::default(),
            entries: E::default(),
        }
    }
}

impl<K: Default, E: Default> BackedArray<K, E> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<K: Container<Data = usize>, E> BackedArray<K, E> {
    /// Total size of stored data.
    pub fn len(&self) -> usize {
        *self.key_ends.c_ref().as_ref().last().unwrap_or(&0)
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

/// Finds idx in a [`BackedArray`].
///
/// Starts and ends must be the same size, and starts[i] must be <= ends[i].
///
/// # Parameters
/// * `starts`: Starts of a sub-container ranges.
/// * `ends`: Ends of a sub-container ranges.
/// * `idx`: Location to get an [`ArrayLoc`] translation for.
fn internal_idx<K: AsRef<[usize]>>(starts: K, ends: K, idx: usize) -> Option<ArrayLoc> {
    let starts = starts.as_ref();
    let ends = ends.as_ref();

    let entry_idx = match ends.binary_search(&idx) {
        // Matched with a range end, but not the final one. The final range
        // end is out of bounds.
        Ok(x) if x != ends.len() - 1 => x + 1,
        // Can be inserted at some position beyond the final end range.
        Err(x) if x != ends.len() => x,
        // Out of bounds
        _ => {
            return None;
        }
    };

    Some(ArrayLoc {
        entry_idx,
        inside_entry_idx: (idx - starts[entry_idx]),
    })
}
