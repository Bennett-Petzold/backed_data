use std::{ops::Range, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    array::sync_impl::BackedArray,
    entry::{disks::Plainfile, BackedEntryArr},
};

#[cfg(feature = "async")]
use crate::entry::BackedEntryAsync;

#[cfg(feature = "async")]
pub mod async_impl;
pub mod sync_impl;

/// [`BackedArray`] that uses a directory of plain files
#[derive(Debug, Serialize, Deserialize)]
pub struct DirectoryBackedArray<K, E> {
    array: BackedArray<K, E>,
    directory_root: PathBuf,
}

pub type StdDirBackedArray<T, Coder> =
    DirectoryBackedArray<Vec<Range<usize>>, Vec<BackedEntryArr<T, Plainfile, Coder>>>;

#[cfg(feature = "async")]
pub type AsyncStdDirBackedArray<T, Coder> =
    DirectoryBackedArray<Vec<Range<usize>>, Vec<BackedEntryAsync<Box<[T]>, Plainfile, Coder>>>;
