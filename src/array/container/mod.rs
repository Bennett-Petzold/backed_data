use std::ops::{Deref, DerefMut};

use serde::{Deserialize, Serialize};
use stable_deref_trait::StableDeref;

use crate::{
    entry::{
        disks::{ReadDisk, WriteDisk},
        formats::{Decoder, Encoder},
        BackedEntry,
    },
    utils::Once,
};

pub trait RefIter<T> {
    type IterRef<'b>: AsRef<T> + Deref<Target = T> + StableDeref
    where
        Self: 'b;
    fn ref_iter(&self) -> impl Iterator<Item = Self::IterRef<'_>>;
}

pub trait MutIter<T> {
    type IterMut<'b>: AsMut<T> + DerefMut<Target = T> + StableDeref
    where
        Self: 'b;
    fn mut_iter(&mut self) -> impl Iterator<Item = Self::IterMut<'_>>;
}

/// Generic wrapper for any container type.
///
/// Methods are prepended with `c_*` to avoid namespace conflicts.
///
/// `&[T]` is insufficiently generic for types that return a ref handle to `T`,
/// instead of `&T` directly, so this allows for more complex container types.
pub trait Container: RefIter<Self::Data> + MutIter<Self::Data> {
    /// The data container entries give references to.
    type Data;
    type Ref<'b>: AsRef<Self::Data> + Deref<Target = Self::Data> + StableDeref
    where
        Self: 'b;
    type Mut<'b>: AsMut<Self::Data> + DerefMut<Target = Self::Data> + StableDeref
    where
        Self: 'b;
    type RefSlice<'b>: AsRef<[Self::Data]> + Deref<Target = [Self::Data]> + StableDeref
    where
        Self: 'b;
    type MutSlice<'b>: AsMut<[Self::Data]> + DerefMut<Target = [Self::Data]> + StableDeref
    where
        Self: 'b;

    fn c_get(&self, index: usize) -> Option<Self::Ref<'_>>;
    fn c_get_mut(&mut self, index: usize) -> Option<Self::Mut<'_>>;
    fn c_len(&self) -> usize;
    fn c_ref(&self) -> Self::RefSlice<'_>;
    fn c_mut(&mut self) -> Self::MutSlice<'_>;
}

/// A [`Container`] that supports resizing operations.
pub trait ResizingContainer:
    Container + Default + FromIterator<Self::Data> + Extend<Self::Data>
{
    fn c_push(&mut self, value: Self::Data);
    fn c_remove(&mut self, index: usize);
    fn c_append(&mut self, other: &mut Self);
}

/// A [`BackedEntry`] holding a valid [`Container`] type.
///
/// For internal use, reduces size of generics boilerplate.
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
/// For internal use, reduces size of generics boilerplate.
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

/// [`BackedEntryContainerNested`] variant.
///
/// For internal use, reduces size of generics boilerplate.
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

/// [`BackedEntryContainerNested`] variant.
///
/// For internal use, reduces size of generics boilerplate.
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

/// [`BackedEntryContainerNested`] variant.
///
/// For internal use, reduces size of generics boilerplate.
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

mod standard_types;
#[allow(unused_imports)]
pub use standard_types::*;

#[cfg(feature = "encrypted")]
mod encrypted;
#[cfg(feature = "encrypted")]
pub use encrypted::*;
