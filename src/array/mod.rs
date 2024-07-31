#[cfg(feature = "async")]
pub mod async_impl;
pub mod sync_impl;

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
