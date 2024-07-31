#[cfg(feature = "async")]
pub mod async_impl;
pub mod sync_impl;

use std::ops::Range;

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

pub trait BackingContainer<T>: Default {
    // add code here
}

impl<T> BackingContainer<T> for Vec<T> {
    // add code here
}
