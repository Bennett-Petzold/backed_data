#[cfg(feature = "async")]
pub mod async_impl;
pub mod container;
pub mod sync_impl;

use std::ops::Range;

use derive_getters::Getters;

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

/// Returns the entry location for multiple accesses, or None if idx invalid.
fn multiple_internal_idx_strict<'a, I, K: AsRef<[Range<usize>]> + 'a>(
    keys: K,
    idxs: I,
) -> impl Iterator<Item = Option<ArrayLoc>> + 'a
where
    I: IntoIterator<Item = usize> + 'a,
{
    idxs.into_iter()
        .map(move |idx| internal_idx(keys.as_ref(), idx))
}
