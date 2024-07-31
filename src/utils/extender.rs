#![cfg(any(feature = "array", feature = "encrypted"))]

use std::{
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

#[cfg(feature = "async")]
use futures::Future;
use stable_deref_trait::StableDeref;

/// Takes ownership of a parent value and computes the child with a reference.
///
/// Used to prevent the parent from being freed, and therefore return a
/// referenced value from a function.
#[derive(Debug)]
pub struct BorrowExtender<T: StableDeref, U> {
    _parent: T,
    child: U,
}

unsafe impl<T: StableDeref, U: StableDeref> StableDeref for BorrowExtender<T, U> {}

pub type BorrowNest<A, B, C> = BorrowExtender<BorrowExtender<A, B>, C>;

impl<T: StableDeref, U> BorrowExtender<T, U> {
    pub fn new<V: FnOnce(&T) -> U>(parent: T, child: V) -> Self {
        let child = (child)(&parent);
        Self {
            _parent: parent,
            child,
        }
    }

    pub fn try_new<W, V: FnOnce(&T) -> Result<U, W>>(parent: T, child: V) -> Result<Self, W> {
        let child = (child)(&parent)?;
        Ok(Self {
            _parent: parent,
            child,
        })
    }

    pub fn maybe_new<V: FnOnce(&T) -> Option<U>>(parent: T, child: V) -> Option<Self> {
        let child = (child)(&parent)?;
        Some(Self {
            _parent: parent,
            child,
        })
    }

    pub fn new_mut<V: FnOnce(NonNull<T>) -> U>(mut parent: T, child: V) -> Self {
        let child = (child)((&mut parent).into());
        BorrowExtender {
            _parent: parent,
            child,
        }
    }

    pub fn try_new_mut<W, V: FnOnce(NonNull<T>) -> Result<U, W>>(
        mut parent: T,
        child: V,
    ) -> Result<Self, W> {
        let child = (child)((&mut parent).into())?;
        Ok(BorrowExtender {
            _parent: parent,
            child,
        })
    }

    pub fn maybe_new_mut<V: FnOnce(NonNull<T>) -> Option<U>>(
        mut parent: T,
        child: V,
    ) -> Option<Self> {
        let child = (child)((&mut parent).into())?;
        Some(BorrowExtender {
            _parent: parent,
            child,
        })
    }
}

#[cfg(feature = "async")]
#[derive(Debug)]
/// Pointer wrapper that is Send/Sync if it wraps a Send/Sync type.
///
/// Pointers are necessary for dealing with lifetimes in futures, and do not
/// auto-impl Send/Sync if the underlying type does. This exists to enable a
/// boundary cross reference that is Send/Sync.
pub struct ExtenderPtr<T>(*const T);

#[cfg(feature = "async")]
unsafe impl<T: Send> Send for ExtenderPtr<T> {}

#[cfg(feature = "async")]
unsafe impl<T: Sync> Sync for ExtenderPtr<T> {}

#[cfg(feature = "async")]
impl<T> From<&T> for ExtenderPtr<T> {
    fn from(value: &T) -> Self {
        Self(value)
    }
}

#[cfg(feature = "async")]
impl<T> From<ExtenderPtr<T>> for *const T {
    fn from(val: ExtenderPtr<T>) -> Self {
        val.0
    }
}

#[cfg(feature = "async")]
impl<T> ExtenderPtr<T> {
    pub fn ptr(self) -> *const T {
        self.0
    }
}

#[cfg(feature = "async")]
#[derive(Debug)]
/// NonNull wrapper that is Send/Sync if it wraps a Send/Sync type.
///
/// Pointers are necessary for dealing with lifetimes in futures, and do not
/// auto-impl Send/Sync if the underlying type does. This exists to enable a
/// boundary cross reference that is Send/Sync.
pub struct SendNonNull<T>(NonNull<T>);

#[cfg(feature = "async")]
unsafe impl<T: Send> Send for SendNonNull<T> {}

#[cfg(feature = "async")]
unsafe impl<T: Sync> Sync for SendNonNull<T> {}

#[cfg(feature = "async")]
impl<T> From<&mut T> for SendNonNull<T> {
    fn from(value: &mut T) -> Self {
        Self(value.into())
    }
}

#[cfg(feature = "async")]
impl<T> From<SendNonNull<T>> for NonNull<T> {
    fn from(val: SendNonNull<T>) -> NonNull<T> {
        val.0
    }
}

#[cfg(feature = "async")]
impl<T> SendNonNull<T> {
    pub fn ptr(self) -> *mut T {
        self.0.as_ptr()
    }
}

#[cfg(feature = "async")]
impl<T: StableDeref, U> BorrowExtender<T, U> {
    pub async fn a_new<F: Future<Output = U>, V: FnOnce(ExtenderPtr<T>) -> F>(
        parent: T,
        child: V,
    ) -> Self {
        let child = (child)((&parent).into()).await;
        Self {
            _parent: parent,
            child,
        }
    }

    pub async fn a_try_new<W, F: Future<Output = Result<U, W>>, V: FnOnce(ExtenderPtr<T>) -> F>(
        parent: T,
        child: V,
    ) -> Result<Self, W> {
        let child = (child)((&parent).into()).await?;
        Ok(Self {
            _parent: parent,
            child,
        })
    }

    pub async fn a_maybe_new<F: Future<Output = Option<U>>, V: FnOnce(ExtenderPtr<T>) -> F>(
        parent: T,
        child: V,
    ) -> Option<Self> {
        let child = (child)((&parent).into()).await?;
        Some(Self {
            _parent: parent,
            child,
        })
    }

    pub async fn a_new_mut<F: Future<Output = U>, V: FnOnce(SendNonNull<T>) -> F>(
        mut parent: T,
        child: V,
    ) -> Self {
        let child = (child)((&mut parent).into()).await;
        Self {
            _parent: parent,
            child,
        }
    }

    pub async fn a_try_new_mut<
        W,
        F: Future<Output = Result<U, W>>,
        V: FnOnce(SendNonNull<T>) -> F,
    >(
        mut parent: T,
        child: V,
    ) -> Result<Self, W> {
        let child = (child)((&mut parent).into()).await?;
        Ok(Self {
            _parent: parent,
            child,
        })
    }

    pub async fn a_maybe_new_mut<F: Future<Output = Option<U>>, V: FnOnce(SendNonNull<T>) -> F>(
        mut parent: T,
        child: V,
    ) -> Option<Self> {
        let child = (child)((&mut parent).into()).await?;
        Some(Self {
            _parent: parent,
            child,
        })
    }
}

impl<T: StableDeref, U> Deref for BorrowExtender<T, U> {
    type Target = U;
    fn deref(&self) -> &Self::Target {
        &self.child
    }
}

impl<T: StableDeref, U> DerefMut for BorrowExtender<T, U> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.child
    }
}

