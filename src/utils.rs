/// Wrapper that allows &T to implement [`AsRef<T>`].
#[derive(Debug)]
pub struct ToRef<'a, T>(pub &'a T);

impl<T> AsRef<T> for ToRef<'_, T> {
    fn as_ref(&self) -> &T {
        self.0
    }
}

/// Wrapper that allows &mut T to implement [`AsMut<T>`] and [`AsRef<T>`].
#[derive(Debug)]
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

/// Combined trait for types with both [`AsRef`] and [`AsMut`].
pub trait AsRefMut<T>: AsRef<T> + AsMut<T> {}
impl<T, U> AsRefMut<T> for U where U: AsRef<T> + AsMut<T> {}
