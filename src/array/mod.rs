//#[cfg(feature = "async")]
//pub mod async_impl;
pub mod sync_impl;

use std::{
    borrow::{Borrow, BorrowMut},
    cell::{Ref, RefMut},
    ops::{Deref, Index, IndexMut, Range},
    slice::{Iter, IterMut, SliceIndex},
    sync::{Mutex, MutexGuard},
};

use derive_getters::Getters;

#[cfg(feature = "encrypted")]
use secrets::traits::Bytes;
use serde::{Deserialize, Serialize};

use crate::{
    entry::{
        disks::{ReadDisk, WriteDisk},
        formats::{Decoder, Encoder},
        BackedEntry, BackedEntryTrait,
    },
    utils::{Once, ToMut, ToRef},
};

#[derive(Debug)]
pub enum BackedArrayError<T> {
    OutsideEntryBounds(usize),
    Coder(T),
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone)]
struct ArrayLoc {
    pub entry_idx: usize,
    pub inside_entry_idx: usize,
}

#[derive(Debug, Getters)]
pub struct BackedArrayEntry<'a, T> {
    range: &'a Range<usize>,
    entry: &'a T,
}

impl<'a, T> From<(&'a Range<usize>, &'a T)> for BackedArrayEntry<'a, T> {
    fn from(value: (&'a Range<usize>, &'a T)) -> Self {
        Self {
            range: value.0,
            entry: value.1,
        }
    }
}

fn internal_idx(keys: &[Range<usize>], idx: usize) -> Option<ArrayLoc> {
    keys.iter()
        .enumerate()
        .find(|(_, key_range)| key_range.contains(&idx))
        .map(|(entry_idx, key_range)| ArrayLoc {
            entry_idx,
            inside_entry_idx: (idx - key_range.start),
        })
}

/// Returns the entry location for multiple accesses.
///
/// Silently ignores invalid idx values, shortening the return iterator.
fn multiple_internal_idx<'a, I>(
    keys: &'a [Range<usize>],
    idxs: I,
) -> impl Iterator<Item = ArrayLoc> + 'a
where
    I: IntoIterator<Item = usize> + 'a,
{
    idxs.into_iter().flat_map(|idx| internal_idx(keys, idx))
}

/// [`Self::multiple_internal_idx`], but returns None for invalid idx
fn multiple_internal_idx_strict<'a, I>(
    keys: &'a [Range<usize>],
    idxs: I,
) -> impl Iterator<Item = Option<ArrayLoc>> + 'a
where
    I: IntoIterator<Item = usize> + 'a,
{
    idxs.into_iter().map(|idx| internal_idx(keys, idx))
}

pub trait RefIter<T> {
    fn ref_iter(&self) -> impl Iterator<Item: AsRef<T>>;
}

pub trait Container: AsRef<[Self::Data]> + AsMut<[Self::Data]> + RefIter<Self::Data> {
    type Data;
    type Ref<'a>: AsRef<Self::Data> + 'a
    where
        Self: 'a;
    type Mut<'a>: AsMut<Self::Data> + 'a
    where
        Self: 'a;

    fn c_get<'a>(&'a self, index: usize) -> Option<Self::Ref<'a>>;
    fn c_get_mut<'a>(&'a mut self, index: usize) -> Option<Self::Mut<'a>>;
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
    type Container;
    type Disk;
    type Coder;

    fn get_ref(&self) -> &BackedEntry<Self::Container, Self::Disk, Self::Coder>;
    fn get_mut(&mut self) -> &mut BackedEntry<Self::Container, Self::Disk, Self::Coder>;
    fn get(self) -> BackedEntry<Self::Container, Self::Disk, Self::Coder>;
}

/// A [`BackedEntryContainer`] inside a [`Container`].
///
/// For internal use, reduces size of generics boilerplate.
pub trait BackedEntryContainerNested<'a>:
    Container<
    Data: BackedEntryContainer<
        Container = Self::OnceWrapper,
        Disk = Self::Disk,
        Coder = Self::Coder,
    > + From<BackedEntry<Self::OnceWrapper, Self::Disk, Self::Coder>>
              + BackedEntryContainer<
        Container: Once<Inner: Container<Ref<'a> = Self::DataRef<'a>, Mut<'a> = Self::DataMut<'a>>>,
    >,
