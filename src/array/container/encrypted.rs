/*
#[cfg(feature = "encrypted")]
#[derive(Debug)]
pub struct SecretVec<T: Bytes> {
    inner: secrets::SecretVec<T>,
}

#[cfg(feature = "encrypted")]
impl<T: Bytes> Default for SecretVec<T> {
    fn default() -> Self {
        Self {
            inner: secrets::SecretVec::<T>::zero(0),
        }
    }
}

impl<T: Bytes> Container for SecretVec<T> {
    fn c_push(&mut self, value: Self::Data) {
        todo!()
    }
    fn c_remove(&mut self, index: usize) -> Self::Data {
        todo!()
    }
    fn c_append(&mut self, other: &mut Self) {
        todo!()
    }
}
*/
