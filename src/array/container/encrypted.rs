use std::{
    mem::transmute,
    ops::{Deref, DerefMut, Range},
    sync::OnceLock,
};

use secrets::{secret_vec::ItemMut, traits::Bytes};
use stable_deref_trait::StableDeref;

use crate::{
    array::BackedArray,
    entry::{
        disks::{Encrypted, SecretVecWrapper},
        BackedEntry,
    },
};

use super::{Container, MutIter, RefIter, ResizingContainer};

#[derive(Debug)]
pub struct SecretVecItemRef<'a, T: Bytes>(secrets::secret_vec::ItemRef<'a, T>);

/// [`secrets::secret_vec::Ref`] is backed by a Box pointer, and meets the
/// [`StableDeref`] API requirements.
unsafe impl<T: Bytes> StableDeref for SecretVecItemRef<'_, T> {}

impl<T: Bytes> Deref for SecretVecItemRef<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Bytes> AsRef<T> for SecretVecItemRef<'_, T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T: Bytes> RefIter<T> for SecretVecWrapper<T> {
    type IterRef<'b> = SecretVecItemRef<'b, T> where Self: 'b;
    fn ref_iter(&self) -> impl Iterator<Item = Self::IterRef<'_>> {
        let len = self.borrow().len();
        (0..len).map(|x| SecretVecItemRef(self.get(x).unwrap()))
    }
}

#[derive(Debug)]
pub struct SecretVecItemMut<'a, T: Bytes>(secrets::secret_vec::ItemMut<'a, T>);

/// [`secrets::secret_vec::Mut`] is backed by a Box pointer, and meets the
/// [`StableDeref`] API requirements.
unsafe impl<T: Bytes> StableDeref for SecretVecItemMut<'_, T> {}

impl<T: Bytes> Deref for SecretVecItemMut<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Bytes> DerefMut for SecretVecItemMut<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T: Bytes> AsRef<T> for SecretVecItemMut<'_, T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T: Bytes> AsMut<T> for SecretVecItemMut<'_, T> {
    fn as_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

#[derive(Debug)]
pub struct SecretVecRef<'a, T: Bytes>(secrets::secret_vec::Ref<'a, T>);

/// [`secrets::secret_vec::Mut`] is backed by a Box pointer, and meets the
/// [`StableDeref`] API requirements.
unsafe impl<T: Bytes> StableDeref for SecretVecRef<'_, T> {}

impl<T: Bytes> Deref for SecretVecRef<'_, T> {
    type Target = [T];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Bytes> AsRef<[T]> for SecretVecRef<'_, T> {
    fn as_ref(&self) -> &[T] {
        &self.0
    }
}

#[derive(Debug)]
pub struct SecretVecMut<'a, T: Bytes>(secrets::secret_vec::RefMut<'a, T>);

/// [`secrets::secret_vec::Mut`] is backed by a Box pointer, and meets the
/// [`StableDeref`] API requirements.
unsafe impl<T: Bytes> StableDeref for SecretVecMut<'_, T> {}

impl<T: Bytes> Deref for SecretVecMut<'_, T> {
    type Target = [T];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Bytes> DerefMut for SecretVecMut<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T: Bytes> AsRef<[T]> for SecretVecMut<'_, T> {
    fn as_ref(&self) -> &[T] {
        &self.0
    }
}

impl<T: Bytes> AsMut<[T]> for SecretVecMut<'_, T> {
    fn as_mut(&mut self) -> &mut [T] {
        &mut self.0
    }
}

impl<T: Bytes> Default for SecretVecWrapper<T> {
    fn default() -> Self {
        Self(secrets::SecretVec::zero(0))
    }
}

impl<T: Bytes> MutIter<T> for SecretVecWrapper<T> {
    type IterMut<'b> = SecretVecItemMut<'b, T> where Self: 'b;
    fn mut_iter(&mut self) -> impl Iterator<Item = Self::IterMut<'_>> {
        let len = self.borrow().len();
        (0..len).map(|x| {
            let mut_borrow = self.get_mut(x).unwrap();
            // All of these mutable borrows are unique, escape FnMut borrowing
            // bounds.
            let mut_borrow = unsafe { transmute::<ItemMut<'_, T>, ItemMut<'_, T>>(mut_borrow) };
            SecretVecItemMut(mut_borrow)
        })
    }
}

impl<T: Bytes> Container for SecretVecWrapper<T> {
    type Data = T;
    type Ref<'b> = SecretVecItemRef<'b, T> where Self: 'b;
    type Mut<'b> = SecretVecItemMut<'b, T> where Self: 'b;
    type RefSlice<'b> = SecretVecRef<'b, T> where Self: 'b;
    type MutSlice<'b> = SecretVecMut<'b, T> where Self: 'b;

    fn c_get(&self, index: usize) -> Option<Self::Ref<'_>> {
        self.get(index).map(SecretVecItemRef)
    }
    fn c_get_mut(&mut self, index: usize) -> Option<Self::Mut<'_>> {
        self.get_mut(index).map(SecretVecItemMut)
    }
    fn c_len(&self) -> usize {
        self.borrow().len()
    }
    fn c_ref(&self) -> Self::RefSlice<'_> {
        SecretVecRef(self.borrow())
    }
    fn c_mut(&mut self) -> Self::MutSlice<'_> {
        SecretVecMut(self.borrow_mut())
    }
}

