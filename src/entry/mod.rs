/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/*!
Defines [`BackedEntry`], the core of this library.

Also defines [`disks`] and [`formats`], which define the backing for [`BackedEntry`].
*/

use std::{cell::OnceCell, sync::OnceLock};

use embed_doc_image::embed_doc_image;
use serde::{Deserialize, Serialize};

use crate::utils::Once;

#[cfg(feature = "async")]
use crate::utils::sync::AsyncOnceLock;

#[cfg(feature = "async")]
pub mod async_impl;
#[cfg(feature = "async")]
pub use async_impl::*;

pub mod sync_impl;
pub use sync_impl::*;

pub mod disks;
pub mod formats;

#[cfg(feature = "async")]
pub mod adapters;
#[cfg(feature = "async")]
pub use adapters::SyncAsAsync;

/// Entry kept on some backing storage, loaded into memory on usage.
///
/// Unloading is explicit.
///
/// Use a heap pointer type or [`BackedEntryBox`], otherwise this will occupy
/// the full type size even when unloaded.
///
/// # Generics
///
/// * `T`: a [`Once`] wrapper over the type to store.
/// * `Disk`: an implementor of [`ReadDisk`](`disks::ReadDisk`) and/or [`WriteDisk`](`disks::WriteDisk`).
/// * `Coder`: an implementor of [`Encoder`](`formats::Encoder`) and/or [`Decoder`](`formats::Decoder`).
///
/// * See [`BackedEntryBox`] for a heap-storing version.
/// * See [`BackedEntryArr`] for an array-specialized version.
/// * See [`BackedEntryAsync`] with feature `async` enabled for async.
///
/// # Example Dataflow
/// ![Example BackedEntry Dataflow][backed_load]
#[embed_doc_image("backed_load", "media_output/backed_load.gif")]
#[derive(Debug, Serialize, Deserialize)]
pub struct BackedEntry<T, Disk, Coder> {
    #[serde(skip)]
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
/// * `Disk`: an implementor of [`AsyncReadDisk`](`disks::AsyncReadDisk`) and/or [`AsyncWriteDisk`](`disks::AsyncWriteDisk`).
/// * `Coder`: an implementor of [`AsyncEncoder`](`formats::AsyncEncoder`) and/or [`AsyncDecoder`](`formats::AsyncDecoder`).
pub type BackedEntryAsync<T, Disk, Coder> = BackedEntry<AsyncOnceLock<T>, Disk, Coder>;

/// Thread-unsafe typedef of [`BackedEntry`] for non-pointer types.
///
/// # Example
///
/// ```rust
/// #[cfg(feature = "bincode")] {
///     use std::fs::{File, remove_file};
///     use backed_data::entry::{
///         BackedEntryBox,
///         formats::BincodeCoder,
///         disks::Plainfile,
///     };
///
///     let FILENAME = std::env::temp_dir().join("example_box");
///     let file = Plainfile::new(FILENAME.clone());
///
///     // Write string to file
///     let mut writer: BackedEntryBox<str, _, BincodeCoder<Box<str>>> =
///         BackedEntryBox::with_disk(file.clone());
///     writer.write_unload("HELLO I AM A STRING").unwrap();
///     drop(writer);
///
///     // Read string from file
///     let mut sparse: BackedEntryBox<str, _, BincodeCoder<_>> =
///         BackedEntryBox::with_disk(file.clone());
///     assert_eq!(sparse.load().unwrap().as_ref(), "HELLO I AM A STRING");
///
///     // Cleanup
///     remove_file(FILENAME).unwrap();
/// }
/// ```
pub type BackedEntryBox<T, Disk, Coder> = BackedEntryCell<Box<T>, Disk, Coder>;

/// Thread-safe typedef of [`BackedEntry`] for non-pointer types.
///
/// # Example
///
/// ```rust
/// #[cfg(feature = "bincode")] {
///     use std::fs::{File, remove_file};
///     use backed_data::entry::{
///         BackedEntryBoxLock,
///         formats::BincodeCoder,
///         disks::Plainfile,
///     };
///
///     let FILENAME = std::env::temp_dir().join("example_box_safe");
///     let file = Plainfile::new(FILENAME.clone());
///
///     // Write string to file
///     let mut writer: BackedEntryBoxLock<str, _, BincodeCoder<Box<str>>> =
///         BackedEntryBoxLock::with_disk(file.clone());
///     writer.write_unload("HELLO I AM A STRING").unwrap();
///     drop(writer);
///
///     // Read string from file
///     let mut sparse: BackedEntryBoxLock<str, _, BincodeCoder<_>> =
///         BackedEntryBoxLock::with_disk(file.clone());
///     assert_eq!(sparse.load().unwrap().as_ref(), "HELLO I AM A STRING");
///
///     // Cleanup
///     remove_file(FILENAME).unwrap();
/// }
/// ```
pub type BackedEntryBoxLock<T, Disk, Coder> = BackedEntryLock<Box<T>, Disk, Coder>;

/// Thread-unsafe typedef of [`BackedEntry`] for arrays.
///
/// # Example
///
/// ```rust
/// #[cfg(feature = "bincode")] {
///     use std::fs::{File, remove_file};
///     use backed_data::entry::{
///         BackedEntryArr,
///         formats::BincodeCoder,
///         disks::Plainfile,
///     };
///    
///     let FILENAME = std::env::temp_dir().join("example_array");
///     let file = Plainfile::new(FILENAME.clone());
///    
///     // Write array to file
///     let mut writer: BackedEntryArr<u8, _, BincodeCoder<_>> = BackedEntryArr::new(file.clone(),
///         BincodeCoder::default());
///     writer.write_unload([1, 2, 3]).unwrap();
///     drop(writer);
///    
///     // Read array from file
///     let mut sparse: BackedEntryArr<u8, _, _> = BackedEntryArr::new(file.clone(),
///         BincodeCoder::default());
///     assert_eq!(sparse.load().unwrap().as_ref(), [1, 2, 3]);
///    
///     // Cleanup
///     remove_file(FILENAME).unwrap();
/// }
/// ```
pub type BackedEntryArr<T, Disk, Coder> = BackedEntryBox<[T], Disk, Coder>;

/// Thread-safe typedef of [`BackedEntry`] for arrays.
///
/// # Example
///
/// ```rust
/// #[cfg(feature = "bincode")] {
///     use std::fs::{File, remove_file};
///     use backed_data::entry::{
///         BackedEntryArrLock,
///         formats::BincodeCoder,
///         disks::Plainfile,
///     };
///    
///     let FILENAME = std::env::temp_dir().join("example_array_safe");
///     let file = Plainfile::new(FILENAME.clone());
///    
///     // Write array to file
///     let mut writer: BackedEntryArrLock<u8, _, BincodeCoder<_>> = BackedEntryArrLock::new(file.clone(),
///         BincodeCoder::default());
///     writer.write_unload([1, 2, 3]).unwrap();
///     drop(writer);
///    
///     // Read array from file
///     let mut sparse: BackedEntryArrLock<u8, _, _> = BackedEntryArrLock::new(file.clone(),
///         BincodeCoder::default());
///     assert_eq!(sparse.load().unwrap().as_ref(), [1, 2, 3]);
///    
///     // Cleanup
///     remove_file(FILENAME).unwrap();
/// }
/// ```
pub type BackedEntryArrLock<T, Disk, Coder> = BackedEntryBoxLock<[T], Disk, Coder>;

// ----- Initializations ----- //

impl<T: Once, Disk, Coder> BackedEntry<T, Disk, Coder> {
    /// See [`Self::with_disk`] when `Coder` implements default.
    pub fn new(disk: Disk, coder: Coder) -> Self {
        Self {
            value: T::new(),
            disk,
            coder,
        }
    }
}

impl<T: Once, Disk, Coder: Default> BackedEntry<T, Disk, Coder> {
    /// [`Self::new`], but builds `Coder` from default.
    pub fn with_disk(disk: Disk) -> Self {
        Self::new(disk, Coder::default())
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

/// Wrapper to minimize bounds specification for [`BackedEntry`].
///
/// [`BackedEntry`] is always this.
/// Only [`BackedEntry`] is this.
pub trait BackedEntryTrait {
    type T;
    type Disk;
    type Coder;

    fn get(self) -> BackedEntry<Self::T, Self::Disk, Self::Coder>;
    fn get_ref(&self) -> &BackedEntry<Self::T, Self::Disk, Self::Coder>;
    fn get_mut(&mut self) -> &mut BackedEntry<Self::T, Self::Disk, Self::Coder>;
}

impl<T, Disk, Coder> BackedEntryTrait for BackedEntry<T, Disk, Coder> {
    type T = T;
    type Disk = Disk;
    type Coder = Coder;

    fn get(self) -> BackedEntry<Self::T, Self::Disk, Self::Coder> {
        self
    }
    fn get_ref(&self) -> &BackedEntry<Self::T, Self::Disk, Self::Coder> {
        self
    }
    fn get_mut(&mut self) -> &mut BackedEntry<Self::T, Self::Disk, Self::Coder> {
        self
    }
}
