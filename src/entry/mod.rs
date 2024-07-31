pub trait BackedEntryUnload {
    /// Removes the entry from memory.
    fn unload(&mut self);
}

use std::{cell::OnceCell, sync::OnceLock};

use serde::{Deserialize, Deserializer, Serialize};

use crate::utils::Once;

#[cfg(feature = "async")]
pub mod async_impl;
pub mod formats;
pub mod sync_impl;

/// Indicates that a disk entry can be overwritten safely.
///
/// Okay for many types of storage (e.g. a solo reference to a file),
/// dangerous for some types of storage (e.g. a slice into a file used to store
/// multiple entries).
/// Only implement this for backing stores that can be freely overwritten with
/// any size.
pub trait DiskOverwritable {}

/// Entry kept on some backing storage, loaded into memory on request.
///
/// Use a heap pointer type or [`BackedEntryBox`], otherwise this will occupy
/// the full type size even when unloaded.
/// Writing/reading is serialized with bincode
///     <https://docs.rs/bincode/latest/bincode/> for space efficiency.
///
/// # Generics
///
/// * `T`: the type to store
/// * `Disk`: any backing store with write/read (or the async equivalent)
///
/// # Examples
/// See [`BackedEntryBox`] for a heap-storing version.
/// See [`BackedEntryArr`] for an array-specialized version.
#[derive(Debug, Serialize, Deserialize)]
pub struct BackedEntry<T, Disk: for<'df> Deserialize<'df>, Coder> {
    #[serde(skip)]
    value: T,
    #[serde(deserialize_with = "backing_deserialize")]
    disk: Disk,
    coder: Coder,
}

impl<T: Clone, Disk: for<'df> Deserialize<'df> + Clone, Coder: Clone> Clone
    for BackedEntry<T, Disk, Coder>
{
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            disk: self.disk.clone(),
            coder: self.coder.clone(),
        }
    }
}

impl<T, Disk: for<'de> Deserialize<'de>, Coder> BackedEntry<T, Disk, Coder> {
    pub fn get_disk(&self) -> &Disk {
        &self.disk
    }

    pub fn get_disk_mut(&mut self) -> &mut Disk {
        &mut self.disk
    }
}

fn backing_deserialize<'de, D, Backing: Deserialize<'de>>(
    deserializer: D,
) -> Result<Backing, D::Error>
where
    D: Deserializer<'de>,
{
    Deserialize::deserialize(deserializer)
}

pub type BackedEntryCell<T, Disk, Coder> = BackedEntry<OnceCell<T>, Disk, Coder>;
pub type BackedEntryLock<T, Disk, Coder> = BackedEntry<OnceLock<T>, Disk, Coder>;

/// Specialized typedef of [`BackedEntry`] for non-pointer types.
///
/// # Example
///
/// ```rust
/// use std::fs::{File, remove_file};
/// use backed_data::entry::{
///     BackedEntryBox,
///     formats::BincodeCoder,
/// };
///
/// let FILENAME = std::env::temp_dir().join("example_box");
/// // Write array to file
/// let mut writer: BackedEntryBox<str, _, _> = BackedEntryBox::new(FILENAME.clone(),
///     BincodeCoder{});
/// writer.write_unload("HELLO I AM A STRING").unwrap();
/// drop(writer);
///
/// // Read array from file
/// let mut sparse: BackedEntryBox<str, _, _> = BackedEntryBox::new(FILENAME.clone(),
///     BincodeCoder{});
/// assert_eq!(sparse.load().unwrap().as_ref(), "HELLO I AM A STRING");
///
/// // Cleanup
/// remove_file(FILENAME).unwrap();
/// ```
pub type BackedEntryBox<T, Disk, Coder> = BackedEntryCell<Box<T>, Disk, Coder>;
pub type BackedEntryBoxLock<T, Disk, Coder> = BackedEntryLock<Box<T>, Disk, Coder>;

/// Specialized typedef of [`BackedEntry`] for arrays.
///
/// # Example
///
/// ```rust
/// use std::fs::{File, remove_file};
/// use backed_data::entry::{
///     BackedEntryArr,
///     formats::BincodeCoder,
/// };
///
/// let FILENAME = std::env::temp_dir().join("example_array");
///
/// // Write array to file
/// let mut writer: BackedEntryArr<u8, _, _> = BackedEntryArr::new(FILENAME.clone(),
///     BincodeCoder{});
/// writer.write_unload([1, 2, 3]).unwrap();
/// drop(writer);
///
/// // Read array from file
/// let mut sparse: BackedEntryArr<u8, _, _> = BackedEntryArr::new(FILENAME.clone(),
///     BincodeCoder{});
/// assert_eq!(sparse.load().unwrap().as_ref(), [1, 2, 3]);
///
/// // Cleanup
/// remove_file(FILENAME).unwrap();
/// ```
pub type BackedEntryArr<T, Disk, Coder> = BackedEntryBox<[T], Disk, Coder>;
pub type BackedEntryArrLock<T, Disk, Coder> = BackedEntryBoxLock<[T], Disk, Coder>;

// ----- Initializations ----- //

impl<T: Once, Disk: for<'de> Deserialize<'de>, Coder> BackedEntry<T, Disk, Coder> {
    pub fn new(disk: Disk, coder: Coder) -> Self {
        Self {
            value: T::new(),
            disk,
            coder,
        }
    }
}

impl<T, Disk: for<'de> Deserialize<'de>, Coder> AsRef<BackedEntry<T, Disk, Coder>>
    for BackedEntry<T, Disk, Coder>
{
    fn as_ref(&self) -> &BackedEntry<T, Disk, Coder> {
        self
    }
}

impl<T, Disk: for<'de> Deserialize<'de>, Coder> AsMut<BackedEntry<T, Disk, Coder>>
    for BackedEntry<T, Disk, Coder>
{
    fn as_mut(&mut self) -> &mut BackedEntry<T, Disk, Coder> {
        self
    }
}