impl<D: Bytes> FromIterator<D> for SecretVecWrapper<D> {
    fn from_iter<T: IntoIterator<Item = D>>(iter: T) -> Self {
        let data: Vec<_> = iter.into_iter().collect();

        Self(secrets::SecretVec::new(data.len(), |s| {
            s.copy_from_slice(&data)
        }))
    }
}

impl<B: Bytes> Extend<B> for SecretVecWrapper<B> {
    fn extend<T: IntoIterator<Item = B>>(&mut self, iter: T) {
        let new: Self = FromIterator::from_iter(iter);
        self.0 = secrets::SecretVec::new(self.len() + new.len(), |s| {
            s[0..].copy_from_slice(&self.borrow());
            s[self.len()..].copy_from_slice(&new.borrow());
        })
    }
}

impl<T: Bytes> ResizingContainer for SecretVecWrapper<T> {
    /// Very inefficient push approach!
    fn c_push(&mut self, value: Self::Data) {
        self.0 = secrets::SecretVec::new(self.len() + 1, |s| {
            s[0..].copy_from_slice(&self.borrow());
            s[s.len() - 1] = value;
        });
    }

    /// Very inefficient remove approach!
    fn c_remove(&mut self, index: usize) {
        self.0 = secrets::SecretVec::new(self.len() - 1, |s| {
            let contents = self.borrow();
            s[0..].copy_from_slice(&contents[0..index]);
            s[index..].copy_from_slice(&contents[index + 1..]);
        });
    }

    // Append approach efficiency is fine, actually.
    fn c_append(&mut self, other: &mut Self) {
        self.0 = secrets::SecretVec::new(self.len() + other.len(), |s| {
            s[0..].copy_from_slice(&self.borrow());
            s[self.len()..].copy_from_slice(&other.borrow());
        });
    }
}

/// Wraps `T` in [`SecretVecWrapper`] and wraps `Disk` in [`Encrypted`].
pub type SecretBackedArray<'a, T, Disk, Coder> = BackedArray<
    Vec<Range<usize>>,
    Vec<BackedEntry<OnceLock<SecretVecWrapper<T>>, Encrypted<'a, Disk>, Coder>>,
>;

#[cfg(test)]
#[cfg(not(miri))]
#[cfg(feature = "bincode")]
mod tests {
    use std::{io::Cursor, sync::LazyLock};

    use crate::{
        array::VecBackedArray,
        entry::{disks::Encrypted, formats::BincodeCoder},
        test_utils::OwnedCursorVec,
    };

    use super::*;

    const INPUT_1: [u8; 4] = [32, 4, 5, 89];
    const INPUT_2: [u8; 4] = [23, 34, 51, 98];
    static COMBINED: LazyLock<Vec<u8>> = LazyLock::new(|| [INPUT_1, INPUT_2].concat());

    // Disabled until I find a good way to catch a SIGSEGV in testing,
    // comments can be verified by re-enabling and running.
    #[test]
    #[ignore = "can't catch and resume from SIGSEGV"]
    fn panic_on_peek() {
        let backing = OwnedCursorVec::new(Cursor::default());

        let mut array = SecretBackedArray::new();

        array
            .append_memory(INPUT_1, Encrypted::new_random(backing), BincodeCoder {})
            .unwrap();

        let val = array.generic_get(0).unwrap();

        #[cfg(not(miri))]
        let val_ptr: *const _ = &val;

        // This does not cause a crash
        #[cfg(not(miri))]
        assert_eq!(unsafe { **val_ptr }, 32);

        drop(val);

        // This causes a crash
        #[cfg(not(miri))]
        assert_eq!(unsafe { **val_ptr }, 32);
    }

    // This is the kind of behavior SecretVec prevents by panicking. The test
    // may need to be updated on a future rustc or LLVM version. It shouldn't
    // be done in actual code, for obvious reasons.
    #[test]
    fn allow_unprotected_peek() {
        let backing = OwnedCursorVec::default();

        let mut array = VecBackedArray::new();

        array
            .append_memory(INPUT_1, backing, BincodeCoder {})
            .unwrap();

        let val = array.generic_get(0).unwrap();

        #[cfg(not(miri))]
        let val_ptr: *const _ = &val;
        #[cfg(not(miri))]
        assert_eq!(unsafe { **val_ptr }, 32);

        drop(val);

        #[cfg(not(miri))]
        assert_eq!(unsafe { **val_ptr }, 32);
    }

    // Miri can't handle the FFI calls
    #[test]
    fn multiple_retrieve() {
        let mut array = SecretBackedArray::new();

        array
            .append_memory(
                INPUT_1,
                Encrypted::new_random(OwnedCursorVec::default()),
                BincodeCoder {},
            )
            .unwrap();
        array
            .append_memory(
                INPUT_2,
                Encrypted::new_random(OwnedCursorVec::default()),
                BincodeCoder {},
            )
            .unwrap();

        array
            .generic_iter()
            .enumerate()
            .for_each(|(x, val)| assert_eq!(*val.unwrap(), COMBINED[x]));

        let mut vals: Vec<_> = [0, 2, 4, 6, 7]
            .into_iter()
            .map(|x| array.generic_get(x).unwrap())
            .collect();

        // Should still be valid after a disconnected handle drops.
        vals.pop();
        assert_eq!(*vals[3], COMBINED[6]);
        assert_eq!(*vals[0], COMBINED[0]);

        // Should still be valid after all other handles dropped
        let three = vals.pop().unwrap();
        drop(vals);
        assert_eq!(*three, COMBINED[6]);
    }
}
