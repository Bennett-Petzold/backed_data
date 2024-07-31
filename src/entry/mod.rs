use std::{cell::OnceCell, sync::OnceLock};

use crate::utils::Once;

#[cfg(feature = "async")]
pub mod async_impl;
pub mod sync_impl;

pub mod disks;
pub mod formats;

/// Entry kept on some backing storage, loaded into memory on request.
///
/// Use a heap pointer type or [`BackedEntryBox`], otherwise this will occupy
/// the full type size even when unloaded.
///
/// # Generics
///
/// * `T`: a [`Once`] wrapper over the type to store.
/// * `Disk`: an implementor of [`ReadDisk`] and/or [`WriteDisk`].
/// * `Coder`: an implementor of [`Encoder`] and/or [`Decoder`].
///
/// * See [`BackedEntryBox`] for a heap-storing version.
/// * See [`BackedEntryArr`] for an array-specialized version.
/// * See [`BackedEntryAsync`] with feature [`async`] enabled for async.
#[derive(Debug)]
pub struct BackedEntry<T, Disk, Coder> {
    value: T,
    disk: Disk,
    coder: Coder,
}

impl<T: Clone, Disk: Clone, Coder: Clone> Clone for BackedEntry<T, Disk, Coder> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            disk: self.disk.clone(),
            coder: self.coder.clone(),
        }
    }
}

impl<T, Disk, Coder> BackedEntry<T, Disk, Coder> {
    pub fn get_disk(&self) -> &Disk {
        &self.disk
    }

    pub fn get_disk_mut(&mut self) -> &mut Disk {
        &mut self.disk
    }
}

/// Thread unsafe [`BackedEntry`] with inner value `T`.
pub type BackedEntryCell<T, Disk, Coder> = BackedEntry<OnceCell<T>, Disk, Coder>;
/// Thread safe [`BackedEntry`] with inner value `T`.
pub type BackedEntryLock<T, Disk, Coder> = BackedEntry<OnceLock<T>, Disk, Coder>;

#[cfg(feature = "async")]
/// Async specialization of [`BackedEntry`].
///
/// Async versions of methods are prepended with `a_`.
///
/// Only implemented over a [`tokio::sync::OnceCell`], so it can be used within
/// a tokio multi-threaded runtime without blocking.
///
/// # Generics
///
/// * `T`: the type to store.
/// * `Disk`: an implementor of [`AsyncReadDisk`] and/or [`AsyncWriteDisk`].
/// * `Coder`: an implementor of [`AsyncEncoder`] and/or [`AsyncDecoder`].
pub type BackedEntryAsync<T, Disk, Coder> = BackedEntry<tokio::sync::OnceCell<T>, Disk, Coder>;

/// Specialized typedef of [`BackedEntry`] for non-pointer types.
///
/// # Example
///
/// ```rust
/// use std::fs::{File, remove_file};
/// use backed_data::entry::{
///     BackedEntryBox,
///     formats::BincodeCoder,
///     disks::Plainfile,
/// };
///
/// let FILENAME = std::env::temp_dir().join("example_box");
/// let file = Plainfile::new(FILENAME.clone());
///
/// // Write array to file
/// let mut writer: BackedEntryBox<str, _, _> = BackedEntryBox::new(file.clone(),
///     BincodeCoder{});
/// writer.write_unload("HELLO I AM A STRING").unwrap();
/// drop(writer);
///
/// // Read array from file
/// let mut sparse: BackedEntryBox<str, _, _> = BackedEntryBox::new(file.clone(),
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
///     disks::Plainfile,
/// };
///
/// let FILENAME = std::env::temp_dir().join("example_array");
/// let file = Plainfile::new(FILENAME.clone());
///
/// // Write array to file
/// let mut writer: BackedEntryArr<u8, _, _> = BackedEntryArr::new(file.clone(),
///     BincodeCoder{});
/// writer.write_unload([1, 2, 3]).unwrap();
/// drop(writer);
///
/// // Read array from file
/// let mut sparse: BackedEntryArr<u8, _, _> = BackedEntryArr::new(file.clone(),
///     BincodeCoder{});
/// assert_eq!(sparse.load().unwrap().as_ref(), [1, 2, 3]);
///
/// // Cleanup
/// remove_file(FILENAME).unwrap();
/// ```
pub type BackedEntryArr<T, Disk, Coder> = BackedEntryBox<[T], Disk, Coder>;
pub type BackedEntryArrLock<T, Disk, Coder> = BackedEntryBoxLock<[T], Disk, Coder>;

// ----- Initializations ----- //

impl<T: Once, Disk, Coder> BackedEntry<T, Disk, Coder> {
    pub fn new(disk: Disk, coder: Coder) -> Self {
        Self {
            value: T::new(),
            disk,
            coder,
        }
    }
}

impl<T, Disk, Coder> AsRef<BackedEntry<T, Disk, Coder>> for BackedEntry<T, Disk, Coder> {
    fn as_ref(&self) -> &BackedEntry<T, Disk, Coder> {
        self
    }
}

impl<T, Disk, Coder> AsMut<BackedEntry<T, Disk, Coder>> for BackedEntry<T, Disk, Coder> {
    fn as_mut(&mut self) -> &mut BackedEntry<T, Disk, Coder> {
        self
    }
}
