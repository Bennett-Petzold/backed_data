use std::io::{Read, Seek, Write};

use bincode::{deserialize_from, serialize_into};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[cfg(feature = "async")]
use {
    async_bincode::tokio::{AsyncBincodeReader, AsyncBincodeWriter},
    futures::{SinkExt, StreamExt},
    std::borrow::Borrow,
    tokio::io::{AsyncRead, AsyncSeek, AsyncSeekExt, AsyncWrite, AsyncWriteExt},
    tokio_util::io::SyncIoBridge,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackedEntry<T, Disk: ?Sized> {
    value: T,
    disk_entry: Disk,
}

pub trait BackedEntryUnload {
    /// Removes the entry from memory.
    fn unload(&mut self);
}

#[cfg(feature = "async")]
/// Used by [`BackedEntryAsync`] to track the valid reader.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackedEntryWriteMode {
    Sync,
    Async,
}

#[cfg(feature = "async")]
/// Async variant of [`BackedEntry`].
///
/// # Why Separate?
/// AsyncBincodeReader and BincodeReader use different formats.
/// Therefore all writes and reads from AsyncBincodeWriter must support
/// one or the other. This adds resolution so only the valid read is possible.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackedEntryAsync<T, Disk: ?Sized> {
    mode: BackedEntryWriteMode,
    inner: BackedEntry<T, Disk>,
}

