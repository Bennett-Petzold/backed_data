use std::{fmt::Debug, iter::FusedIterator, ops::Range};

#[cfg(feature = "unsafe_array")]
use std::{
    iter::Peekable,
    mem::transmute,
    ops::DerefMut,
    sync::{Arc, Mutex},
};

use crate::entry::{sync_impl::BackedEntryMut, BackedEntry};

use super::{
    super::{
        container::{
            BackedEntryContainer, BackedEntryContainerNestedAll, BackedEntryContainerNestedRead,
            Container,
        },
        internal_idx, BackedArray, BackedArrayError,
    },
    CountedHandle,
};

/// Read implementations
impl<K: Container<Data = Range<usize>>, E: BackedEntryContainerNestedRead> BackedArray<K, E>
where
    E: AsRef<[E::Data]>,
    E::Unwrapped: AsRef<[E::InnerData]>,
{
    /// Return a value (potentially loading its backing array).
    ///
    /// Backing arrays stay in memory until freed by a mutable function.
    pub fn get(&self, idx: usize) -> Result<&E::InnerData, BackedArrayError<E::ReadError>> {
        let loc = internal_idx(self.keys.c_ref().as_ref(), idx)
            .ok_or(BackedArrayError::OutsideEntryBounds(idx))?;

        let wrapped_container = &self.entries.as_ref()[loc.entry_idx];
        let backed_entry = BackedEntryContainer::get_ref(wrapped_container);
        let entry = backed_entry.load().map_err(BackedArrayError::Coder)?;
        Ok(&entry.as_ref()[loc.inside_entry_idx])
    }
}

// ---------- Iterator Returns ---------- //

/// Specialization of [`BackedArrayIter`] for slices.
#[derive(Debug)]
pub struct BackedArrayIter<'a, K, E> {
    backed: &'a BackedArray<K, E>,
    pos: usize,
}

impl<'a, K, E> BackedArrayIter<'a, K, E> {
    fn new(backed: &'a BackedArray<K, E>) -> Self {
        Self { backed, pos: 0 }
    }
}

impl<'a, K: Container<Data = Range<usize>>, E: BackedEntryContainerNestedRead> Iterator
    for BackedArrayIter<'a, K, E>
where
    E: AsRef<[E::Data]>,
    E::Unwrapped: AsRef<[E::InnerData]>,
{
    type Item = Result<&'a E::InnerData, E::ReadError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.nth(0)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.pos += n;
        match self.backed.get(self.pos) {
            Ok(val) => {
                self.pos += 1;
                Some(Ok(val))
            }
            Err(e) => match e {
                BackedArrayError::OutsideEntryBounds(_) => None,
                BackedArrayError::Coder(c) => {
                    self.pos += 1;
                    Some(Err(c))
                }
            },
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.backed.len() - self.pos;
        (remaining, Some(remaining))
    }
}

impl<K: Container<Data = Range<usize>>, E: BackedEntryContainerNestedRead> FusedIterator
    for BackedArrayIter<'_, K, E>
where
    E: AsRef<[E::Data]>,
    E::Unwrapped: AsRef<[E::InnerData]>,
{
}

/// Iterates over a backed array, returning each item in order.
///
/// To keep an accurate size count, failed reads will not be retried.
/// This will keep each disk loaded after pulling data from it.
/// Stepping by > 1 with `nth` implementation may skip loading a disk.
#[cfg(feature = "unsafe_array")]
pub struct BackedArrayIterMut<'a, E: BackedEntryContainerNestedAll + 'a> {
    pos: usize,
    len: usize,
    keys: Peekable<std::slice::Iter<'a, Range<usize>>>,
    entries: std::slice::IterMut<'a, E::Data>,
    // TODO: Rewrite this to be a box of Once or MaybeUninit type
    // Problem with once types is that either a generic needs to be introduced,
    // or this iterator needs to choose between cell/lock tradeoffs.
    #[allow(clippy::type_complexity)]
    handles: Vec<
        Arc<
            Mutex<
                Result<
                    BackedEntryMut<'a, BackedEntry<E::OnceWrapper, E::Disk, E::Coder>>,
                    E::ReadError,
                >,
            >,
        >,
    >,
}

