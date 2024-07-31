#[cfg(feature = "async")]
pub mod async_impl;
pub mod sync_impl;

use std::{
    ops::{Index, IndexMut, Range},
    slice::{Iter, IterMut, SliceIndex},
};

use derive_getters::Getters;

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

pub trait Container:
    Default
    + IndexMut<usize, Output: Sized>
    + AsRef<[Self::Output]>
    + AsMut<[Self::Output]>
    + FromIterator<Self::Output>
    + IntoIterator<Item = Self::Output>
    + Extend<Self::Output>
{
    /// [`Vec::last`].
    fn c_push(&mut self, value: Self::Output);
    fn c_remove(&mut self, index: usize) -> Self::Output;
    fn c_append(&mut self, other: &mut Self);
}

impl<T> Container for Vec<T> {
    fn c_push(&mut self, value: Self::Output) {
        self.push(value)
    }
    fn c_remove(&mut self, index: usize) -> Self::Output {
        self.remove(index)
    }
    fn c_append(&mut self, other: &mut Self) {
        self.append(other)
    }
}
