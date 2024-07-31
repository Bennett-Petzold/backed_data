use serde::{Deserialize, Serialize};

use crate::{
    entry::{
        disks::{ReadDisk, WriteDisk},
        formats::{Decoder, Encoder},
        BackedEntry,
    },
    utils::{Once, ToMut, ToRef},
};

pub trait RefIter<T> {
    fn ref_iter(&self) -> impl Iterator<Item: AsRef<T>>;
}

pub trait Container: AsRef<[Self::Data]> + AsMut<[Self::Data]> + RefIter<Self::Data> {
    type Data;
    type Ref<'b>: AsRef<Self::Data>
    where
        Self: 'b;
    type Mut<'b>: AsMut<Self::Data>
    where
        Self: 'b;

    fn c_get(&self, index: usize) -> Option<Self::Ref<'_>>;
    fn c_get_mut(&mut self, index: usize) -> Option<Self::Mut<'_>>;
    fn c_len(&self) -> usize;
}

pub trait ResizingContainer:
    Container
    + Default
    + FromIterator<Self::Data>
    + IntoIterator<Item = Self::Data>
    + Extend<Self::Data>
{
    fn c_push(&mut self, value: Self::Data);
    fn c_remove(&mut self, index: usize);
    fn c_append(&mut self, other: &mut Self);
}

pub trait BackedEntryContainer {
    type Container: Once<Inner: Container>;
    type Disk;
    type Coder;

    fn get_ref(&self) -> &BackedEntry<Self::Container, Self::Disk, Self::Coder>;
    fn get_mut(&mut self) -> &mut BackedEntry<Self::Container, Self::Disk, Self::Coder>;
    fn get(self) -> BackedEntry<Self::Container, Self::Disk, Self::Coder>;
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
    T: Container<Data: BackedEntryContainer>,
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
    Coder: Decoder<<Self::Disk as ReadDisk>::ReadDisk, Error = Self::ReadError>,
>
{
    type ReadError;
}

impl<T> BackedEntryContainerNestedRead for T
where
    T: BackedEntryContainerNested<
        Unwrapped: for<'de> Deserialize<'de>,
        Disk: ReadDisk,
        Coder: Decoder<<Self::Disk as ReadDisk>::ReadDisk>,
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
    Coder: Encoder<<Self::Disk as WriteDisk>::WriteDisk, Error = Self::WriteError>,
>
{
    type WriteError;
}

impl<T> BackedEntryContainerNestedWrite for T
where
    T: BackedEntryContainerNested<
        Unwrapped: Serialize,
        Disk: WriteDisk,
        Coder: Encoder<<Self::Disk as WriteDisk>::WriteDisk>,
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

/// [`BackedEntry`] that holds a [`Container`] type.
impl<C, D, Enc> BackedEntryContainer for BackedEntry<C, D, Enc>
where
    C: Once<Inner: Container>,
{
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

/// Mutable open for a reference to a [`BackedEntryContainer`].
macro_rules! open_mut {
    ($x:expr) => {
        $x.as_mut().get_mut()
    };
}
pub(crate) use open_mut;

/// Immutable open for a reference to a [`BackedEntryContainer`].
macro_rules! open_ref {
    ($x:expr) => {
        $x.as_ref().get_ref()
    };
}
pub(crate) use open_ref;

impl<T> RefIter<T> for Box<[T]> {
    fn ref_iter(&self) -> impl Iterator<Item: AsRef<T>> {
        self.iter().map(|v| ToRef(v))
    }
}

impl<T> Container for Box<[T]> {
    type Data = T;
    type Ref<'b> = ToRef<'b, Self::Data> where Self: 'b;
    type Mut<'b> = ToMut<'b, Self::Data> where Self: 'b;

    fn c_get(&self, index: usize) -> Option<Self::Ref<'_>> {
        self.get(index).map(|v| ToRef(v))
    }
    fn c_get_mut(&mut self, index: usize) -> Option<Self::Mut<'_>> {
        self.get_mut(index).map(|v| ToMut(v))
    }
    fn c_len(&self) -> usize {
        self.len()
    }
}

impl<T> RefIter<T> for Vec<T> {
    fn ref_iter(&self) -> impl Iterator<Item: AsRef<T>> {
        self.iter().map(|v| ToRef(v))
    }
}

impl<T> Container for Vec<T> {
    type Data = T;
    type Ref<'b> = ToRef<'b, Self::Data> where Self: 'b;
    type Mut<'b> = ToMut<'b, Self::Data> where Self: 'b;

    fn c_get(&self, index: usize) -> Option<Self::Ref<'_>> {
        self.get(index).map(|v| ToRef(v))
    }
    fn c_get_mut(&mut self, index: usize) -> Option<Self::Mut<'_>> {
        self.get_mut(index).map(|v| ToMut(v))
    }
    fn c_len(&self) -> usize {
        self.len()
    }
}

impl<T> ResizingContainer for Vec<T> {
    fn c_push(&mut self, value: Self::Data) {
        self.push(value)
    }
    fn c_remove(&mut self, index: usize) {
        self.remove(index);
    }
    fn c_append(&mut self, other: &mut Self) {
        self.append(other)
    }
}

/*
#[cfg(feature = "encrypted")]
#[derive(Debug)]
pub struct SecretVec<T: Bytes> {
    inner: secrets::SecretVec<T>,
}

#[cfg(feature = "encrypted")]
impl<T: Bytes> Default for SecretVec<T> {
    fn default() -> Self {
        Self {
            inner: secrets::SecretVec::<T>::zero(0),
        }
    }
}

impl<T: Bytes> Container for SecretVec<T> {
    fn c_push(&mut self, value: Self::Data) {
        todo!()
    }
    fn c_remove(&mut self, index: usize) -> Self::Data {
        todo!()
    }
    fn c_append(&mut self, other: &mut Self) {
        todo!()
    }
}
*/