#[cfg(feature = "async")]
impl<T, Disk> BackedEntryUnload for BackedEntryAsync<T, Disk>
where
    BackedEntry<T, Disk>: BackedEntryUnload,
{
    fn unload(&mut self) {
        self.inner.unload()
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

#[cfg(feature = "async")]
impl<T, Disk> BackedEntryAsync<T, Disk> {
    pub fn get_disk(&self) -> &Disk {
        &self.inner.disk_entry
    }

    pub fn get_disk_mut(&mut self) -> &mut Disk {
        &mut self.inner.disk_entry
    }

    pub fn get_mode(&self) -> &BackedEntryWriteMode {
        &self.mode
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

#[cfg(feature = "async")]
/// Async version of [`BackedEntryArr`]
pub type BackedEntryArrAsync<T, Disk> = BackedEntryAsync<Box<[T]>, Disk>;

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

#[cfg(feature = "async")]
/// Async version of [`BackedEntryOption`]
pub type BackedEntryOptionAsync<T, Disk> = BackedEntryAsync<Option<T>, Disk>;

// ----- Initializations ----- //

impl<T, Disk> BackedEntryArr<T, Disk> {
    pub fn new(disk_entry: Disk) -> Self {
        Self {
            value: Box::new([]),
            disk_entry,
        }
    }
}

#[cfg(feature = "async")]
impl<T, Disk> BackedEntryArrAsync<T, Disk> {
    pub fn new(disk_entry: Disk, mode: Option<BackedEntryWriteMode>) -> Self {
        let mode = mode.unwrap_or(BackedEntryWriteMode::Async);
        Self {
            inner: BackedEntryArr::new(disk_entry),
            mode,
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

#[cfg(feature = "async")]
impl<T, Disk> BackedEntryOptionAsync<T, Disk> {
    pub fn new(disk_entry: Disk, mode: Option<BackedEntryWriteMode>) -> Self {
        let mode = mode.unwrap_or(BackedEntryWriteMode::Async);
        Self {
            inner: BackedEntryOption::new(disk_entry),
            mode,
        }
    }
}

// ----- General Implementations ----- //

impl<T: Serialize, Disk: Write> BackedEntry<T, Disk> {
    /// Writes the new value to memory and disk.
    ///
    /// See [`Self::write_unload`] to skip the memory write.
    pub fn write(&mut self, new_value: T) -> bincode::Result<()> {
        serialize_into(&mut self.disk_entry, &new_value)?;
        self.disk_entry.flush()?; // Make sure buffer is emptied
        self.value = new_value;
        Ok(())
    }
}

impl<T: Serialize, Disk: Write> BackedEntry<T, Disk>
where
    BackedEntry<T, Disk>: BackedEntryUnload,
{
    /// Write the value to disk only, unloading current memory.
    ///
    /// See [`Self::write`] to keep the value in memory.
    pub fn write_unload<U>(&mut self, new_value: &U) -> bincode::Result<()>
    where
        for<'a> &'a U: Into<&'a T>,
    {
        self.unload();
        serialize_into(&mut self.disk_entry, new_value.into())?;
        self.disk_entry.flush()?; // Make sure buffer is emptied
        Ok(())
    }
}

#[cfg(feature = "async")]
impl<T: Serialize, Disk: AsyncWrite + Unpin> BackedEntryAsync<T, Disk> {
    #[inline]
    async fn update_lower_async<U: Borrow<T>>(&mut self, new_value: U) -> bincode::Result<()> {
        let new_value = new_value.borrow();
        let mut bincode_writer = AsyncBincodeWriter::from(&mut self.inner.disk_entry);
        match self.mode {
            BackedEntryWriteMode::Async => {
                bincode_writer.for_async().send(&new_value).await?;
            }
            BackedEntryWriteMode::Sync => {
                bincode_writer.send(&new_value).await?;
            }
        };

        self.inner.disk_entry.flush().await?;
        Ok(())
    }

    /// Writes the new value to memory and disk.
    ///
    /// See [`Self::async_write_unload`] to skip the memory write.
    pub async fn write_async(&mut self, new_value: T) -> bincode::Result<()> {
        self.update_lower_async(&new_value).await?;
        self.inner.value = new_value;
        Ok(())
    }

    /// Convert the backed version to be sync read compatible
    pub async fn conv_to_sync(&mut self) -> bincode::Result<()> {
        if self.mode == BackedEntryWriteMode::Async {
            self.mode = BackedEntryWriteMode::Sync;
            AsyncBincodeWriter::from(&mut self.inner.disk_entry)
                .send(&self.inner.value)
                .await?;
            self.inner.disk_entry.flush().await?;
        }
        Ok(())
    }

    /// Convert the backed version to be async read compatible
    pub async fn conv_to_async(&mut self) -> bincode::Result<()> {
        if self.mode == BackedEntryWriteMode::Sync {
            self.mode = BackedEntryWriteMode::Async;
            AsyncBincodeWriter::from(&mut self.inner.disk_entry)
                .for_async()
                .send(&self.inner.value)
                .await?;
            self.inner.disk_entry.flush().await?;
        }
        Ok(())
    }
}

#[cfg(feature = "async")]
impl<T: Serialize, Disk: AsyncWrite + Unpin> BackedEntryAsync<T, Disk>
where
    BackedEntryAsync<T, Disk>: BackedEntryUnload,
{
    /// Write the value to disk only, unloading current memory.
    ///
    /// See [`Self::async_write`] to keep the value in memory.
    pub async fn write_unload_async(&mut self, new_value: &T) -> bincode::Result<()> {
        self.unload();
        self.update_lower_async(new_value).await
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
impl<T: DeserializeOwned, Disk: AsyncRead + AsyncSeek + Unpin> BackedEntryArrAsync<T, Disk> {
    /// Async version of [`BackedEntryArr::load`].
    ///
    /// Will use AsyncBincodeReader or BincodeReader depending on mode.
    /// Check that [`Self::get_mode`] is async for optimal read performance.
    /// The sync implementation will block the thread.
    pub async fn load(&mut self) -> Result<&[T], Box<bincode::ErrorKind>> {
        if self.inner.value.is_empty() {
            self.inner.disk_entry.rewind().await?;
            match self.mode {
                BackedEntryWriteMode::Sync => {
                    self.inner.value =
                        deserialize_from(SyncIoBridge::new(&mut self.inner.disk_entry))?;
                }
                BackedEntryWriteMode::Async => {
                    self.inner.value = AsyncBincodeReader::from(&mut self.inner.disk_entry)
                        .next()
                        .await
                        .ok_or(bincode::ErrorKind::Custom(
                            "AsyncBincodeReader stream empty".to_string(),
                        ))??
                }
            }
        }
        Ok(&self.inner.value)
    }
}

impl<T, U> BackedEntryUnload for BackedEntryArr<T, U> {
    fn unload(&mut self) {
        self.value = Box::new([]);
    }
}

impl<T: Serialize, Disk: Write> BackedEntryArr<T, Disk> {
    /// [`Self::write_unload`] that takes a slice, to avoid a box copy
    pub fn write_unload_slice(&mut self, new_value: &[T]) -> bincode::Result<()> {
        self.unload();
        serialize_into(&mut self.disk_entry, new_value)?;
        self.disk_entry.flush()?; // Make sure buffer is emptied
        Ok(())
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
impl<T: DeserializeOwned, Disk: AsyncRead + Unpin> BackedEntryOptionAsync<T, Disk> {
    /// Async version of [`BackedEntryOption::load`].
    ///
    /// Will use AsyncBincodeReader or BincodeReader depending on mode.
    /// Check that [`Self::get_mode`] is async for optimal read performance.
    /// The sync implementation will block the thread.
    pub async fn load_async(&mut self) -> Result<&T, Box<bincode::ErrorKind>> {
        if self.inner.value.is_none() {
            match self.mode {
                BackedEntryWriteMode::Sync => {
                    self.inner.value =
                        deserialize_from(SyncIoBridge::new(&mut self.inner.disk_entry))?;
                }
                BackedEntryWriteMode::Async => {
                    self.inner.value = AsyncBincodeReader::from(&mut self.inner.disk_entry)
                        .next()
                        .await
                        .ok_or(bincode::ErrorKind::Custom(
                            "AsyncBincodeReader stream empty".to_string(),
                        ))??
                }
            }
        }
        Ok(self.inner.value.as_ref().unwrap())
    }
}

impl<T, U> BackedEntryUnload for BackedEntryOption<T, U> {
    fn unload(&mut self) {
        self.value = None;
    }
}
