use std::{
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::{ReadDisk, WriteDisk};

#[cfg(feature = "async")]
use super::{AsyncReadDisk, AsyncWriteDisk};

/// [`Plainfile`](`super::Plainfile`), but with no write buffering.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
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

#[cfg(runtime)]
impl AsyncReadDisk for WriteUnbuffered {
    type ReadDisk = futures::io::BufReader<super::async_file::AsyncFile>;

    async fn async_read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        Ok(futures::io::BufReader::new(
            super::async_file::read_file(self.path.clone()).await?,
        ))
    }
}

#[cfg(runtime)]
impl AsyncWriteDisk for WriteUnbuffered {
    type WriteDisk = super::async_file::AsyncFile;

    async fn async_write_disk(&self) -> std::io::Result<Self::WriteDisk> {
        super::async_file::write_file(self.path.clone()).await
    }
}
