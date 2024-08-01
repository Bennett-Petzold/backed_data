use std::io::{Read, Write};

#[cfg(feature = "async")]
use {
    futures::io::{AsyncRead, AsyncWrite},
    std::future::Future,
};

pub trait ReadDisk {
    type ReadDisk: Read;
    fn read_disk(&self) -> std::io::Result<Self::ReadDisk>;
}

pub trait WriteDisk {
    type WriteDisk: Write;
    fn write_disk(&self) -> std::io::Result<Self::WriteDisk>;
}

#[cfg(feature = "async")]
pub trait AsyncReadDisk: Unpin {
    type ReadDisk: AsyncRead + Unpin;
    fn async_read_disk(
        &self,
    ) -> impl Future<Output = std::io::Result<Self::ReadDisk>> + Send + Sync;
}

#[cfg(feature = "async")]
pub trait AsyncWriteDisk: Unpin {
    type WriteDisk: AsyncWrite + Unpin;
    fn async_write_disk(
        &self,
    ) -> impl Future<Output = std::io::Result<Self::WriteDisk>> + Send + Sync;
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
pub use network::{default_client, Network};

#[cfg(mmap_impl)]
mod mmap;
#[cfg(mmap_impl)]
pub use mmap::Mmap;

#[cfg(runtime)]
mod async_file;
#[cfg(runtime)]
pub use async_file::AsyncFile;
