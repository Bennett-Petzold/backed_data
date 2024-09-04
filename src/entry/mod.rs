/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/*!
Defines [`BackedEntryInner`], the core of this library.

Also defines [`disks`] and [`formats`], which define the backing for [`BackedEntryInner`].
*/

use std::ops::{Deref, DerefMut};

use embed_doc_image::embed_doc_image;

use crate::utils::Once;

#[cfg(feature = "async")]
pub mod async_impl;
#[cfg(feature = "async")]
pub use async_impl::*;

pub mod sync_impl;
pub use sync_impl::*;

mod aliases;
pub use aliases::*;
mod shared_functionality;
use shared_functionality::*;

pub mod disks;
pub mod formats;

/*
#[cfg(feature = "async")]
pub mod adapters;
#[cfg(feature = "async")]
pub use adapters::SyncAsAsync;
*/

/// Entry kept on some backing storage, loaded into memory on usage.
///
/// The backed entry methods are split across this, [`BackedEntryRead`], [`BackedEntryWrite`], and
/// [`BackedEntryMutHandle`]. [`BackedEntryMutHandle`] has the full
/// capabilities; if at least one of the `Read`/`Write` traits is not
/// implemented this overall trait is useless.
///
/// Unloading is explicit.
///
/// Use a heap pointer type or [`BackedEntryBox`], otherwise this will occupy
/// the full type size even when unloaded.
///
/// # Generics
///
/// * `OnceWrapper`: a [`Once`] wrapper over the type to store.
/// * `Disk`: an implementor of [`ReadDisk`](`disks::ReadDisk`) and/or [`WriteDisk`](`disks::WriteDisk`). Uses the async trait versions for the async impl.
/// * `Coder`: an implementor of [`Encoder`](`formats::Encoder`) and/or [`Decoder`](`formats::Decoder`). Uses the async trait versions for the async impl.
///
/// * See [`BackedEntrySync`] for the base synchronous version.
/// * See [`BackedEntryBox`] for a heap-storing version.
/// * See [`BackedEntryArr`] for an array-specialized version.
/// * See [`BackedEntryAsync`] with feature `async` enabled for async.
///
/// # Example Dataflow
/// ![Example BackedEntry Dataflow][backed_load]
#[embed_doc_image("backed_load", "media_output/backed_load.gif")]
pub trait BackedEntry {
    type OnceWrapper: Once;
    type Disk;
    type Coder;

    fn get_storage(&self) -> (&Self::Disk, &Self::Coder);
    fn get_storage_mut(&mut self) -> (&mut Self::Disk, &mut Self::Coder);

    /// Returns true if the value is currently in memory.
    fn is_loaded(&self) -> bool;

    /// Removes the value from memory, if loaded.
    fn unload(&mut self);

    /// Take the stored value if in memory.
    fn take(self) -> Option<<Self::OnceWrapper as Once>::Inner>;

    fn new(disk: Self::Disk, coder: Self::Coder) -> Self
    where
        Self: Sized;
}

/// A [`BackedEntry`] that supports loading from disk.
pub trait BackedEntryRead: BackedEntry {
    type LoadResult<'a>
    where
        Self: 'a;
    /// Returns the entry, loading from disk if not in memory.
    ///
    /// Will remain in memory until an explicit call to unload.
    fn load(&self) -> Self::LoadResult<'_>;
}

/// A [`BackedEntry`] that supports writing to disk.
pub trait BackedEntryWrite: BackedEntry {
    type UpdateResult<'a>
    where
        Self: 'a;
    /// Updates underlying storage with the current entry.
    fn update(&mut self) -> Self::UpdateResult<'_>;

    /// Updates underlying storage with the current entry.
    ///
    /// See [`Self::write_unload`] to skip the memory write.
    fn write<U>(&mut self, new_value: U) -> Self::UpdateResult<'_>
    where
        U: Into<<Self::OnceWrapper as Once>::Inner>;

    /// Write the value to disk only, unloading current memory.
    ///
    /// See [`Self::write`] to keep the value in memory.
    fn write_ref<'a>(
        &'a mut self,
        new_value: &'a <Self::OnceWrapper as Once>::Inner,
    ) -> Self::UpdateResult<'a>;
}

/// Handle that protects the mutation of some [`BackedEntry`].
///
/// Modifying by [`BackedEntryWrite::write`] writes the entire value to the
/// underlying storage on every modification. This allows for multiple values
/// values to be written before syncing with disk.
///
/// Call [`Self::flush`] to sync changes with underlying storage before
/// dropping. Otherwise the contents are not guaranteed to be written to disk.
pub trait MutHandle<T>: Deref<Target = T> + DerefMut {
    /// Returns true if the memory version is desynced from the disk version
    fn is_modified(&self) -> bool;

    type FlushResult<'a>
    where
        Self: 'a;
    /// Saves modifications to disk, unsetting the modified flag if sucessful.
    fn flush(&mut self) -> Self::FlushResult<'_>;
}

/// A [`BackedEntry`] that can be mutated.
///
/// This trait exposes full capabilities of [`BackedEntry`], as it requires the read and write traits.
pub trait BackedEntryMutHandle: BackedEntryRead + BackedEntryWrite {
    type MutHandleResult<'a>
    where
        Self: 'a;

    /// Returns a handle to allow efficient in-memory modifications.
    ///
    /// Make sure to call `flush` to sync with disk before dropping. Drops may
    /// otherwise panic if `flush` fails.
    fn mut_handle(&mut self) -> Self::MutHandleResult<'_>;
}