#[cfg(feature = "unsafe_array")]
impl<'a, E: BackedEntryContainerNestedAll> BackedArrayIterMut<'a, E>
where
    E: AsMut<[E::Data]>,
    E::Unwrapped: AsMut<[E::InnerData]>,
{
    fn new<K>(backed: &'a mut BackedArray<K, E>) -> Self
    where
        K: AsRef<[Range<usize>]>,
    {
        let keys = backed.keys.as_ref();

        let len = keys.last().unwrap_or(&(0..0)).end;
        let mut handles = Vec::with_capacity(backed.chunks_len());

        let keys = keys.iter().peekable();
        let mut entries = backed.entries.as_mut().iter_mut();

        if let Some(entry) = entries.by_ref().next() {
            handles.push(Arc::new(Mutex::new(entry.get_mut().mut_handle())));
        }

        Self {
            pos: 0,
            len,
            keys,
            handles,
            entries,
        }
    }

    /// Flush all opened handles.
    pub fn flush(&mut self) -> Result<&mut Self, E::WriteError> {
        for h in self.handles.iter_mut() {
            let mut h = h.lock().unwrap();
            if let Ok(h) = h.deref_mut() {
                h.flush()?;
            }
        }
        Ok(self)
    }
}

#[cfg(feature = "unsafe_array")]
impl<E: BackedEntryContainerNestedAll> Drop for BackedArrayIterMut<'_, E> {
    fn drop(&mut self) {
        for h in self.handles.iter_mut() {
            if let Some(h) = Arc::get_mut(h) {
                let mut h = h.get_mut().unwrap();
                if let Ok(h) = h.deref_mut() {
                    h.flush()
                        .map_err(|_| "Failed to drop handle in BackedArrayIterMut!")
                        .unwrap();
                }
            }
        }
    }
}

