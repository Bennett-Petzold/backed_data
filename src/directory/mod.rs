use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{
    array::BackedArray,
    entry::{disks::Plainfile, BackedEntryArr},
};

#[cfg(all(runtime, any(feature = "zstd", feature = "async_zstd")))]
use crate::entry::disks::{WriteUnbuffered, ZstdDisk};

#[cfg(feature = "async")]
use crate::entry::BackedEntryAsync;

#[cfg(runtime)]
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
    DirectoryBackedArray<Vec<usize>, Vec<BackedEntryArr<T, Plainfile, Coder>>>;

#[cfg(runtime)]
pub type AsyncStdDirBackedArray<T, Coder> =
    DirectoryBackedArray<Vec<usize>, Vec<BackedEntryAsync<Box<[T]>, Plainfile, Coder>>>;

#[cfg(all(runtime, feature = "zstd"))]
pub type ZstdDirBackedArray<'a, const ZSTD_LEVEL: u8, T, Coder> = DirectoryBackedArray<
    Vec<usize>,
    Vec<BackedEntryArr<T, ZstdDisk<'a, ZSTD_LEVEL, WriteUnbuffered>, Coder>>,
>;

#[cfg(all(runtime, feature = "async_zstd"))]
pub type AsyncZstdDirBackedArray<'a, const ZSTD_LEVEL: u8, T, Coder> = DirectoryBackedArray<
    Vec<usize>,
    Vec<BackedEntryAsync<Box<[T]>, ZstdDisk<'a, ZSTD_LEVEL, WriteUnbuffered>, Coder>>,
>;
