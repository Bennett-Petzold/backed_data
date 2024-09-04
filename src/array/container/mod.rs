/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/*!
Defines generic container types.
*/

use std::ops::{Deref, DerefMut};

use serde::{Deserialize, Serialize};

use crate::{
    entry::{
        disks::{ReadDisk, WriteDisk},
        formats::{Decoder, Encoder},
        BackedEntry,
    },
    utils::Once,
};

#[cfg(feature = "unsafe_array")]
use stable_deref_trait::StableDeref;

/// Requires [`stable_deref_trait::StableDeref`] when `unsafe` code is enabled.
///
/// If unsafe code is not enabled, this implements for everything. This hackery
/// gets around a trap between borrow checker limitations and
/// `forbid(unsafe_code)` limitations.
#[cfg(not(feature = "unsafe_array"))]
pub trait StableDerefFeatureOpt {}

#[cfg(not(feature = "unsafe_array"))]
impl<T: ?Sized> StableDerefFeatureOpt for T {}

/// Requires [`stable_deref_trait::StableDeref`] when `unsafe` code is enabled.
///
/// If unsafe code is not enabled, this implements for everything. This hackery
/// gets around a trap between borrow checker limitations and
/// `forbid(unsafe_code)` limitations.
#[cfg(feature = "unsafe_array")]
pub trait StableDerefFeatureOpt: StableDeref {}

#[cfg(feature = "unsafe_array")]
impl<T> StableDerefFeatureOpt for T where T: StableDeref + ?Sized {}

pub trait RefIter<T> {
    type IterRef<'b>: AsRef<T> + Deref<Target = T> + StableDerefFeatureOpt
    where
        Self: 'b;
    fn ref_iter(&self) -> impl Iterator<Item = Self::IterRef<'_>>;
}

pub trait MutIter<T> {
    type IterMut<'b>: AsMut<T> + DerefMut<Target = T> + StableDerefFeatureOpt
    where
        Self: 'b;
    fn mut_iter(&mut self) -> impl Iterator<Item = Self::IterMut<'_>>;
}

/// Generic wrapper for any container type.
///
/// This is implemented over standard Rust containers ([`Vec`], boxed slices,
/// etc.) in addition to more exotic types.
/// `&[T]` is insufficiently generic for types that return a ref handle to `T`,
/// instead of `&T` directly, so this allows for more complex container types.
///
/// Methods are prepended with `*` to avoid namespace conflicts.
pub trait Container:
    RefIter<Self::Data>
    + MutIter<Self::Data>
    + IntoIterator<Item = Self::Data>
    + FromIterator<Self::Data>
    + Default
{
    /// The data container entries give references to.
    type Data;
    type Ref<'b>: AsRef<Self::Data> + Deref<Target = Self::Data> + StableDerefFeatureOpt
    where
        Self: 'b;
    type Mut<'b>: AsMut<Self::Data> + DerefMut<Target = Self::Data> + StableDerefFeatureOpt
    where
        Self: 'b;
    type RefSlice<'b>: AsRef<[Self::Data]> + Deref<Target = [Self::Data]> + StableDerefFeatureOpt
    where
        Self: 'b;
    type MutSlice<'b>: AsMut<[Self::Data]> + DerefMut<Target = [Self::Data]> + StableDerefFeatureOpt
    where
        Self: 'b;

    fn get(&self, index: usize) -> Option<Self::Ref<'_>>;
    fn get_mut(&mut self, index: usize) -> Option<Self::Mut<'_>>;
    fn len(&self) -> usize;
    fn c_ref(&self) -> Self::RefSlice<'_>;
    fn c_mut(&mut self) -> Self::MutSlice<'_>;
}

/// A [`Container`] that supports resizing operations.
pub trait ResizingContainer:
    Container + Default + FromIterator<Self::Data> + Extend<Self::Data>
{
    fn push(&mut self, value: Self::Data);
    fn remove(&mut self, index: usize);
    fn append(&mut self, other: &mut Self);
}

