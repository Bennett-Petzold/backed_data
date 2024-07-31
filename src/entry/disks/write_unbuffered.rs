use std::{
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::{ReadDisk, WriteDisk};

#[cfg(feature = "async")]
use super::{AsyncReadDisk, AsyncWriteDisk};

/// [`super::Plainfile`], but with no write buffering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteUnbuffered {
    /// File location.
    path: PathBuf,
}

impl From<PathBuf> for WriteUnbuffered {
    fn from(value: PathBuf) -> Self {
        Self { path: value }
    }
}

impl From<WriteUnbuffered> for PathBuf {
    fn from(val: WriteUnbuffered) -> Self {
        val.path
    }
}

impl AsRef<Path> for WriteUnbuffered {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl WriteUnbuffered {
    pub fn new(path: PathBuf) -> Self {
        path.into()
    }
}

impl ReadDisk for WriteUnbuffered {
    type ReadDisk = BufReader<File>;

    fn read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        Ok(BufReader::new(File::open(self.path.clone())?))
    }
}

impl WriteDisk for WriteUnbuffered {
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
impl AsyncReadDisk for WriteUnbuffered {
    type ReadDisk = tokio::io::BufReader<tokio::fs::File>;

    async fn async_read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        Ok(tokio::io::BufReader::new(
            tokio::fs::File::open(self.path.clone()).await?,
        ))
    }
}

#[cfg(feature = "async")]
impl AsyncWriteDisk for WriteUnbuffered {
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
