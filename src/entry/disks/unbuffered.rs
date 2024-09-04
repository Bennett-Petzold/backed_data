/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use std::{
    fs::File,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
};

use serde::{Deserialize, Serialize};

use super::{Plainfile, ReadDisk, WriteDisk, WriteUnbuffered};

#[cfg(mmap_impl)]
use super::Mmap;

#[cfg(feature = "async")]
use super::{AsyncReadDisk, AsyncWriteDisk};

/// [`Plainfile`](`super::Plainfile`), but with no buffering at all.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Unbuffered {
    /// File location.
    path: PathBuf,
}

impl From<PathBuf> for Unbuffered {
    fn from(value: PathBuf) -> Self {
        Self { path: value }
    }
}

impl From<Unbuffered> for PathBuf {
    fn from(val: Unbuffered) -> Self {
        val.path
    }
}

impl AsRef<Path> for Unbuffered {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl Unbuffered {
    pub fn new(path: PathBuf) -> Self {
        path.into()
    }
}

impl ReadDisk for Unbuffered {
    type ReadDisk<'r> = File where Self: 'r;

    fn read_disk(&self) -> std::io::Result<Self::ReadDisk<'_>> {
        File::open(self.path.clone())
    }
}

impl WriteDisk for Unbuffered {
    type WriteDisk<'w> = File where Self: 'w;

    fn write_disk(&mut self) -> std::io::Result<Self::WriteDisk<'_>> {
        File::options()
            .write(true)
            .create(true)
            .truncate(true)
            .open(self.path.clone())
    }
}

#[cfg(runtime)]
impl AsyncReadDisk for Unbuffered {
    type ReadDisk<'r> = super::async_file::AsyncFile where Self: 'r;
    type ReadFut<'f> =
        Pin<Box<dyn Future<Output = std::io::Result<super::async_file::AsyncFile>> + Sync + Send>> where Self: 'f;

    fn async_read_disk(&self) -> Self::ReadFut<'_> {
        super::async_file::read_file(self.path.clone())
    }
}

#[cfg(runtime)]
impl AsyncWriteDisk for Unbuffered {
    type WriteDisk<'w> = super::async_file::AsyncFile where Self: 'w;
    type WriteFut<'f> =
        Pin<Box<dyn Future<Output = std::io::Result<super::async_file::AsyncFile>> + Sync + Send>> where Self: 'f;

    fn async_write_disk(&mut self) -> Self::WriteFut<'_> {
        super::async_file::write_file(self.path.clone())
    }
}

impl From<Unbuffered> for WriteUnbuffered {
    fn from(value: Unbuffered) -> Self {
        Self::new(value.path)
    }
}

impl From<Unbuffered> for Plainfile {
    fn from(value: Unbuffered) -> Self {
        Self::new(value.path)
    }
}

#[cfg(mmap_impl)]
impl TryFrom<Unbuffered> for Mmap {
    type Error = std::io::Error;
    fn try_from(value: Unbuffered) -> std::io::Result<Self> {
        Self::new(value.path)
    }
}
