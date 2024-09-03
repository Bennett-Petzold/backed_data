/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/*!
Defines the underlying storage for [`BackedEntry`][`super::BackedEntry`].

The types [`Plainfile`], [`WriteUnbuffered`], [`Unbuffered`], and [`Mmap`] are
completely interchangeable. A disk written to by one can be read by another.
*/

use std::io::{Read, Write};

#[cfg(feature = "async")]
use {
    futures::io::{AsyncRead, AsyncWrite},
    std::future::Future,
};

/// Produces storage that can be read from synchronously.
pub trait ReadDisk {
    type ReadDisk: Read;
    fn read_disk(&self) -> std::io::Result<Self::ReadDisk>;
}

/// Produces storage that can be written to synchronously.
pub trait WriteDisk {
    type WriteDisk: Write;
    fn write_disk(&mut self) -> std::io::Result<Self::WriteDisk>;
}

#[cfg(feature = "async")]
/// Produces storage that can be read from asynchronously.
pub trait AsyncReadDisk {
    type ReadDisk<'r>: AsyncRead
    where
        Self: 'r;
    type ReadFut<'f>: Future<Output = std::io::Result<Self::ReadDisk<'f>>>
    where
        Self: 'f;

    fn async_read_disk(&self) -> Self::ReadFut<'_>;
}

#[cfg(feature = "async")]
/// Produces storage that can be written to asynchronously.
pub trait AsyncWriteDisk {
    type WriteDisk: AsyncWrite;
    type WriteFut: Future<Output = std::io::Result<Self::WriteDisk>>;
    fn async_write_disk(&mut self) -> Self::WriteFut;
}

impl<T: ReadDisk> ReadDisk for &T {
    type ReadDisk = T::ReadDisk;
    fn read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        T::read_disk(self)
    }
}

impl<T: WriteDisk> WriteDisk for &mut T {
    type WriteDisk = T::WriteDisk;
    fn write_disk(&mut self) -> std::io::Result<Self::WriteDisk> {
        T::write_disk(self)
    }
}

#[cfg(feature = "async")]
impl<T: AsyncReadDisk + Send> AsyncReadDisk for &T {
    type ReadDisk<'r> = T::ReadDisk<'r> where Self: 'r;
    type ReadFut<'f> = T::ReadFut<'f> where Self: 'f;
    fn async_read_disk(&self) -> Self::ReadFut<'_> {
        T::async_read_disk(self)
    }
}

#[cfg(feature = "async")]
impl<T: AsyncWriteDisk + Send> AsyncWriteDisk for &mut T {
    type WriteDisk = T::WriteDisk;
    type WriteFut = T::WriteFut;
    fn async_write_disk(&mut self) -> Self::WriteFut {
        T::async_write_disk(self)
    }
}

mod plainfile;
pub use plainfile::*;

mod write_unbuffered;
pub use write_unbuffered::*;

mod unbuffered;
pub use unbuffered::*;

pub mod custom;

#[cfg(any(feature = "zstd", feature = "async_zstd"))]
pub mod zstd;
#[cfg(any(feature = "zstd", feature = "async_zstd"))]
pub use zstd::ZstdDisk;

#[cfg(feature = "encrypted")]
pub mod encrypted;
#[cfg(feature = "encrypted")]
pub use encrypted::{Encrypted, SecretVecWrapper};

#[cfg(feature = "network")]
mod network;
#[cfg(feature = "network")]
pub use network::{default_client, BoxedNetwork, Network};

#[cfg(mmap_impl)]
pub mod mmap;
#[cfg(mmap_impl)]
pub use mmap::Mmap;

#[cfg(runtime)]
mod async_file;
#[cfg(runtime)]
pub use async_file::{AsyncError, AsyncFile};
