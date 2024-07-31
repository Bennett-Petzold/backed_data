pub trait BackedEntryUnload {
    /// Removes the entry from memory.
    fn unload(&mut self);
}

use serde::{Deserialize, Deserializer, Serialize};

#[cfg(feature = "async")]
pub mod async_impl;
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
pub struct BackedEntry<T, Disk: for<'df> Deserialize<'df>> {
    #[serde(skip)]
    value: Option<T>,
    #[serde(deserialize_with = "backing_deserialize")]
    disk: Disk,
}

impl<T: Clone, Disk: for<'df> Deserialize<'df> + Clone> Clone for BackedEntry<T, Disk> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            disk: self.disk.clone(),
        }
    }
}

impl<T, Disk: for<'de> Deserialize<'de>> BackedEntry<T, Disk> {
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

/// Specialized typedef of [`BackedEntry`] for non-pointer types.
///
/// # Example
///
/// ```rust
/// use std::fs::{File, remove_file};
/// use backed_data::entry::BackedEntryBox;
///
/// let FILENAME = std::env::temp_dir().join("example_box");
///
/// // Write array to file
/// let mut writer: BackedEntryBox<str, _> = BackedEntryBox::new(FILENAME.clone());
/// writer.write_unload("HELLO I AM A STRING").unwrap();
/// drop(writer);
///
/// // Read array from file
/// let mut sparse: BackedEntryBox<str, _> = BackedEntryBox::new(FILENAME.clone());
/// assert_eq!(sparse.load().unwrap().as_ref(), "HELLO I AM A STRING");
///
/// // Cleanup
/// remove_file(FILENAME).unwrap();
/// ```
pub type BackedEntryBox<T, Disk> = BackedEntry<Box<T>, Disk>;

/// Specialized typedef of [`BackedEntry`] for arrays.
///
/// # Example
///
/// ```rust
/// use std::fs::{File, remove_file};
/// use backed_data::entry::BackedEntryArr;
///
/// let FILENAME = std::env::temp_dir().join("example_array");
///
/// // Write array to file
/// let mut writer: BackedEntryArr<i32, _> = BackedEntryArr::new(FILENAME.clone());
/// writer.write_unload([1, 2, 3]).unwrap();
/// drop(writer);
///
/// // Read array from file
/// let mut sparse: BackedEntryArr<i32, _> = BackedEntryArr::new(FILENAME.clone());
/// assert_eq!(sparse.load().unwrap().as_ref(), [1, 2, 3]);
///
/// // Cleanup
/// remove_file(FILENAME).unwrap();
/// ```
pub type BackedEntryArr<T, Disk> = BackedEntryBox<[T], Disk>;

// ----- Initializations ----- //

impl<T, Disk: for<'de> Deserialize<'de>> BackedEntry<T, Disk> {
    pub fn new(disk: Disk) -> Self {
        Self { value: None, disk }
    }
}

impl<T, Disk: for<'de> Deserialize<'de>> AsRef<BackedEntry<T, Disk>> for BackedEntry<T, Disk> {
    fn as_ref(&self) -> &BackedEntry<T, Disk> {
        self
    }
}

impl<T, Disk: for<'de> Deserialize<'de>> AsMut<BackedEntry<T, Disk>> for BackedEntry<T, Disk> {
    fn as_mut(&mut self) -> &mut BackedEntry<T, Disk> {
        self
    }
}