/*
/// A [`BackedEntry`] holding a valid [`Container`] type.
///
/// For internal implementation, reduces size of generics boilerplate.
pub trait BackedEntryContainer {
    type Container;
    type Disk;
    type Coder;

    fn get_ref(&self) -> &BackedEntry<Self::Container, Self::Disk, Self::Coder>;
    fn get_mut(&mut self) -> &mut BackedEntry<Self::Container, Self::Disk, Self::Coder>;
    fn get(self) -> BackedEntry<Self::Container, Self::Disk, Self::Coder>;
}

impl<C, D, Enc> BackedEntryContainer for BackedEntry<C, D, Enc> {
    type Container = C;
    type Disk = D;
    type Coder = Enc;

    fn get(self) -> BackedEntry<Self::Container, Self::Disk, Self::Coder> {
        self
    }
    fn get_ref(&self) -> &BackedEntry<Self::Container, Self::Disk, Self::Coder> {
        self
    }
    fn get_mut(&mut self) -> &mut BackedEntry<Self::Container, Self::Disk, Self::Coder> {
        self
    }
}

/// A [`BackedEntryContainer`] inside a [`Container`].
///
/// For internal implementation, reduces size of generics boilerplate.
pub trait BackedEntryContainerNested:
    Container<
    Data: BackedEntryContainer<
        Container = Self::OnceWrapper,
        Disk = Self::Disk,
        Coder = Self::Coder,
    > + From<BackedEntry<Self::OnceWrapper, Self::Disk, Self::Coder>>,
>
{
    type InnerData;
    type OnceWrapper: Once<Inner = Self::Unwrapped>;
    type Unwrapped: Container<Data = Self::InnerData>;
    type Disk;
    type Coder;
}

/// Auto-implement trait to wrap intended generics.
impl<T> BackedEntryContainerNested for T
where
    for<'a> T: Container<Data: BackedEntryContainer>,
    <<T as Container>::Data as BackedEntryContainer>::Container: Once<Inner: Container>,
    <T as Container>::Data: From<
        BackedEntry<
            <T::Data as BackedEntryContainer>::Container,
            <T::Data as BackedEntryContainer>::Disk,
            <T::Data as BackedEntryContainer>::Coder,
        >,
    >,
{
    type InnerData =
        <<<T::Data as BackedEntryContainer>::Container as Once>::Inner as Container>::Data;
    type OnceWrapper = <T::Data as BackedEntryContainer>::Container;
    type Unwrapped = <<T::Data as BackedEntryContainer>::Container as Once>::Inner;
    type Disk = <T::Data as BackedEntryContainer>::Disk;
    type Coder = <T::Data as BackedEntryContainer>::Coder;
}

/// [`BackedEntryContainerNested`] variant that has reading.
///
/// For internal implementation, reduces size of generics boilerplate.
pub trait BackedEntryContainerNestedRead:
    BackedEntryContainerNested<
    Unwrapped: for<'de> Deserialize<'de>,
    Disk: ReadDisk,
    Coder: Decoder<
        <Self::Disk as ReadDisk>::ReadDisk,
        T = Self::Unwrapped,
        Error = Self::ReadError,
    >,
>
{
    type ReadError;
}

impl<T> BackedEntryContainerNestedRead for T
where
    T: BackedEntryContainerNested<
        Unwrapped: for<'de> Deserialize<'de>,
        Disk: ReadDisk,
        Coder: Decoder<<Self::Disk as ReadDisk>::ReadDisk, T = Self::Unwrapped>,
    >,
{
    type ReadError = <Self::Coder as Decoder<<Self::Disk as ReadDisk>::ReadDisk>>::Error;
}

/// [`BackedEntryContainerNested`] variant that has writing.
///
/// For internal implementation, reduces size of generics boilerplate.
pub trait BackedEntryContainerNestedWrite:
    BackedEntryContainerNested<
    Unwrapped: Serialize,
    Disk: WriteDisk,
    Coder: Encoder<
        <Self::Disk as WriteDisk>::WriteDisk,
        T = Self::Unwrapped,
        Error = Self::WriteError,
    >,
>
{
    type WriteError;
}

impl<T> BackedEntryContainerNestedWrite for T
where
    T: BackedEntryContainerNested<
        Unwrapped: Serialize,
        Disk: WriteDisk,
        Coder: Encoder<<Self::Disk as WriteDisk>::WriteDisk, T = Self::Unwrapped>,
    >,
{
    type WriteError = <Self::Coder as Encoder<<Self::Disk as WriteDisk>::WriteDisk>>::Error;
}

/// [`BackedEntryContainerNested`] variant that has writing and reading.
///
/// For internal implementation, reduces size of generics boilerplate.
pub trait BackedEntryContainerNestedAll:
    BackedEntryContainerNestedRead + BackedEntryContainerNestedWrite
{
}

impl<T> BackedEntryContainerNestedAll for T where
    T: BackedEntryContainerNestedRead + BackedEntryContainerNestedWrite
{
}

/// Mutable open for a reference to a [`BackedEntryContainer`].
macro_rules! open_mut {
    ($x:expr) => {
        $x.as_mut().get_mut()
    };
}
pub(crate) use open_mut;
*/

mod standard_types;
#[allow(unused_imports)]
pub use standard_types::*;

#[cfg(feature = "encrypted")]
mod encrypted;
#[cfg(feature = "encrypted")]
pub use encrypted::*;
