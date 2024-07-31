use std::io::{Read, Seek, Write};

use bincode::{deserialize_from, serialize_into};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[cfg(feature = "async")]
use {
    async_bincode::tokio::{AsyncBincodeReader, AsyncBincodeWriter},
    futures::{SinkExt, StreamExt},
    tokio::io::{AsyncRead, AsyncWrite},
};

/// Entry kept on some backing storage, loaded into memory on request.
///
/// This is only useful for types that can occupy zero/almost zero heap,
/// so most types should use the generic Option wrapper with a boxed type.
/// Boxed arrays can occupy zero heap, so they have a special implementation.
/// Broader collections do not have a special implementation, as they do not
/// guarantee memory freeing.
///
/// Writing/reading is serialized with bincode
///     <https://docs.rs/bincode/latest/bincode/> for space efficiency.
///
/// # Generics
///
/// * `T`: the type to store (should use heap, to make clearing useful)
/// * `Disk`: any backing store with write/read (or the async equivalent)
///
/// # Examples
/// See [`BackedEntryArr`] and [`BackedEntryOption`]
#[derive(Debug, Serialize, Deserialize)]
pub struct BackedEntry<T, Disk: ?Sized> {
    value: T,
    disk_entry: Disk,
}

impl<T: Clone, Disk: Clone> Clone for BackedEntry<T, Disk> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            disk_entry: self.disk_entry.clone(),
        }
    }
}

impl<T, Disk> BackedEntry<T, Disk> {
    pub fn get_disk(&self) -> &Disk {
        &self.disk_entry
    }

    pub fn get_disk_mut(&mut self) -> &mut Disk {
        &mut self.disk_entry
    }
}

/// Typedef of [`BackedEntry`].
///
/// # Example
///
/// ```rust
/// use std::fs::{File, remove_file};
/// use backed_array::entry::BackedEntryArr;
///
/// let FILENAME = std::env::temp_dir().join("example_array");
///
/// // Write array to file
/// let file = File::create(FILENAME.clone()).unwrap();
/// let mut writer: BackedEntryArr<i32, _> = BackedEntryArr::new(file);
/// writer.write_unload(&[1, 2, 3]).unwrap();
/// drop(writer);
///
/// // Read array from file
/// let file = File::open(FILENAME.clone()).unwrap();
/// let mut sparse: BackedEntryArr<i32, _> = BackedEntryArr::new(file);
/// assert_eq!(sparse.load().unwrap(), [1, 2, 3]);
///
/// // Cleanup
/// remove_file(FILENAME).unwrap();
/// ```
pub type BackedEntryArr<T, Disk> = BackedEntry<Box<[T]>, Disk>;

/// Typedef of [`BackedEntry`].
///
/// # Example
///
/// ```rust
/// use std::fs::{File, remove_file};
/// use std::env::temp_dir;
/// use backed_array::entry::BackedEntryOption;
///
/// let FILENAME = std::env::temp_dir().join("example_option");
///
/// // Write array to file
/// let file = File::create(FILENAME.clone()).unwrap();
/// let mut writer: BackedEntryOption<String, _> = BackedEntryOption::new(file);
/// writer.write_unload(&"Limited memory".to_string()).unwrap();
/// drop(writer);
///
/// // Read array from file
/// let file = File::open(FILENAME.clone()).unwrap();
/// let mut sparse: BackedEntryOption<String, _> = BackedEntryOption::new(file);
/// assert_eq!(sparse.load().unwrap(), "Limited memory");
///
/// // Cleanup
/// remove_file(FILENAME).unwrap();
/// ```
pub type BackedEntryOption<T, Disk> = BackedEntry<Option<T>, Disk>;

// ----- Initializations ----- //

impl<T, Disk> BackedEntryArr<T, Disk> {
    pub fn new(disk_entry: Disk) -> Self {
        Self {
            value: Box::new([]),
            disk_entry,
        }
    }
}

impl<T, Disk> BackedEntryOption<T, Disk> {
    pub fn new(disk_entry: Disk) -> Self {
        Self {
            value: None,
            disk_entry,
        }
    }
}

// ----- Boxed Array Implementations ----- //

impl<T: DeserializeOwned, Disk: Read + Seek> BackedEntryArr<T, Disk> {
    /// Returns the entry, loading from disk if not in memory.
    ///
    /// Will remain in memory until an explicit call to unload.
    pub fn load(&mut self) -> bincode::Result<&[T]> {
        if self.value.is_empty() {
            self.disk_entry.rewind()?;
            self.value = deserialize_from(&mut self.disk_entry)?;
        }
        Ok(&self.value)
    }
}

#[cfg(feature = "async")]
impl<T: DeserializeOwned, Disk: AsyncRead + Unpin> BackedEntryArr<T, Disk> {
    /// Returns the entry, loading from disk if not in memory.
    ///
    /// Will remain in memory until an explicit call to unload.
    pub async fn load_async(&mut self) -> Result<&[T], Box<bincode::ErrorKind>> {
        if self.value.is_empty() {
            self.value = AsyncBincodeReader::from(&mut self.disk_entry)
                .next()
                .await
                .ok_or(bincode::ErrorKind::Custom(
                    "AsyncBincodeReader stream empty".to_string(),
                ))??;
        }
        Ok(&self.value)
    }
}

