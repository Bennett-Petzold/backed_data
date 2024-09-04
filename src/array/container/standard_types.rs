/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use crate::utils::{IndirectMut, IndirectRef, ToMut, ToRef};

use super::{Container, MutIter, RefIter, ResizingContainer};

impl<T> RefIter<T> for &[T] {
    type IterRef<'a> = ToRef<'a, T> where Self: 'a;
    fn ref_iter(&self) -> impl Iterator<Item = Self::IterRef<'_>> {
        self[..].iter().map(|v| ToRef(v))
    }
}

impl<T> RefIter<T> for &mut [T] {
    type IterRef<'a> = ToRef<'a, T> where Self: 'a;
    fn ref_iter(&self) -> impl Iterator<Item = Self::IterRef<'_>> {
        self[..].iter().map(|v| ToRef(v))
    }
}

impl<T> MutIter<T> for &mut [T] {
    type IterMut<'a> = ToMut<'a, T> where Self: 'a;
    fn mut_iter(&mut self) -> impl Iterator<Item = Self::IterMut<'_>> {
        self.iter_mut().map(|v| ToMut(v))
    }
}

/// ------------------------------------------------------------------------ ///

impl<T> RefIter<T> for Box<[T]> {
    type IterRef<'a> = ToRef<'a, T> where T: 'a;
    fn ref_iter(&self) -> impl Iterator<Item = Self::IterRef<'_>> {
        self.iter().map(|v| ToRef(v))
    }
}

impl<T> MutIter<T> for Box<[T]> {
    type IterMut<'a> = ToMut<'a, T> where T: 'a;
    fn mut_iter(&mut self) -> impl Iterator<Item = Self::IterMut<'_>> {
        self.iter_mut().map(|v| ToMut(v))
    }
}

impl<T> Container for Box<[T]> {
    type Data = T;
    type Ref<'b> = ToRef<'b, Self::Data> where Self: 'b;
    type Mut<'b> = ToMut<'b, Self::Data> where Self: 'b;
    type RefSlice<'b> = IndirectRef<'b, [Self::Data]> where Self: 'b;
    type MutSlice<'b> = IndirectMut<'b, [Self::Data]> where Self: 'b;

    fn get(&self, index: usize) -> Option<Self::Ref<'_>> {
        <[T]>::get(self, index).map(|v| ToRef(v))
    }
    fn get_mut(&mut self, index: usize) -> Option<Self::Mut<'_>> {
        <[T]>::get_mut(self, index).map(|v| ToMut(v))
    }
    fn len(&self) -> usize {
        <[T]>::len(self)
    }
    fn c_ref(&self) -> Self::RefSlice<'_> {
        IndirectRef(&self[..])
    }
    fn c_mut(&mut self) -> Self::MutSlice<'_> {
        IndirectMut(&mut self[..])
    }
}

/// ------------------------------------------------------------------------ ///

impl<T> RefIter<T> for Vec<T> {
    type IterRef<'a> = ToRef<'a, T> where T: 'a;
    fn ref_iter(&self) -> impl Iterator<Item = Self::IterRef<'_>> {
        self.iter().map(|v| ToRef(v))
    }
}

impl<T> MutIter<T> for Vec<T> {
    type IterMut<'a> = ToMut<'a, T> where T: 'a;
    fn mut_iter(&mut self) -> impl Iterator<Item = Self::IterMut<'_>> {
        self.iter_mut().map(|v| ToMut(v))
    }
}

impl<T> Container for Vec<T> {
    type Data = T;
    type Ref<'b> = ToRef<'b, Self::Data> where Self: 'b;
    type Mut<'b> = ToMut<'b, Self::Data> where Self: 'b;
    type RefSlice<'b> = IndirectRef<'b, [Self::Data]> where Self: 'b;
    type MutSlice<'b> = IndirectMut<'b, [Self::Data]> where Self: 'b;

    fn get(&self, index: usize) -> Option<Self::Ref<'_>> {
        <[T]>::get(self, index).map(|v| ToRef(v))
    }
    fn get_mut(&mut self, index: usize) -> Option<Self::Mut<'_>> {
        <[T]>::get_mut(self, index).map(|v| ToMut(v))
    }
    fn len(&self) -> usize {
        <[T]>::len(self)
    }
    fn c_ref(&self) -> Self::RefSlice<'_> {
        IndirectRef(&self[..])
    }
    fn c_mut(&mut self) -> Self::MutSlice<'_> {
        IndirectMut(&mut self[..])
    }
}

impl<T> ResizingContainer for Vec<T> {
    fn push(&mut self, value: Self::Data) {
        self.push(value)
    }
    fn remove(&mut self, index: usize) {
        self.remove(index);
    }
    fn append(&mut self, other: &mut Self) {
        self.append(other)
    }
}
