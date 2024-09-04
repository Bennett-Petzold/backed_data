/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use std::{
    iter::FusedIterator,
    mem::transmute,
    ops::{Deref, DerefMut},
    sync::OnceLock,
};

use num_traits::{Saturating, SaturatingAdd};
use secrets::{secret_vec::ItemMut, traits::Bytes, SecretVec};
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
        (0..len).map(|x| SecretVecItemRef(SecretVec::get(self, x).unwrap()))
    }
}

#[derive(Debug)]
pub struct SecretVecItemMut<'a, T: Bytes>(secrets::secret_vec::ItemMut<'a, T>);

/// [`secrets::secret_vec::ItemMut`] is backed by a Box pointer, and meets the
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

/// [`secrets::secret_vec::ItemMut`] is backed by a Box pointer, and meets the
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

/// [`secrets::secret_vec::RefMut`] is backed by a Box pointer, and meets the
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
            let mut_borrow = SecretVec::get_mut(self, x).unwrap();
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

    fn get(&self, index: usize) -> Option<Self::Ref<'_>> {
        secrets::SecretVec::<T>::get(self, index).map(SecretVecItemRef)
    }
    fn get_mut(&mut self, index: usize) -> Option<Self::Mut<'_>> {
        secrets::SecretVec::<T>::get_mut(self, index).map(SecretVecItemMut)
    }
    fn len(&self) -> usize {
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

#[derive(Debug)]
/// Takes each element directly out of the underlying protected region.
///
/// The returned data is not in protected heap, so make sure the iterator
/// results are placed into a protected region to avoid leaks.
pub struct SecretVecIter<B: Bytes> {
    vec: SecretVec<B>,
    pos: usize,
}

impl<B: Bytes> Iterator for SecretVecIter<B> {
    type Item = B;

    fn next(&mut self) -> Option<Self::Item> {
        self.nth(0)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        // May overflow, if so cap. It will return zero items when capped,
        // because no `usize` is greater than `usize::MAX`.
        self.pos = if let Some(pos) = self.pos.checked_add(n) {
            pos
        } else {
            usize::MAX
        };

        if self.pos < self.vec.len() {
            let idx = self.pos;
            // Cannot overflow
            self.pos += 1;

            let mut vec = self.vec.borrow_mut();
            Some(std::mem::replace(
                &mut vec.as_mut()[idx],
                B::uninitialized(),
            ))
        } else {
            None
        }
    }
}

impl<B: Bytes> FusedIterator for SecretVecIter<B> {}

impl<B: Bytes> IntoIterator for SecretVecWrapper<B> {
    type Item = B;
    type IntoIter = SecretVecIter<B>;
    fn into_iter(self) -> Self::IntoIter {
        SecretVecIter {
            vec: self.0,
            pos: 0,
        }
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
    fn push(&mut self, value: Self::Data) {
        self.0 = secrets::SecretVec::new(self.len() + 1, |s| {
            let last_idx = s.len() - 1;
            s[0..last_idx].copy_from_slice(&self.borrow());
            s[last_idx] = value;
        });
    }

    /// Very inefficient remove approach!
    fn remove(&mut self, index: usize) {
        self.0 = secrets::SecretVec::new(self.len() - 1, |s| {
            let contents = self.borrow();
            s[0..].copy_from_slice(&contents[0..index]);
            s[index..].copy_from_slice(&contents[index + 1..]);
        });
    }

    // Append approach efficiency is fine, actually.
    fn append(&mut self, other: &mut Self) {
        self.0 = secrets::SecretVec::new(self.len() + other.len(), |s| {
            s[0..].copy_from_slice(&self.borrow());
            s[self.len()..].copy_from_slice(&other.borrow());
        });
    }
}

/*
/// Wraps `T` in [`SecretVecWrapper`] and wraps `Disk` in [`Encrypted`].
pub type SecretBackedArray<'a, T, Disk, Coder> = BackedArray<
    SecretVecWrapper<usize>,
    Vec<BackedEntry<OnceLock<SecretVecWrapper<T>>, Encrypted<'a, Disk>, Coder>>,
>;
*/

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
    fn panion_peek() {
        let backing = OwnedCursorVec::new(Cursor::default());

        let mut array = SecretBackedArray::new();

        array
            .append_memory(
                INPUT_1,
                Encrypted::new_random(backing),
                BincodeCoder::default(),
            )
            .unwrap();

        let val = array.generiget(0).unwrap();

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
            .append_memory(INPUT_1, backing, BincodeCoder::default())
            .unwrap();

        let val = array.generiget(0).unwrap();

        #[cfg(not(miri))]
        let val_ptr: *const _ = &val;
        #[cfg(not(miri))]
        assert_eq!(unsafe { **val_ptr }, 32);

        drop(val);

        #[cfg(not(miri))]
        assert_eq!(unsafe { **val_ptr }, 32);

        assert_eq!(*array.generiget(0).unwrap(), 32);
    }

    // Miri can't handle the FFI calls
    #[test]
    fn multiple_retrieve() {
        let mut array = SecretBackedArray::new();

        array
            .append_memory(
                INPUT_1,
                Encrypted::new_random(OwnedCursorVec::default()),
                BincodeCoder::default(),
            )
            .unwrap();
        array
            .append_memory(
                INPUT_2,
                Encrypted::new_random(OwnedCursorVec::default()),
                BincodeCoder::default(),
            )
            .unwrap();

        array
            .generiiter()
            .enumerate()
            .for_each(|(x, val)| assert_eq!(*val.unwrap(), COMBINED[x]));

        let mut vals: Vec<_> = [0, 2, 4, 6, 7]
            .into_iter()
            .map(|x| array.generiget(x).unwrap())
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
