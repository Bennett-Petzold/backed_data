/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use std::{
    fs::File,
    future::Future,
    io::{BufReader, BufWriter},
    path::{Path, PathBuf},
    pin::Pin,
    task::{Context, Poll},
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
#[derive(Debug)]
pub struct BufferedReadFut {
    file: Box<dyn Future<Output = std::io::Result<super::async_file::AsyncFile>> + Send + Sync>,
}

#[cfg(runtime)]
impl BufferedReadFut {
    pub fn new(
        file: Box<dyn Future<Output = std::io::Result<super::async_file::AsyncFile>> + Send + Sync>,
    ) -> Self {
        Self { file }
    }
}

#[cfg(runtime)]
impl Future for BufferedReadFut {
    type Output = std::io::Result<futures::io::BufReader<super::async_file::AsyncFile>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.file.poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(x) => match x {
                Ok(f) => Poll::Ready(Ok(futures::io::BufReader::new(f))),
                Err(e) => Poll::Ready(Err(e)),
            },
        }
    }
}

#[cfg(runtime)]
impl AsyncReadDisk for Plainfile {
    type ReadDisk = futures::io::BufReader<super::async_file::AsyncFile>;
    type ReadFut = BufferedReadFut;

    fn async_read_disk(&self) -> Self::ReadFut {
        BufferedReadFut::new(super::async_file::read_file(self.path.clone()))
    }
}

#[cfg(runtime)]
#[derive(Debug)]
pub struct BufferedWriteFut {
    file: Box<dyn Future<Output = std::io::Result<super::async_file::AsyncFile>>>,
}

#[cfg(runtime)]
impl BufferedWriteFut {
    pub fn new(
        file: Box<dyn Future<Output = std::io::Result<super::async_file::AsyncFile>>>,
    ) -> Self {
        Self { file }
    }
}

#[cfg(runtime)]
impl Future for BufferedWriteFut {
    type Output = std::io::Result<futures::io::BufWriter<super::async_file::AsyncFile>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.file.poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(x) => match x {
                Ok(f) => Poll::Ready(Ok(futures::io::BufWriter::new(f))),
                Err(e) => Poll::Ready(Err(e)),
            },
        }
    }
}

#[cfg(runtime)]
impl AsyncWriteDisk for Plainfile {
    type WriteDisk = futures::io::BufWriter<super::async_file::AsyncFile>;
    type WriteFut = BufferedWriteFut;

    fn async_write_disk(&mut self) -> BufferedWriteFut {
        BufferedWriteFut::new(super::async_file::write_file(self.path.clone()))
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
