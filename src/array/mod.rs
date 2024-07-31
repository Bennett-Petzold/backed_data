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
use serde::Deserialize;

use crate::{
    entry::BackedEntry,
    utils::{ToMut, ToRef},
};

#[derive(Debug)]
pub enum BackedArrayError {
    OutsideEntryBounds(usize),
    Bincode(bincode::Error),
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

    fn c_get(&self, index: usize) -> Option<impl AsRef<Self::Data>>;
    fn c_get_mut(&mut self, index: usize) -> Option<impl AsMut<Self::Data>>;
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
    type Disk: for<'de> Deserialize<'de>;
    fn get_ref(&self) -> &BackedEntry<Self::Container, Self::Disk>;
    fn get_mut(&mut self) -> &mut BackedEntry<Self::Container, Self::Disk>;
    fn get(self) -> BackedEntry<Self::Container, Self::Disk>;
}

#[derive(Debug)]
pub struct ContDiskPair<C, D> {
    container: C,
    disk: D,
}

impl<C: Container, D: for<'de> Deserialize<'de>> BackedEntryContainer for BackedEntry<C, D> {
    type Container = C;
    type Disk = D;
    fn get(self) -> BackedEntry<Self::Container, Self::Disk> {
        self
    }
    fn get_ref(&self) -> &BackedEntry<Self::Container, Self::Disk> {
        self
    }
    fn get_mut(&mut self) -> &mut BackedEntry<Self::Container, Self::Disk> {
        self
    }
}

pub trait BackedEntryContainerStub {}

impl<T> RefIter<T> for Box<[T]> {
    fn ref_iter(&self) -> impl Iterator<Item: AsRef<T>> {
        self.iter().map(|v| ToRef(v))
    }
}

impl<T> Container for Box<[T]> {
    type Data = T;

    fn c_get(&self, index: usize) -> Option<impl AsRef<Self::Data>> {
        self.get(index).map(|v| ToRef(v))
    }
    fn c_get_mut(&mut self, index: usize) -> Option<impl AsMut<Self::Data>> {
        self.get_mut(index).map(|v| ToMut(v))
    }
}

impl<T> RefIter<T> for Vec<T> {
    fn ref_iter(&self) -> impl Iterator<Item: AsRef<T>> {
        self.iter().map(|v| ToRef(v))
    }
}

impl<T> Container for Vec<T> {
    type Data = T;

    fn c_get(&self, index: usize) -> Option<impl AsRef<Self::Data>> {
        self.get(index).map(|v| ToRef(v))
    }
    fn c_get_mut(&mut self, index: usize) -> Option<impl AsMut<Self::Data>> {
        self.get_mut(index).map(|v| ToMut(v))
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
