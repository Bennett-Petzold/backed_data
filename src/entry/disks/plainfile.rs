/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use std::{
    fs::File,
    io::{BufReader, BufWriter},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::{ReadDisk, Unbuffered, WriteDisk, WriteUnbuffered};

#[cfg(mmap_impl)]
use super::Mmap;

#[cfg(feature = "async")]
use super::{AsyncReadDisk, AsyncWriteDisk};

/// A regular file entry.
///
/// This is used to open a [`File`] on demand, but drop the handle when unused.
/// Large collections of [`BackedEntry`](`super::super::BackedEntry`)s would otherwise risk overwhelming
/// OS limts on the number of open file descriptors.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Plainfile {
    /// File location.
    path: PathBuf,
}

impl From<PathBuf> for Plainfile {
    fn from(value: PathBuf) -> Self {
        Self { path: value }
    }
}

impl From<Plainfile> for PathBuf {
    fn from(val: Plainfile) -> Self {
        val.path
    }
}

impl AsRef<Path> for Plainfile {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl Plainfile {
    pub fn new(path: PathBuf) -> Self {
        path.into()
    }
}

impl ReadDisk for Plainfile {
    type ReadDisk = BufReader<File>;

    fn read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        Ok(BufReader::new(File::open(self.path.clone())?))
    }
}

impl WriteDisk for Plainfile {
    type WriteDisk = BufWriter<File>;

    fn write_disk(&mut self) -> std::io::Result<Self::WriteDisk> {
        Ok(BufWriter::new(
            File::options()
                .write(true)
                .create(true)
                .truncate(true)
                .open(self.path.clone())?,
        ))
    }
}

#[cfg(runtime)]
impl AsyncReadDisk for Plainfile {
    type ReadDisk = futures::io::BufReader<super::async_file::AsyncFile>;

    async fn async_read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        Ok(futures::io::BufReader::new(
            super::async_file::read_file(self.path.clone()).await?,
        ))
    }
}

#[cfg(runtime)]
impl AsyncWriteDisk for Plainfile {
    type WriteDisk = futures::io::BufWriter<super::async_file::AsyncFile>;

    async fn async_write_disk(&mut self) -> std::io::Result<Self::WriteDisk> {
        Ok(futures::io::BufWriter::new(
            super::async_file::write_file(self.path.clone()).await?,
        ))
    }
}

impl From<Plainfile> for WriteUnbuffered {
    fn from(value: Plainfile) -> Self {
        Self::new(value.path)
    }
}

impl From<Plainfile> for Unbuffered {
    fn from(value: Plainfile) -> Self {
        Self::new(value.path)
    }
}

#[cfg(mmap_impl)]
impl TryFrom<Plainfile> for Mmap {
    type Error = std::io::Error;
    fn try_from(value: Plainfile) -> std::io::Result<Self> {
        Self::new(value.path)
    }
}
