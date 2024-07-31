use crate::utils::{IndirectMut, IndirectRef, ToMut, ToRef};

use super::{Container, MutIter, RefIter, ResizingContainer};

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

    fn c_get(&self, index: usize) -> Option<Self::Ref<'_>> {
        self.get(index).map(|v| ToRef(v))
    }
    fn c_get_mut(&mut self, index: usize) -> Option<Self::Mut<'_>> {
        self.get_mut(index).map(|v| ToMut(v))
    }
    fn c_len(&self) -> usize {
        self.len()
    }
    fn c_ref(&self) -> Self::RefSlice<'_> {
        IndirectRef(&self[..])
    }
    fn c_mut(&mut self) -> Self::MutSlice<'_> {
        IndirectMut(&mut self[..])
    }
}

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

    fn c_get(&self, index: usize) -> Option<Self::Ref<'_>> {
        self.get(index).map(|v| ToRef(v))
    }
    fn c_get_mut(&mut self, index: usize) -> Option<Self::Mut<'_>> {
        self.get_mut(index).map(|v| ToMut(v))
    }
    fn c_len(&self) -> usize {
        self.len()
    }
    fn c_ref(&self) -> Self::RefSlice<'_> {
        IndirectRef(&self[..])
    }
    fn c_mut(&mut self) -> Self::MutSlice<'_> {
        IndirectMut(&mut self[..])
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
