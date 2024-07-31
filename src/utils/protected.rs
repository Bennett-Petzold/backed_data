use std::{
    cell::UnsafeCell,
    sync::{Mutex, MutexGuard},
};

use super::{ToMut, ToRef};

#[derive(Debug)]
pub struct GuardToRef<'a, T>(MutexGuard<'a, T>);

impl<T> AsRef<T> for GuardToRef<'_, T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

#[derive(Debug)]
pub struct GuardToMut<'a, T>(MutexGuard<'a, T>);

impl<T> AsMut<T> for GuardToMut<'_, T> {
    fn as_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

/// Trait to combine Mutex (allowing panics) and UnsafeCell in one generic.
///
/// Mutex provides the thread-safe access pattern, UnsafeCell optimizes for
/// single-threaded performance.
pub trait ProtectedAccess {
    type Val;
    /// Mutable reference return from an immutable context.
    ///
    /// Intended for structs where mutation is used to set a single value and
    /// hold that value across multiple mutable calls.
    ///
    /// # Safety
    /// This should only be used to update values in a way that does not
    /// invalidate any other live references. Borrow self mutably before any
    /// invalidations, to make sure no references exist.
    unsafe fn get_mut_unsafe(&self) -> impl AsMut<Self::Val>;
    /// Immutable reference return.
    ///
    /// # Safety
    /// Requires [`Self::get_mut_unsafe`] is used properly to be valid.
    unsafe fn get_unsafe(&self) -> impl AsRef<Self::Val>;
    /// Regular mutable reference return.
    /// # Safety
    /// Requires [`Self::get_mut_unsafe`] is used properly to be valid.
    fn get_mut(&mut self) -> impl AsMut<Self::Val>;
    fn inner(self) -> Self::Val;
    fn wrap(val: Self::Val) -> Self;
}

impl<T> ProtectedAccess for Mutex<T> {
    type Val = T;
    unsafe fn get_mut_unsafe(&self) -> impl AsMut<Self::Val> {
        GuardToMut(self.lock().unwrap())
    }
    unsafe fn get_unsafe(&self) -> impl AsRef<Self::Val> {
        GuardToRef(self.lock().unwrap())
    }
    fn get_mut(&mut self) -> impl AsMut<Self::Val> {
        ToMut(self.get_mut().unwrap())
    }
    fn inner(self) -> Self::Val {
        self.into_inner().unwrap()
    }
    fn wrap(val: Self::Val) -> Self {
        Self::new(val)
    }
}

impl<T> ProtectedAccess for UnsafeCell<T> {
    type Val = T;
    unsafe fn get_mut_unsafe(&self) -> impl AsMut<Self::Val> {
        ToMut(&mut *self.get())
    }
    unsafe fn get_unsafe(&self) -> impl AsRef<Self::Val> {
        ToRef(&*self.get())
    }
    fn get_mut(&mut self) -> impl AsMut<Self::Val> {
        ToMut(self.get_mut())
    }
    fn inner(self) -> Self::Val {
        self.into_inner()
    }
    fn wrap(val: Self::Val) -> Self {
        Self::new(val)
    }
}
