use std::ops::{Deref, DerefMut};

/// Wrapper that allows &T to implement [`AsRef<T>`].
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ToRef<'a, T: ?Sized>(pub &'a T);

impl<T: ?Sized> AsRef<T> for ToRef<'_, T> {
    fn as_ref(&self) -> &T {
        self.0
    }
}

impl<T: ?Sized> Deref for ToRef<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<'a, T: ?Sized> PartialEq<&'a T> for ToRef<'a, T>
where
    &'a T: PartialEq<&'a T>,
{
    fn eq(&self, other: &&'a T) -> bool {
        self.0 == other
    }
}

impl<'a, T: ?Sized> PartialEq<T> for ToRef<'a, T>
where
    &'a T: PartialEq<T>,
{
    fn eq(&self, other: &T) -> bool {
        self.0 == *other
    }
}

impl<'a, T: ?Sized> PartialOrd<&'a T> for ToRef<'a, T>
where
    &'a T: PartialOrd<&'a T>,
{
    fn partial_cmp(&self, other: &&'a T) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(other)
    }
}

/// Wrapper that allows &mut T to implement [`AsMut<T>`] and [`AsRef<T>`].
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ToMut<'a, T: ?Sized>(pub &'a mut T);

impl<T: ?Sized> AsRef<T> for ToMut<'_, T> {
    fn as_ref(&self) -> &T {
        self.0
    }
}

impl<T: ?Sized> AsMut<T> for ToMut<'_, T> {
    fn as_mut(&mut self) -> &mut T {
        self.0
    }
}

impl<T: ?Sized> Deref for ToMut<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<T: ?Sized> DerefMut for ToMut<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
    }
}

impl<'a, T: ?Sized> PartialEq<&'a T> for ToMut<'a, T>
where
    &'a mut T: PartialEq<&'a T>,
{
    fn eq(&self, other: &&'a T) -> bool {
        self.0 == other
    }
}

impl<'a, T: ?Sized> PartialEq<T> for ToMut<'a, T>
where
    &'a mut T: PartialEq<T>,
{
    fn eq(&self, other: &T) -> bool {
        self.0 == *other
    }
}

impl<'a, T: ?Sized> PartialOrd<&'a T> for ToMut<'a, T>
where
    &'a mut T: PartialOrd<&'a T>,
{
    fn partial_cmp(&self, other: &&'a T) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(other)
    }
}

/// Wrapper that allows &T to implement [`AsRef<T>`].
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct IndirectRef<'a, T: ?Sized>(pub &'a T);

impl<T: AsRef<U> + ?Sized, U: ?Sized> AsRef<U> for IndirectRef<'_, T> {
    fn as_ref(&self) -> &U {
        self.0.as_ref()
    }
}

impl<T: ?Sized> Deref for IndirectRef<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<'a, T: ?Sized> PartialEq<&'a T> for IndirectRef<'a, T>
where
    &'a T: PartialEq<&'a T>,
{
    fn eq(&self, other: &&'a T) -> bool {
        self.0 == other
    }
}

impl<'a, T: ?Sized> PartialEq<T> for IndirectRef<'a, T>
where
    &'a T: PartialEq<T>,
{
    fn eq(&self, other: &T) -> bool {
        self.0 == *other
    }
}

impl<'a, T: ?Sized> PartialOrd<&'a T> for IndirectRef<'a, T>
where
    &'a T: PartialOrd<&'a T>,
{
    fn partial_cmp(&self, other: &&'a T) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(other)
    }
}

/// Wrapper that allows &mut T to implement [`AsMut<T>`] and [`AsRef<T>`].
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct IndirectMut<'a, T: ?Sized>(pub &'a mut T);

impl<T: AsRef<U> + ?Sized, U: ?Sized> AsRef<U> for IndirectMut<'_, T> {
    fn as_ref(&self) -> &U {
        self.0.as_ref()
    }
}

impl<T: AsMut<U> + ?Sized, U: ?Sized> AsMut<U> for IndirectMut<'_, T> {
    fn as_mut(&mut self) -> &mut U {
        self.0.as_mut()
    }
}

impl<T: ?Sized> Deref for IndirectMut<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<T: ?Sized> DerefMut for IndirectMut<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
    }
}

impl<'a, T: ?Sized> PartialEq<&'a T> for IndirectMut<'a, T>
where
    &'a mut T: PartialEq<&'a T>,
{
    fn eq(&self, other: &&'a T) -> bool {
        self.0 == other
    }
}

impl<'a, T: ?Sized> PartialEq<T> for IndirectMut<'a, T>
where
    &'a mut T: PartialEq<T>,
{
    fn eq(&self, other: &T) -> bool {
        self.0 == *other
    }
}

impl<'a, T: ?Sized> PartialOrd<&'a T> for IndirectMut<'a, T>
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
