use std::{ops::Range, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    array::sync_impl::BackedArray,
    entry::{
        disks::{Plainfile, ZstdDisk},
        BackedEntryArr,
    },
};

#[cfg(feature = "async")]
use crate::entry::BackedEntryAsync;

#[cfg(feature = "async")]
pub mod async_impl;
pub mod sync_impl;

const META_FILE: &str = "meta.dat";

/// [`BackedArray`] wrapper for automatically creating new entries in a
/// directory.
///
/// The array can be accessed directly when the wrapper methods are
/// insufficient. The wrapper methods are sufficient for most operations.
#[derive(Debug, Serialize, Deserialize)]
pub struct DirectoryBackedArray<K, E> {
    pub array: BackedArray<K, E>,
    directory_root: PathBuf,
}

pub type StdDirBackedArray<T, Coder> =
    DirectoryBackedArray<Vec<Range<usize>>, Vec<BackedEntryArr<T, Plainfile, Coder>>>;

#[cfg(feature = "async")]
pub type AsyncStdDirBackedArray<T, Coder> =
    DirectoryBackedArray<Vec<Range<usize>>, Vec<BackedEntryAsync<Box<[T]>, Plainfile, Coder>>>;

#[cfg(feature = "zstd")]
pub type ZstdDirBackedArray<'a, const ZSTD_LEVEL: u8, T, Coder> = DirectoryBackedArray<
    Vec<Range<usize>>,
    Vec<BackedEntryArr<T, ZstdDisk<'a, ZSTD_LEVEL, Plainfile>, Coder>>,
>;

#[cfg(feature = "async_zstd")]
pub type AsyncZstdDirBackedArray<'a, const ZSTD_LEVEL: u8, T, Coder> = DirectoryBackedArray<
    Vec<Range<usize>>,
    Vec<BackedEntryAsync<Box<[T]>, ZstdDisk<'a, ZSTD_LEVEL, Plainfile>, Coder>>,
>;
