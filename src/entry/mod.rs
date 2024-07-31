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
pub struct BackedEntry<T, Disk: for<'df> Deserialize<'df>> {
    #[serde(skip)]
    value: T,
    #[serde(deserialize_with = "backing_deserialize")]
    disk: Disk,
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
/// let mut writer: BackedEntryArr<i32, _> = BackedEntryArr::new(FILENAME.clone());
/// writer.write_unload(&[1, 2, 3]).unwrap();
/// drop(writer);
///
/// // Read array from file
/// let mut sparse: BackedEntryArr<i32, _> = BackedEntryArr::new(FILENAME.clone());
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
/// let mut writer: BackedEntryOption<String, _> = BackedEntryOption::new(FILENAME.clone());
/// writer.write_unload(&"Limited memory".to_string()).unwrap();
/// drop(writer);
///
/// // Read array from file
/// let mut sparse: BackedEntryOption<String, _> = BackedEntryOption::new(FILENAME.clone());
/// assert_eq!(sparse.load().unwrap(), "Limited memory");
///
/// // Cleanup
/// remove_file(FILENAME).unwrap();
/// ```
pub type BackedEntryOption<T, Disk> = BackedEntry<Option<T>, Disk>;

// ----- Initializations ----- //

impl<T, Disk: for<'de> Deserialize<'de>> BackedEntryArr<T, Disk> {
    pub fn new(disk: Disk) -> Self {
        Self {
            value: Box::new([]),
            disk,
        }
    }
}

impl<T, Disk: for<'de> Deserialize<'de>> BackedEntryOption<T, Disk> {
    pub fn new(disk: Disk) -> Self {
        Self { value: None, disk }
    }
}
