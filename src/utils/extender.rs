use std::{
    ops::{Deref, DerefMut},
    pin::Pin,
};

#[cfg(feature = "async")]
use futures::Future;

/// Takes ownership of a parent value and computes the child with a reference.
///
/// Used to prevent the parent from being freed, and therefore return a
/// referenced value from a function.
#[derive(Debug)]
pub struct BorrowExtender<T, U> {
    _parent: T,
    child: U,
}

pub type BorrowNest<A, B, C> = BorrowExtender<BorrowExtender<A, B>, C>;

impl<T, U> BorrowExtender<T, U> {
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
impl<T, U> BorrowExtender<T, U> {
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

impl<T, U: AsRef<V>, V> AsRef<V> for BorrowExtender<T, U> {
    fn as_ref(&self) -> &V {
        self.child.as_ref()
    }
}

impl<T, U> AsMut<U> for BorrowExtender<T, U> {
    fn as_mut(&mut self) -> &mut U {
        &mut self.child
    }
}

impl<'a, T, U> PartialEq<&'a U> for BorrowExtender<T, U>
where
    U: PartialEq<&'a U>,
{
    fn eq(&self, other: &&'a U) -> bool {
        self.child == other
    }
}

impl<T, U> PartialEq<U> for BorrowExtender<T, U>
where
    U: PartialEq<U>,
{
    fn eq(&self, other: &U) -> bool {
        self.child == *other
    }
}

impl<T, U> PartialOrd<U> for BorrowExtender<T, U>
where
    U: PartialOrd<U>,
{
    fn partial_cmp(&self, other: &U) -> Option<std::cmp::Ordering> {
        self.child.partial_cmp(other)
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

/// [`BorrowExtender`], but uses boxed pins to satisfy stack borrows rules
/// for mutable borrows.
///
/// This uses a pinned box. This heap allocation is undesirable, but needed
/// for the solution as it currently stands.
#[derive(Debug)]
pub struct BorrowExtenderMut<T, U> {
    _parent: Pin<Box<T>>,
    child: U,
}

impl<T, U> BorrowExtenderMut<T, U> {
    pub fn new<V: FnOnce(&mut T) -> U>(parent: T, child: V) -> Self {
        let mut parent = Box::pin(parent);
        let child = (child)(unsafe { parent.as_mut().get_unchecked_mut() });
        BorrowExtenderMut {
            _parent: parent,
            child,
        }
    }

    pub fn try_new<W, V: FnOnce(&mut T) -> Result<U, W>>(parent: T, child: V) -> Result<Self, W> {
        let mut parent = Box::pin(parent);
        let child = (child)(unsafe { parent.as_mut().get_unchecked_mut() })?;
        Ok(Self {
            _parent: parent,
            child,
        })
    }

    pub fn maybe_new<V: FnOnce(&mut T) -> Option<U>>(parent: T, child: V) -> Option<Self> {
        let mut parent = Box::pin(parent);
        let child = (child)(unsafe { parent.as_mut().get_unchecked_mut() })?;
        Some(Self {
            _parent: parent,
            child,
        })
    }
}

#[cfg(feature = "async")]
impl<T, U> BorrowExtenderMut<T, U> {
    pub async fn a_new<F: Future<Output = U>, V: FnOnce(&mut T) -> F>(parent: T, child: V) -> Self {
        let mut parent = Box::pin(parent);
        let child = (child)(unsafe { parent.as_mut().get_unchecked_mut() }).await;
        Self {
            _parent: parent,
            child,
        }
    }

    pub async fn a_try_new<W, F: Future<Output = Result<U, W>>, V: FnOnce(&mut T) -> F>(
        parent: T,
        child: V,
    ) -> Result<Self, W> {
        let mut parent = Box::pin(parent);
        let child = (child)(unsafe { parent.as_mut().get_unchecked_mut() }).await?;
        Ok(Self {
            _parent: parent,
            child,
        })
    }

    pub async fn a_maybe_new<F: Future<Output = Option<U>>, V: FnOnce(&mut T) -> F>(
        parent: T,
        child: V,
    ) -> Option<Self> {
        let mut parent = Box::pin(parent);
        let child = (child)(unsafe { parent.as_mut().get_unchecked_mut() }).await?;
        Some(Self {
            _parent: parent,
            child,
        })
    }
}

impl<T, U> Deref for BorrowExtenderMut<T, U> {
    type Target = U;
    fn deref(&self) -> &Self::Target {
        &self.child
    }
}

impl<T, U> DerefMut for BorrowExtenderMut<T, U> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.child
    }
}

impl<T, U> AsRef<U> for BorrowExtenderMut<T, U> {
    fn as_ref(&self) -> &U {
        &self.child
    }
}

impl<T, U> AsMut<U> for BorrowExtenderMut<T, U> {
    fn as_mut(&mut self) -> &mut U {
        &mut self.child
    }
}

impl<T, V, F> BorrowExtenderMut<T, Result<V, F>> {
    pub fn open_result(self) -> Result<BorrowExtenderMut<T, V>, F> {
        Ok(BorrowExtenderMut::<T, V> {
            _parent: self._parent,
            child: self.child?,
        })
    }
}

impl<T, V> BorrowExtenderMut<T, Option<V>> {
    pub fn open_option(self) -> Option<BorrowExtenderMut<T, V>> {
        Some(BorrowExtenderMut::<T, V> {
            _parent: self._parent,
            child: self.child?,
        })
    }
}
