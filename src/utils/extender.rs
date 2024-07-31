use std::{
    ops::{Deref, DerefMut},
    pin::Pin,
};

#[cfg(feature = "async")]
use futures::Future;

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