#[cfg(feature = "unsafe_array")]
impl<'a, E: BackedEntryContainerNestedAll + 'a> Iterator for BackedArrayIterMut<'a, E>
where
    E::Unwrapped: 'a,
    E::ReadError: 'a,
    E::OnceWrapper: 'a,
    E: AsMut<[E::Data]>,
    E::Unwrapped: AsMut<[E::InnerData]>,
{
    type Item = Result<
        CountedHandle<'a, BackedEntry<E::OnceWrapper, E::Disk, E::Coder>, &'a mut E::InnerData>,
        &'a E::ReadError,
    >;

    fn next(&mut self) -> Option<Self::Item> {
        self.nth(0)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.pos += n;

        // # of steps forward taken by keys
        let mut step_count = 0;

        // Remove all key positions prior to `pos`.
        // This iterator only goes forward, so keys are not reused once
        // passed.
        while let Some(key) = self.keys.by_ref().peek() {
            if key.end > self.pos {
                // Update to latest entry handle
                break;
            }
            let _ = self.keys.by_ref().next();
            step_count += 1;
        }

        let inner_pos = self.pos - self.keys.peek()?.start;
        self.pos += 1; // Position advances, even on errors

        if step_count > 0 {
            let entry = self.entries.nth(step_count - 1).unwrap().get_mut();
            self.handles.push(Arc::new(Mutex::new(entry.mut_handle())));
        }

        let handle = self.handles.last_mut().unwrap().clone();
        let mut handle_locked = handle.lock().unwrap();

        Some(match &mut *handle_locked {
            Ok(h) => {
                // The data is valid for the lifetime of the underlying
                // handle, which is guarded by an Arc, not the iterator struct.
                let val = &mut h.as_mut()[inner_pos];
                let val = unsafe { transmute::<&mut E::InnerData, &'a mut E::InnerData>(val) };

                Ok(CountedHandle::new(handle.clone(), val))
            }
            Err(e) => {
                // The error is valid for the lifetime of the underlying
                // handle, which is guarded by an Arc, not the iterator struct.
                let e = unsafe { transmute::<&E::ReadError, &'a E::ReadError>(e) };
                Err(e)
            }
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.len - self.pos;
        (remaining, Some(remaining))
    }
}

#[cfg(feature = "unsafe_array")]
impl<'a, E: BackedEntryContainerNestedAll + 'a> FusedIterator for BackedArrayIterMut<'a, E>
where
    E::Unwrapped: 'a,
    E::ReadError: 'a,
    E::OnceWrapper: 'a,
    E: AsMut<[E::Data]>,
    E::Unwrapped: AsMut<[E::InnerData]>,
{
}

impl<K: Container<Data = Range<usize>>, E: BackedEntryContainerNestedRead> BackedArray<K, E>
where
    E: AsRef<[E::Data]>,
    E::Unwrapped: AsRef<[E::InnerData]>,
{
    /// Iterates over each backed item.
    pub fn iter(&self) -> BackedArrayIter<'_, K, E> {
        BackedArrayIter::new(self)
    }
}

impl<K: Container<Data = Range<usize>>, E: BackedEntryContainerNestedRead> BackedArray<K, E>
where
    E: AsRef<[E::Data]>,
{
    /// Returns underlying chunks in order.
    ///
    /// This will load each chunk before providing it.
    /// All chunks loaded earlier in the iteration will remain loaded.
    pub fn chunk_iter(&self) -> impl Iterator<Item = Result<&E::Unwrapped, E::ReadError>> {
        self.entries.as_ref().iter().map(|arr| arr.get_ref().load())
    }
}

#[cfg(feature = "unsafe_array")]
impl<K: Container<Data = Range<usize>>, E: BackedEntryContainerNestedAll> BackedArray<K, E>
where
    K: AsRef<[Range<usize>]>,
    E: AsMut<[E::Data]>,
    E::Unwrapped: AsMut<[E::InnerData]>,
{
    /// Returns [`BackedArrayIterMut`], which automatically tracks and forces
    /// flushes when mutated values are dropped. Both the iterator and its
    /// handles also provide manually callable flush methods.
    pub fn iter_mut<'a>(&'a mut self) -> BackedArrayIterMut<'a, E>
    where
        E::Unwrapped: 'a,
        E::ReadError: 'a,
    {
        BackedArrayIterMut::new(self)
    }
}

impl<K: Container<Data = Range<usize>>, E: BackedEntryContainerNestedAll> BackedArray<K, E>
where
    E: AsMut<[E::Data]>,
{
    /// Provides mutable handles to underlying chunks, using [`BackedEntryMut`].
    ///
    /// See [`Self::chunk_iter`] for the immutable iterator.
    pub fn chunk_mut_iter(
        &mut self,
    ) -> impl Iterator<
        Item = Result<
            BackedEntryMut<'_, BackedEntry<E::OnceWrapper, E::Disk, E::Coder>>,
            E::ReadError,
        >,
    > {
        self.entries.as_mut().iter_mut().map(|arr| {
            let arr = BackedEntryContainer::get_mut(arr);
            arr.mut_handle()
        })
    }
}

#[cfg(test)]
#[cfg(feature = "bincode")]
mod tests {
    use std::{io::Cursor, sync::Mutex};

    use itertools::Itertools;

    use crate::{entry::formats::BincodeCoder, test_utils::cursor_vec, test_utils::CursorVec};

    use super::super::super::*;

    #[test]
    fn multiple_retrieve() {
        let mut back_vector_0 = &mut Cursor::new(Vec::with_capacity(3));
        let back_vector_0 = CursorVec {
            inner: Mutex::new(&mut back_vector_0),
        };
        let mut back_vector_1 = &mut Cursor::new(Vec::with_capacity(3));
        let back_vector_1 = CursorVec {
            inner: Mutex::new(&mut back_vector_1),
        };

        const INPUT_0: [u8; 3] = [0, 1, 1];
        const INPUT_1: [u8; 3] = [2, 3, 5];
        let combined = [INPUT_0, INPUT_1].concat();

        let mut backed = VecBackedArray::new();
        backed
            .append(INPUT_0, back_vector_0, BincodeCoder::default())
            .unwrap();
        backed
            .append_memory(INPUT_1, back_vector_1, BincodeCoder::default())
            .unwrap();

        assert_eq!(*backed.get(0).unwrap(), 0);
        assert_eq!(*backed.get(4).unwrap(), 3);
        assert_eq!((*backed.get(0).unwrap(), *backed.get(4).unwrap()), (0, 3));

        [0, 2, 4, 5, 5, 1, 0, 5]
            .into_iter()
            .for_each(|x| assert_eq!(*backed.get(x).unwrap(), combined[x]));
    }

    #[test]
    fn out_of_bounds_access() {
        let mut back_vector = &mut Cursor::new(Vec::new());
        let back_vector = CursorVec {
            inner: Mutex::new(&mut back_vector),
        };

        const INPUT: &[u8] = &[0, 1, 1];

        let mut backed = VecBackedArray::new();
        backed
            .append(INPUT, back_vector, BincodeCoder::default())
            .unwrap();

        assert!(backed.get(0).is_ok());
        assert!(backed.get(3).is_err());
        assert!(backed.get(10).is_err());
    }

    #[test]
    fn chunk_iteration() {
        let mut back_vector_0 = &mut Cursor::new(Vec::with_capacity(3));
        let back_vector_0 = CursorVec {
            inner: Mutex::new(&mut back_vector_0),
        };
        let mut back_vector_1 = &mut Cursor::new(Vec::with_capacity(3));
        let back_vector_1 = CursorVec {
            inner: Mutex::new(&mut back_vector_1),
        };

        const INPUT_0: &[u8] = &[0, 1, 1];
        const INPUT_1: &[u8] = &[2, 5, 7];

        let mut backed = VecBackedArray::new();
        backed
            .append(INPUT_0, back_vector_0, BincodeCoder::default())
            .unwrap();
        backed
            .append(INPUT_1, back_vector_1, BincodeCoder::default())
            .unwrap();

        let collected = backed.chunk_iter().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(collected[0].as_ref(), INPUT_0);
        assert_eq!(collected[1].as_ref(), INPUT_1);
        assert_eq!(collected.len(), 2);
    }

    #[test]
    fn item_iteration() {
        let mut back_vector_0 = &mut Cursor::new(Vec::with_capacity(3));
        let back_vector_0 = CursorVec {
            inner: Mutex::new(&mut back_vector_0),
        };
        let mut back_vector_1 = &mut Cursor::new(Vec::with_capacity(3));
        let back_vector_1 = CursorVec {
            inner: Mutex::new(&mut back_vector_1),
        };

        const INPUT_0: &[u8] = &[0, 1, 1];
        const INPUT_1: &[u8] = &[2, 5, 7];

        let mut backed = VecBackedArray::new();
        backed
            .append(INPUT_0, back_vector_0, BincodeCoder::default())
            .unwrap();
        backed
            .append(INPUT_1, back_vector_1, BincodeCoder::default())
            .unwrap();
        let collected = backed.iter().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(*collected[5], 7);
        assert_eq!(*collected[2], 1);
        assert_eq!(collected.len(), 6);
    }

    #[test]
    fn mod_iteration() {
        const FIB: &[u8] = &[0, 1, 1, 5, 7];
        const INPUT_1: &[u8] = &[2, 5, 7];

        cursor_vec!(back_vec_0, backing_store_0);
        cursor_vec!(back_vec_1, backing_store_1);

        // Intentional unsafe access to later peek underlying storage
        let mut backed = VecBackedArray::new();
        unsafe {
            backed
                .append(FIB, &mut *back_vec_0.get(), BincodeCoder::default())
                .unwrap();
        }
        #[cfg(not(miri))]
        assert_eq!(&backing_store_0[backing_store_0.len() - FIB.len()..], FIB);

        // Intentional unsafe access to later peek underlying storage
        unsafe {
            backed
                .append(INPUT_1, &mut *back_vec_1.get(), BincodeCoder::default())
                .unwrap();
        }
        #[cfg(not(miri))]
        assert_eq!(
            &backing_store_1[backing_store_1.len() - INPUT_1.len()..],
            INPUT_1
        );

        let handle = backed.chunk_mut_iter();

        let mut handle_vec = handle.collect::<Result<Vec<_>, _>>().unwrap();
        handle_vec[0][0] = 20;
        handle_vec[0][2] = 30;
        handle_vec[1][0] = 40;

        handle_vec[0].flush().unwrap();
        handle_vec[1].flush().unwrap();
        #[cfg(not(miri))]
        {
            assert_eq!(backing_store_0[backing_store_0.len() - FIB.len()], 20);
            assert_eq!(backing_store_0[backing_store_0.len() - FIB.len() + 2], 30);
            assert_eq!(
                backing_store_0[backing_store_0.len() - FIB.len() + 1],
                FIB[1]
            );
            assert_eq!(backing_store_1[backing_store_1.len() - INPUT_1.len()], 40);
            assert_eq!(
                backing_store_1[backing_store_1.len() - INPUT_1.len() + 1],
                INPUT_1[1]
            );
        }
        drop(handle_vec);

        backed
            .iter()
            .zip([20, 1, 30, 5, 7, 40, 5, 7])
            .for_each(|(back, x)| assert_eq!(*back.unwrap(), x));
    }

    #[test]
    #[cfg(feature = "unsafe_array")]
    fn item_mod_iter() {
        const FIB: &[u8] = &[0, 1, 1, 2, 3, 5];
        const INPUT_1: &[u8] = &[4, 6, 7];

        let after_mod = INPUT_1
            .iter()
            .chain(FIB.iter().skip(INPUT_1.len()))
            .cloned()
            .collect_vec();
        let after_second_mod = INPUT_1.iter().chain(INPUT_1).collect_vec();
        let after_third_mod = INPUT_1.iter().chain(INPUT_1).chain(INPUT_1).collect_vec();

        cursor_vec!(back_vec, back_peek);
        cursor_vec!(back_vec_2, _back_peek_3);

        let mut backed = VecBackedArray::new();
        backed
            .append(
                FIB,
                unsafe { &mut *back_vec.get() },
                BincodeCoder::default(),
            )
            .unwrap();

        // Read checks
        assert_eq!(backed.iter_mut().size_hint(), (FIB.len(), Some(FIB.len())));
        assert_eq!(backed.iter_mut().count(), FIB.len());
        assert_eq!(backed.iter_mut().map(|x| **x.unwrap()).collect_vec(), FIB);

        let mut backed_iter = backed.iter_mut();
        for val in INPUT_1 {
            let mut entry = backed_iter.next().unwrap().unwrap();
            **entry = *val;
        }

        // Backed storage is not modified
        #[cfg(not(miri))]
        assert_eq!(&back_peek[back_peek.len() - FIB.len()..], FIB);

        backed_iter.flush().unwrap();

        // Backed storage is now modified
        #[cfg(not(miri))]
        assert_eq!(back_peek[back_peek.len() - after_mod.len()..], after_mod);

        assert_eq!(
            backed_iter.map(|ent| **ent.unwrap()).collect_vec(),
            FIB.iter().skip(INPUT_1.len()).cloned().collect_vec()
        );

        assert_eq!(
            backed.iter_mut().map(|ent| **ent.unwrap()).collect_vec(),
            after_mod
        );

        let mut backed_iter = backed.iter_mut();
        let first = **backed_iter.next().unwrap().unwrap();
        let second = **backed_iter.next().unwrap().unwrap();
        let third = **backed_iter.next().unwrap().unwrap();
        assert_eq!([first, second, third], INPUT_1);
        for val in INPUT_1 {
            let mut entry = backed_iter.next().unwrap().unwrap();
            **entry = *val;
        }

        #[cfg(not(miri))]
        {
            // USAGE FOR TESTING ONLY
            // This breaks handle guarantees about flush on drop.
            // Also leaks memory.
            std::mem::forget(backed_iter);

            // Backed storage is not modified
            assert_eq!(back_peek[back_peek.len() - after_mod.len()..], after_mod);
        }
        #[cfg(miri)]
        drop(backed_iter);

        assert_eq!(
            backed.iter_mut().map(|x| **x.unwrap()).collect_vec(),
            after_second_mod.iter().map(|x| **x).collect_vec(),
        );

        // Backed storage is updated after drop
        #[cfg(not(miri))]
        assert_eq!(
            back_peek[back_peek.len() - after_mod.len()..],
            after_second_mod.iter().map(|x| **x).collect_vec(),
        );

        backed
            .append(
                INPUT_1,
                unsafe { &mut *back_vec_2.get() },
                BincodeCoder::default(),
            )
            .unwrap();

        // Correctly crosses multiple storage disks
        assert_eq!(
            backed.iter_mut().map(|x| **x.unwrap()).collect_vec(),
            after_third_mod.iter().map(|x| **x).collect_vec(),
        );
    }

    #[test]
    fn length_checking() {
        let mut back_vector_0 = &mut Cursor::new(Vec::with_capacity(3));
        let back_vector_0 = CursorVec {
            inner: Mutex::new(&mut back_vector_0),
        };
        let mut back_vector_1 = &mut Cursor::new(Vec::with_capacity(3));
        let back_vector_1 = CursorVec {
            inner: Mutex::new(&mut back_vector_1),
        };

        const INPUT_0: &[u8] = &[0, 1, 1];
        const INPUT_1: &[u8] = &[2, 5, 7];

        let mut backed = VecBackedArray::new();
        backed
            .append(INPUT_0, back_vector_0, BincodeCoder::default())
            .unwrap();
        backed
            .append_memory(INPUT_1, back_vector_1, BincodeCoder::default())
            .unwrap();

        assert_eq!(backed.len(), 6);
        assert_eq!(backed.loaded_len(), 3);
        backed.shrink_to_query(&[0]);
        assert_eq!(backed.loaded_len(), 0);
        backed.get(0).unwrap();
        assert_eq!(backed.loaded_len(), 3);
        backed.clear_chunk(1);
        assert_eq!(backed.loaded_len(), 3);
        backed.clear_chunk(0);
        assert_eq!(backed.loaded_len(), 0);
        [0, 4].into_iter().map(|x| backed.get(x)).collect_vec();
        assert_eq!(backed.loaded_len(), 6);
        backed.shrink_to_query(&[4]);
        assert_eq!(backed.loaded_len(), 3);
        backed.clear_memory();
        assert_eq!(backed.loaded_len(), 0);
    }
}
