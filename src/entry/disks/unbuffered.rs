use std::{
    fs::File,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::{ReadDisk, WriteDisk};

#[cfg(feature = "async")]
use super::{AsyncReadDisk, AsyncWriteDisk};

/// [`super::Plainfile`], but with no buffering at all.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    type ReadDisk = File;

    fn read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        File::open(self.path.clone())
    }
}

impl WriteDisk for Unbuffered {
    type WriteDisk = File;

    fn write_disk(&self) -> std::io::Result<Self::WriteDisk> {
        File::options()
            .write(true)
            .create(true)
            .truncate(true)
            .open(self.path.clone())
    }
}

#[cfg(feature = "async")]
impl AsyncReadDisk for Unbuffered {
    type ReadDisk = tokio::fs::File;

    async fn async_read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        tokio::fs::File::open(self.path.clone()).await
    }
}

#[cfg(feature = "async")]
impl AsyncWriteDisk for Unbuffered {
    type WriteDisk = tokio::fs::File;

    async fn async_write_disk(&self) -> std::io::Result<Self::WriteDisk> {
        tokio::fs::File::options()
            .write(true)
            .create(true)
            .truncate(true)
            .open(self.path.clone())
            .await
    }
}
