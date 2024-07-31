/// Utility traits and types.
///
/// If you're looking for the `InternalUse` trait, it is only documented
/// privately. Use `--document-private-items` when building docs to see it.
use std::{
    cell::{OnceCell, UnsafeCell},
    ops::{Deref, DerefMut},
    pin::Pin,
    sync::{Mutex, MutexGuard, OnceLock},
};

#[cfg(feature = "async")]
use futures::Future;

#[cfg(feature = "async")]
pub mod blocking;

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

mod refs;
pub use refs::*;

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
    _parent: Pin<Box<T>>,
    child: U,
}

impl<T, U> BorrowExtender<T, U> {
    pub fn new<V: FnOnce(&T) -> U>(parent: T, child: V) -> Self {
        let parent = Box::pin(parent);
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
        let parent = Box::pin(parent);
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
        let parent = Box::pin(parent);
        let parent_ptr: *const _ = &parent;
        let child = (child)(unsafe { &*parent_ptr })?;
        Some(Self {
            _parent: parent,
            child,
        })
    }

    pub fn new_mut<V: FnOnce(&mut T) -> U>(parent: T, child: V) -> Self {
        let mut parent = Box::pin(parent);
        let parent_ptr: *mut _ = unsafe { parent.as_mut().get_unchecked_mut() };
        let child = (child)(unsafe { &mut *parent_ptr });
        Self {
            _parent: parent,
            child,
        }
    }

    pub fn try_new_mut<'a, W, V: FnOnce(&'a mut T) -> Result<U, W>>(
        parent: T,
        child: V,
    ) -> Result<Self, W>
    where
        T: 'a,
    {
        let mut parent = Box::pin(parent);
        let parent_ptr: *mut _ = unsafe { parent.as_mut().get_unchecked_mut() };
        let child = (child)(unsafe { &mut *parent_ptr })?;
        Ok(Self {
            _parent: parent,
            child,
        })
    }

    pub fn maybe_new_mut<'a, V: FnOnce(&'a mut T) -> Option<U>>(parent: T, child: V) -> Option<Self>
    where
        T: 'a,
    {
        let mut parent = Box::pin(parent);
        let parent_ptr: *mut _ = unsafe { parent.as_mut().get_unchecked_mut() };
        let child = (child)(unsafe { &mut *parent_ptr })?;
        Some(Self {
            _parent: parent,
            child,
        })
    }
}

#[cfg(feature = "async")]
impl<T, U> BorrowExtender<T, U> {
    pub async fn a_new<F: Future<Output = U>, V: FnOnce(&T) -> F>(parent: T, child: V) -> Self {
        let parent = Box::pin(parent);
        let parent_ptr: *const _ = &parent;
        let child = (child)(unsafe { &*parent_ptr }).await;
        Self {
            _parent: parent,
            child,
        }
    }

    pub async fn a_try_new<'a, W, F: Future<Output = Result<U, W>>, V: FnOnce(&'a T) -> F>(
        parent: T,
        child: V,
    ) -> Result<Self, W>
    where
        T: 'a,
    {
        let parent = Box::pin(parent);
        let parent_ptr: *const _ = &parent;
        let child = (child)(unsafe { &*parent_ptr }).await?;
        Ok(Self {
            _parent: parent,
            child,
        })
    }

    pub async fn a_maybe_new<'a, F: Future<Output = Option<U>>, V: FnOnce(&'a T) -> F>(
        parent: T,
        child: V,
    ) -> Option<Self>
    where
        T: 'a,
    {
        let parent = Box::pin(parent);
        let parent_ptr: *const _ = &parent;
        let child = (child)(unsafe { &*parent_ptr }).await?;
        Some(Self {
            _parent: parent,
            child,
        })
    }

    pub async fn a_new_mut<F: Future<Output = U>, V: FnOnce(&mut T) -> F>(
        parent: T,
        child: V,
    ) -> Self {
        let mut parent = Box::pin(parent);
        let parent_ptr: *mut _ = unsafe { parent.as_mut().get_unchecked_mut() };
        let child = (child)(unsafe { &mut *parent_ptr }).await;
        Self {
            _parent: parent,
            child,
        }
    }

    pub async fn a_try_new_mut<'a, W, F: Future<Output = Result<U, W>>, V: FnOnce(&'a mut T) -> F>(
        parent: T,
        child: V,
    ) -> Result<Self, W>
    where
        T: 'a,
    {
        let mut parent = Box::pin(parent);
        let parent_ptr: *mut _ = unsafe { parent.as_mut().get_unchecked_mut() };
        let child = (child)(unsafe { &mut *parent_ptr }).await?;
        Ok(Self {
            _parent: parent,
            child,
        })
    }

    pub async fn a_maybe_new_mut<'a, F: Future<Output = Option<U>>, V: FnOnce(&'a mut T) -> F>(
        parent: T,
        child: V,
    ) -> Option<Self>
    where
        T: 'a,
    {
        let mut parent = Box::pin(parent);
        let parent_ptr: *mut _ = unsafe { parent.as_mut().get_unchecked_mut() };
        let child = (child)(unsafe { &mut *parent_ptr }).await?;
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

impl<T, U> DerefMut for BorrowExtender<T, U> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.child
    }
}

impl<T, U> AsRef<U> for BorrowExtender<T, U> {
    fn as_ref(&self) -> &U {
        &self.child
    }
}

impl<T, U> AsMut<U> for BorrowExtender<T, U> {
    fn as_mut(&mut self) -> &mut U {
        &mut self.child
    }
}

impl<T, V, F> BorrowExtender<T, Result<V, F>> {
    pub fn open_result(self) -> Result<BorrowExtender<T, V>, F> {
        Ok(BorrowExtender::<T, V> {
            _parent: self._parent,
            child: self.child?,
        })
    }
}

impl<T, V> BorrowExtender<T, Option<V>> {
    pub fn open_option(self) -> Option<BorrowExtender<T, V>> {
        Some(BorrowExtender::<T, V> {
            _parent: self._parent,
            child: self.child?,
        })
    }
}
