use std::{cell::OnceCell, sync::OnceLock};

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
