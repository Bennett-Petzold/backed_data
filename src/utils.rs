/// Utility traits and types.
///
/// If you're looking for the `InternalUse` trait, it is only documented
/// privately. Use `--document-private-items` when building docs to see it.
use std::{
    cell::{OnceCell, UnsafeCell},
    marker::PhantomData,
    ops::{Deref, DerefMut},
    sync::{Mutex, MutexGuard, OnceLock},
};

/// Used to constrain a trait to internal use only.
///
/// `Internal` marks the only types the trait is implemented for.
/// The crate provides an implementation for all `Internal`, which prevents
/// any extra implementors outside the crate.
pub(crate) trait InternalUse {
    type Internal;
}
impl<T> InternalUse for T {
    type Internal = T;
}

/// Wrapper that allows &T to implement [`AsRef<T>`].
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ToRef<'a, T>(pub &'a T);

impl<T> AsRef<T> for ToRef<'_, T> {
    fn as_ref(&self) -> &T {
        self.0
    }
}

impl<T> Deref for ToRef<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<'a, T> PartialEq<&'a T> for ToRef<'a, T>
where
    &'a T: PartialEq<&'a T>,
{
    fn eq(&self, other: &&'a T) -> bool {
        self.0 == other
    }
}

impl<'a, T> PartialOrd<&'a T> for ToRef<'a, T>
where
    &'a T: PartialOrd<&'a T>,
{
    fn partial_cmp(&self, other: &&'a T) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(other)
    }
}

/// Wrapper that allows &mut T to implement [`AsMut<T>`] and [`AsRef<T>`].
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ToMut<'a, T>(pub &'a mut T);

impl<T> AsRef<T> for ToMut<'_, T> {
    fn as_ref(&self) -> &T {
        self.0
    }
}

impl<T> AsMut<T> for ToMut<'_, T> {
    fn as_mut(&mut self) -> &mut T {
        self.0
    }
}

impl<T> Deref for ToMut<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<T> DerefMut for ToMut<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
    }
}

impl<'a, T> PartialEq<&'a T> for ToMut<'a, T>
where
    &'a mut T: PartialEq<&'a T>,
{
    fn eq(&self, other: &&'a T) -> bool {
        self.0 == other
    }
}

impl<'a, T> PartialOrd<&'a T> for ToMut<'a, T>
where
    &'a mut T: PartialOrd<&'a T>,
{
    fn partial_cmp(&self, other: &&'a T) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(other)
    }
}

/// Combined trait for types with both [`AsRef`] and [`AsMut`].
pub trait AsRefMut<T>: AsRef<T> + AsMut<T> {}
impl<T, U> AsRefMut<T> for U where U: AsRef<T> + AsMut<T> {}

/// Unity trait for [`OnceCell`] and [`OnceLock`].
pub trait Once {
    type Inner;
    fn new() -> Self;
    fn get(&self) -> Option<&Self::Inner>;
    fn get_mut(&mut self) -> Option<&mut Self::Inner>;
    fn set(&self, value: Self::Inner) -> Result<(), Self::Inner>;
    fn get_or_init<F>(&self, f: F) -> &Self::Inner
    where
        F: FnOnce() -> Self::Inner;
    fn into_inner(self) -> Option<Self::Inner>;
    fn take(&mut self) -> Option<Self::Inner>;
}

impl<T> Once for OnceCell<T> {
    type Inner = T;
    fn new() -> Self {
        Self::new()
    }
    fn get(&self) -> Option<&Self::Inner> {
        self.get()
    }
    fn get_mut(&mut self) -> Option<&mut Self::Inner> {
        self.get_mut()
    }
    fn set(&self, value: Self::Inner) -> Result<(), Self::Inner> {
        self.set(value)
    }
    fn get_or_init<F>(&self, f: F) -> &Self::Inner
    where
        F: FnOnce() -> Self::Inner,
    {
        self.get_or_init(f)
    }
    fn into_inner(self) -> Option<Self::Inner> {
        self.into_inner()
    }
    fn take(&mut self) -> Option<Self::Inner> {
        self.take()
    }
}

impl<T> Once for OnceLock<T> {
    type Inner = T;
    fn new() -> Self {
        Self::new()
    }
    fn get(&self) -> Option<&Self::Inner> {
        self.get()
    }
    fn get_mut(&mut self) -> Option<&mut Self::Inner> {
        self.get_mut()
    }
    fn set(&self, value: Self::Inner) -> Result<(), Self::Inner> {
        self.set(value)
    }
    fn get_or_init<F>(&self, f: F) -> &Self::Inner
    where
        F: FnOnce() -> Self::Inner,
    {
        self.get_or_init(f)
    }
    fn into_inner(self) -> Option<Self::Inner> {
        self.into_inner()
    }
    fn take(&mut self) -> Option<Self::Inner> {
        self.take()
    }
}

#[cfg(feature = "async")]
impl<T> Once for tokio::sync::OnceCell<T> {
    type Inner = T;
    fn new() -> Self {
        Self::new()
    }
    fn get(&self) -> Option<&Self::Inner> {
        self.get()
    }
    fn get_mut(&mut self) -> Option<&mut Self::Inner> {
        self.get_mut()
    }
    fn set(&self, value: Self::Inner) -> Result<(), Self::Inner> {
        if self.initialized() {
            Err(value)
        } else {
            let _ = self.set(value);
            Ok(())
        }
    }
    fn get_or_init<F>(&self, f: F) -> &Self::Inner
    where
        F: FnOnce() -> Self::Inner,
    {
        let _ = self.set((f)());
        self.get().unwrap()
    }
    fn into_inner(self) -> Option<Self::Inner> {
        self.into_inner()
    }
    fn take(&mut self) -> Option<Self::Inner> {
        self.take()
    }
}

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

/// Takes ownership of a parent value and computes the child.
///
/// Used to prevent the parent from being freed, and therefore create a child
/// from its reference with the same lifetime. Unsafe
#[derive(Debug)]
pub struct BorrowExtender<T, U> {
    _parent: T,
    child: U,
}

impl<T, U> BorrowExtender<T, U> {
    pub fn new<V: FnOnce(&T) -> U>(parent: T, child: V) -> Self {
        let parent_ptr: *const _ = &parent;
        let child = (child)(unsafe { &*parent_ptr });
        Self {
            _parent: parent,
            child,
        }
    }

    pub fn try_new<'a, W, V: FnOnce(&'a T) -> Result<U, W>>(parent: T, child: V) -> Result<Self, W>
    where
        T: 'a,
    {
        let parent_ptr: *const _ = &parent;
        let child = (child)(unsafe { &*parent_ptr })?;
        Ok(Self {
            _parent: parent,
            child,
        })
    }

    pub fn maybe_new<'a, V: FnOnce(&'a T) -> Option<U>>(parent: T, child: V) -> Option<Self>
    where
        T: 'a,
    {
        let parent_ptr: *const _ = &parent;
        let child = (child)(unsafe { &*parent_ptr })?;
        Some(Self {
            _parent: parent,
            child,
        })
    }
}

impl<T, U> Deref for BorrowExtender<T, U> {
    type Target = U;
    fn deref(&self) -> &Self::Target {
        &self.child
    }
}
