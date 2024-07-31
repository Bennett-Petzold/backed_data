use std::io::{Read, Write};

#[cfg(feature = "async")]
use {
    futures::Future,
    tokio::io::{AsyncRead, AsyncWrite},
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

mod custom;
pub use custom::*;

#[cfg(any(feature = "zstd", feature = "async_zstd"))]
mod zstd;
#[cfg(any(feature = "zstd", feature = "async_zstd"))]
pub use zstd::*;

#[cfg(feature = "encrypted")]
mod encrypted;
#[cfg(feature = "encrypted")]
pub use encrypted::*;
