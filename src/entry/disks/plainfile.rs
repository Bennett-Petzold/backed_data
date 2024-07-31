use std::{
    fs::File,
    io::{BufReader, BufWriter},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::{ReadDisk, WriteDisk};

#[cfg(feature = "async")]
use super::{AsyncReadDisk, AsyncWriteDisk};

/// A regular file entry.
///
/// This is used to open a [`File`] on demand, but drop the handle when unused.
/// Large collections of [`BackedEntry`]s would otherwise risk overwhelming
/// OS limts on the number of open file descriptors.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

    fn write_disk(&self) -> std::io::Result<Self::WriteDisk> {
        Ok(BufWriter::new(
            File::options()
                .write(true)
                .create(true)
                .truncate(true)
                .open(self.path.clone())?,
        ))
    }
}

#[cfg(feature = "async")]
impl AsyncReadDisk for Plainfile {
    type ReadDisk = tokio::io::BufReader<tokio::fs::File>;

    async fn async_read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        Ok(tokio::io::BufReader::new(
            tokio::fs::File::open(self.path.clone()).await?,
        ))
    }
}

#[cfg(feature = "async")]
impl AsyncWriteDisk for Plainfile {
    type WriteDisk = tokio::io::BufWriter<tokio::fs::File>;

    async fn async_write_disk(&self) -> std::io::Result<Self::WriteDisk> {
        Ok(tokio::io::BufWriter::new(
            tokio::fs::File::options()
                .write(true)
                .create(true)
                .truncate(true)
                .open(self.path.clone())
                .await?,
        ))
    }
}
