/// [`From`] for the same type with different generics.
///
/// Trying to implement these with [`From`] is blocked by `Into<T> for T`.
pub trait GenericFrom<T> {
    /// Mirror of [`From::from`].
    fn gen_from(val: T) -> Self;
}

/// [`Into`] for the same type with different generics.
///
/// Trying to implement these with [`Into`] is blocked by `Into<T> for T`.
pub trait GenericInto<T> {
    /// Mirror of [`Into::into`].
    fn gen_into(self) -> T;
}

impl<T, U> GenericInto<U> for T where U: GenericFrom<T> {
    fn gen_into(self) -> U {
        U::gen_from(self)
    }
}