>
where
    <Self as BackedEntryContainerNested<'a>>::Unwrapped: 'a,
{
    type InnerData;
    type DataRef<'b>: AsRef<Self::InnerData>
    where
        Self::Unwrapped: 'b;
    type DataMut<'b>: AsMut<Self::InnerData>
    where
        Self::Unwrapped: 'b;
    type OnceWrapper: Once<Inner = Self::Unwrapped>;
    type Unwrapped: Container<Data = Self::InnerData>;
    type Disk;
    type Coder;
}

/// Auto-implement trait to wrap intended generics.
impl<'a, T> BackedEntryContainerNested<'a> for T
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
    <<<T as Container>::Data as BackedEntryContainer>::Container as Once>::Inner: 'a,
{
    type InnerData =
        <<<T::Data as BackedEntryContainer>::Container as Once>::Inner as Container>::Data;
    type DataRef<'b> =
        <<<T::Data as BackedEntryContainer>::Container as Once>::Inner as Container>::Ref<'b>
        where <<<T as Container>::Data as BackedEntryContainer>::Container as Once>::Inner: 'b;
    type DataMut<'b> =
        <<<T::Data as BackedEntryContainer>::Container as Once>::Inner as Container>::Mut<'b>
        where <<<T as Container>::Data as BackedEntryContainer>::Container as Once>::Inner: 'b;
    type OnceWrapper = <T::Data as BackedEntryContainer>::Container;
    type Unwrapped = <<T::Data as BackedEntryContainer>::Container as Once>::Inner;
    type Disk = <T::Data as BackedEntryContainer>::Disk;
    type Coder = <T::Data as BackedEntryContainer>::Coder;
}

/// [`BackedEntryContainerNested`] variant.
///
/// For internal use, reduces size of generics boilerplate.
pub trait BackedEntryContainerNestedRead<'a>:
    BackedEntryContainerNested<
    'a,
    Unwrapped: for<'de> Deserialize<'de>,
    Disk: ReadDisk,
    Coder: Decoder<<Self::Disk as ReadDisk>::ReadDisk, Error = Self::ReadError>,
>
{
    type ReadError;
}

impl<'a, T> BackedEntryContainerNestedRead<'a> for T
where
    T: BackedEntryContainerNested<
        'a,
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
pub trait BackedEntryContainerNestedWrite<'a>:
    BackedEntryContainerNested<
    'a,
    Unwrapped: Serialize,
    Disk: WriteDisk,
    Coder: Encoder<<Self::Disk as WriteDisk>::WriteDisk, Error = Self::WriteError>,
>
{
    type WriteError;
}

impl<'a, T> BackedEntryContainerNestedWrite<'a> for T
where
    T: BackedEntryContainerNested<
        'a,
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
pub trait BackedEntryContainerNestedAll<'a>:
    BackedEntryContainerNestedRead<'a> + BackedEntryContainerNestedWrite<'a>
{
}

impl<'a, T> BackedEntryContainerNestedAll<'a> for T where
    T: BackedEntryContainerNestedRead<'a> + BackedEntryContainerNestedWrite<'a>
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
    type Ref<'a> = ToRef<'a, Self::Data> where Self::Data: 'a;
    type Mut<'a> = ToMut<'a, Self::Data> where Self::Data: 'a;

    fn c_get<'a>(&'a self, index: usize) -> Option<Self::Ref<'a>> {
        self.get(index).map(|v| ToRef(v))
    }
    fn c_get_mut<'a>(&'a mut self, index: usize) -> Option<Self::Mut<'a>> {
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
    type Ref<'a> = ToRef<'a, Self::Data> where Self::Data: 'a;
    type Mut<'a> = ToMut<'a, Self::Data> where Self::Data: 'a;

    fn c_get<'a>(&'a self, index: usize) -> Option<Self::Ref<'a>> {
        self.get(index).map(|v| ToRef(v))
    }
    fn c_get_mut<'a>(&'a mut self, index: usize) -> Option<Self::Mut<'a>> {
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