impl<T: Serialize, Disk: Write> BackedEntryArr<T, Disk> {
    /// Writes the new value to memory and disk.
    ///
    /// See [`Self::write_unload`] to skip the memory write.
    pub fn write(&mut self, new_value: Box<[T]>) -> bincode::Result<()> {
        serialize_into(&mut self.disk_entry, &new_value)?;
        self.disk_entry.flush()?; // Make sure buffer is emptied
        self.value = new_value;
        Ok(())
    }

    /// Write the value to disk only, unloading current memory.
    ///
    /// See [`Self::write`] to keep the value in memory.
    pub fn write_unload(&mut self, new_value: &[T]) -> bincode::Result<()> {
        self.unload();
        serialize_into(&mut self.disk_entry, new_value)?;
        self.disk_entry.flush()?; // Make sure buffer is emptied
        Ok(())
    }
}

#[cfg(feature = "async")]
impl<T: Serialize, Disk: AsyncWrite + Unpin> BackedEntryArr<T, Disk> {
    /// Writes the new value to memory and disk.
    ///
    /// See [`Self::async_write_unload`] to skip the memory write.
    pub async fn write_async(&mut self, new_value: Box<[T]>) -> bincode::Result<()> {
        AsyncBincodeWriter::from(&mut self.disk_entry)
            .send(&new_value)
            .await?;
        self.value = new_value;
        Ok(())
    }

    /// Write the value to disk only, unloading current memory.
    ///
    /// See [`Self::async_write`] to keep the value in memory.
    pub async fn write_unload_async(&mut self, new_value: &[T]) -> bincode::Result<()> {
        self.unload();
        AsyncBincodeWriter::from(&mut self.disk_entry)
            .send(&new_value)
            .await
    }
}

impl<T, U> BackedEntryArr<T, U> {
    /// Removes the entry from memory.
    pub fn unload(&mut self) {
        self.value = Box::new([]);
    }
}

// ----- Option Implementations ----- //

impl<T: DeserializeOwned, Disk: Read> BackedEntryOption<T, Disk> {
    /// Returns the entry, loading from disk if not in memory.
    ///
    /// Will remain in memory until an explicit call to unload.
    pub fn load(&mut self) -> bincode::Result<&T> {
        if self.value.is_none() {
            self.value = Some(deserialize_from(&mut self.disk_entry)?);
        }
        Ok(self.value.as_ref().unwrap())
    }
}

#[cfg(feature = "async")]
impl<T: DeserializeOwned, Disk: AsyncRead + Unpin> BackedEntryOption<T, Disk> {
    /// Returns the entry, loading from disk if not in memory.
    ///
    /// Will remain in memory until an explicit call to unload.
    pub async fn load_async(&mut self) -> Result<&T, Box<bincode::ErrorKind>> {
        if self.value.is_none() {
            self.value = AsyncBincodeReader::from(&mut self.disk_entry)
                .next()
                .await
                .ok_or(bincode::ErrorKind::Custom(
                    "AsyncBincodeReader stream empty".to_string(),
                ))??;
        }
        Ok(self.value.as_ref().unwrap())
    }
}

impl<T: Serialize, Disk: Write> BackedEntryOption<T, Disk> {
    /// Writes the new value to memory and disk.
    ///
    /// See [`Self::write_unload`] to skip the memory write.
    pub fn write(&mut self, new_value: T) -> bincode::Result<()> {
        serialize_into(&mut self.disk_entry, &new_value)?;
        self.value = Some(new_value);
        Ok(())
    }

    /// Write the value to disk only, unloading current memory.
    ///
    /// See [`Self::write`] to keep the value in memory.
    pub fn write_unload(&mut self, new_value: &T) -> bincode::Result<()> {
        self.unload();
        serialize_into(&mut self.disk_entry, new_value)
    }
}

#[cfg(feature = "async")]
impl<T: Serialize, Disk: AsyncWrite + Unpin> BackedEntryOption<T, Disk> {
    /// Writes the new value to memory and disk.
    ///
    /// See [`Self::async_write_unload`] to skip the memory write.
    pub async fn write_async(&mut self, new_value: T) -> bincode::Result<()> {
        AsyncBincodeWriter::from(&mut self.disk_entry)
            .send(&new_value)
            .await?;
        self.value = Some(new_value);
        Ok(())
    }

    /// Write the value to disk only, unloading current memory.
    ///
    /// See [`Self::async_write`] to keep the value in memory.
    pub async fn write_unload_async(&mut self, new_value: &T) -> bincode::Result<()> {
        self.unload();
        AsyncBincodeWriter::from(&mut self.disk_entry)
            .send(&new_value)
            .await
    }
}

impl<T, U> BackedEntryOption<T, U> {
    /// Removes the entry from memory.
    pub fn unload(&mut self) {
        self.value = None;
    }
}