impl<T: StableDeref, U: AsRef<V>, V> AsRef<V> for BorrowExtender<T, U> {
    fn as_ref(&self) -> &V {
        self.child.as_ref()
    }
}

impl<T: StableDeref, U: AsMut<V>, V> AsMut<V> for BorrowExtender<T, U> {
    fn as_mut(&mut self) -> &mut V {
        self.child.as_mut()
    }
}

impl<'a, T: StableDeref, U> PartialEq<&'a U> for BorrowExtender<T, U>
where
    U: PartialEq<&'a U>,
{
    fn eq(&self, other: &&'a U) -> bool {
        self.child == other
    }
}

impl<T: StableDeref, U> PartialEq<U> for BorrowExtender<T, U>
where
    U: PartialEq<U>,
{
    fn eq(&self, other: &U) -> bool {
        self.child == *other
    }
}

impl<T: StableDeref, U> PartialOrd<U> for BorrowExtender<T, U>
where
    U: PartialOrd<U>,
{
    fn partial_cmp(&self, other: &U) -> Option<std::cmp::Ordering> {
        self.child.partial_cmp(other)
    }
}

impl<T: StableDeref, V, F> BorrowExtender<T, Result<V, F>> {
    pub fn open_result(self) -> Result<BorrowExtender<T, V>, F> {
        Ok(BorrowExtender::<T, V> {
            _parent: self._parent,
            child: self.child?,
        })
    }
}

impl<T: StableDeref, V> BorrowExtender<T, Option<V>> {
    pub fn open_option(self) -> Option<BorrowExtender<T, V>> {
        Some(BorrowExtender::<T, V> {
            _parent: self._parent,
            child: self.child?,
        })
    }
}

#[derive(Debug)]
pub struct NestDeref<A: StableDeref, B: StableDeref, C>(BorrowNest<A, B, C>);

impl<A: StableDeref, B: StableDeref, C> From<BorrowNest<A, B, C>> for NestDeref<A, B, C> {
    fn from(value: BorrowNest<A, B, C>) -> Self {
        Self(value)
    }
}

impl<A: StableDeref, B: StableDeref, C: StableDeref + Deref<Target = D>, D> Deref
    for NestDeref<A, B, C>
{
    type Target = D;
    fn deref(&self) -> &D {
        &self.0
    }
}
