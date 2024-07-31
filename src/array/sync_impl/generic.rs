use std::{
    fmt::Debug,
    iter::{FusedIterator, Peekable},
    mem::transmute,
    ops::{Deref, DerefMut, Range},
    sync::{Arc, Mutex},
};

use crate::{
    entry::{sync_impl::BackedEntryMut, BackedEntry},
    utils::{BorrowExtender, BorrowExtenderMut, BorrowNest, NestDeref},
};

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
impl<K: Container<Data = Range<usize>>, E: BackedEntryContainerNestedRead> BackedArray<K, E> {
    /// Implementor for [`Self::get`] that retains type information.
    #[allow(clippy::type_complexity)]
    fn internal_get(
        &self,
        idx: usize,
    ) -> Result<
        BorrowNest<E::Ref<'_>, &E::Unwrapped, <E::Unwrapped as Container>::Ref<'_>>,
        BackedArrayError<E::ReadError>,
    > {
        let loc = internal_idx(self.keys.c_ref().as_ref(), idx)
            .ok_or(BackedArrayError::OutsideEntryBounds(idx))?;

        let wrapped_container = self
            .entries
            .c_get(loc.entry_idx)
            .ok_or(BackedArrayError::OutsideEntryBounds(idx))?;

        // Need to keep the handle to the container alive, in case it
        // invalidates borrowed content when dropped.
        let entry = BorrowExtender::try_new(wrapped_container, |wrapped_container| {
            // Opening layers of abstraction, no transformations.
            let wrapped_container: &BackedEntry<_, _, _> =
                BackedEntryContainer::get_ref(wrapped_container.deref());

            // Use the wrapped_container pointer once, to load the backing.
            let inner_container = wrapped_container.load().map_err(BackedArrayError::Coder)?;

            // `wrapped_container` is borrowed from self.entries, and held
            // for the entire returned `'a` lifetime by BorrowExtender. The
            // pointer that load is called against is only used once, to get an
            // inner container value that is valid as long as
            // `wrapped_container` is held. Transmute can then be used to
            // extend the validity of this reference, which is valid for the
            // lifetime of this struct.
            //
            // This is actually unsafe until function return. I have not found
            // a way to make the compiler enforce the second borrow extending
            // block.
            Ok(unsafe { transmute::<&E::Unwrapped, &'_ E::Unwrapped>(inner_container) })
        })?;

        // Also need to keep this handle to an unwrapped container alive in
        // the return.
        BorrowExtender::try_new(entry, |entry| {
            entry
                .c_get(loc.inside_entry_idx)
                .ok_or(BackedArrayError::OutsideEntryBounds(idx))
        })
    }

    /// [`Self::get`] that works on containers that don't directly dereference
    /// to slices.
    pub fn generic_get(
        &self,
        idx: usize,
    ) -> Result<impl Deref<Target = E::InnerData> + '_, BackedArrayError<E::ReadError>> {
        Ok(NestDeref::from(self.internal_get(idx)?))
    }
}

// ---------- Iterator Returns ---------- //

/// Iterates over a backed array, returning each item in order.
///
/// To keep an accurate size count, failed reads will not be retried.
/// This will keep each disk loaded after pulling data from it.
/// Stepping by > 1 with `nth` implementation may skip loading a disk.
#[derive(Debug)]
pub struct BackedArrayGenericIter<'a, K, E> {
    backed: &'a BackedArray<K, E>,
    pos: usize,
}

impl<'a, K, E> BackedArrayGenericIter<'a, K, E> {
    fn new(backed: &'a BackedArray<K, E>) -> Self {
        Self { backed, pos: 0 }
    }
}

impl<'a, K: Container<Data = Range<usize>>, E: BackedEntryContainerNestedRead> Iterator
    for BackedArrayGenericIter<'a, K, E>
{
    type Item = Result<
        NestDeref<E::Ref<'a>, &'a E::Unwrapped, <E::Unwrapped as Container>::Ref<'a>>,
        E::ReadError,
    >;

    fn next(&mut self) -> Option<Self::Item> {
        self.nth(0)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.pos += n;
        match self.backed.internal_get(self.pos) {
            Ok(val) => {
                self.pos += 1;
                Some(Ok(NestDeref::from(val)))
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
    for BackedArrayGenericIter<'_, K, E>
{
}

/// Iterates over a backed array, returning each item in order.
///
/// To keep an accurate size count, failed reads will not be retried.
/// This will keep each disk loaded after pulling data from it.
/// Stepping by > 1 with `nth` implementation may skip loading a disk.
pub struct BackedArrayGenericIterMut<'a, K: Container + 'a, E: BackedEntryContainerNestedAll + 'a> {
    pos: usize,
    len: usize,
    keys: BorrowExtender<<K as Container>::RefSlice<'a>, Peekable<std::slice::Iter<'a, K::Data>>>,
    entries: BorrowExtenderMut<<E as Container>::MutSlice<'a>, std::slice::IterMut<'a, E::Data>>,
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

impl<'a, K: Container<Data = Range<usize>>, E: BackedEntryContainerNestedAll>
    BackedArrayGenericIterMut<'a, K, E>
{
    fn new(backed: &'a mut BackedArray<K, E>) -> Self {
        let len = backed.keys.c_ref().as_ref().last().unwrap_or(&(0..0)).end;
        let mut handles = Vec::with_capacity(backed.chunks_len());
        let keys = BorrowExtender::new(backed.keys.c_ref(), |keys| {
            // The keys reference is valid as long for the life of this struct,
            // which is valid as long as `Self` is valid.
            let keys = unsafe { transmute::<&K::RefSlice<'_>, &'a K::RefSlice<'a>>(keys) };
            keys.as_ref().iter().peekable()
        });
        let mut entries = BorrowExtenderMut::new(backed.entries.c_mut(), |ent| {
            // The entries reference is valid as long for the life of this
            // struct, which is valid as long as `Self` is valid.
            let ent = unsafe { transmute::<&mut E::MutSlice<'_>, &'a mut E::MutSlice<'a>>(ent) };
            ent.as_mut().iter_mut()
        });

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

impl<K: Container, E: BackedEntryContainerNestedAll> Drop for BackedArrayGenericIterMut<'_, K, E> {
    fn drop(&mut self) {
        for h in self.handles.iter_mut() {
            if let Some(h) = Arc::get_mut(h) {
                let mut h = h.get_mut().unwrap();
                if let Ok(h) = h.deref_mut() {
                    h.flush()
                        .map_err(|_| "Failed to drop handle in BackedArrayGenericIterMut!")
                        .unwrap();
                }
            }
        }
    }
}

impl<'a, K: Container<Data = Range<usize>> + 'a, E: BackedEntryContainerNestedAll + 'a> Iterator
    for BackedArrayGenericIterMut<'a, K, E>
where
    E::Unwrapped: 'a,
    E::ReadError: 'a,
    E::OnceWrapper: 'a,
{
    type Item = Result<
        CountedHandle<
            'a,
            BackedEntry<E::OnceWrapper, E::Disk, E::Coder>,
            <E::Unwrapped as Container>::Mut<'a>,
        >,
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

        match &mut *handle_locked {
            Ok(h) => {
                // The data is valid for the lifetime of the underlying
                // handle, which is guarded by an Arc, not the iterator struct.
                let h = unsafe {
                    transmute::<
                        &mut BackedEntryMut<'a, BackedEntry<E::OnceWrapper, E::Disk, E::Coder>>,
                        &'a mut BackedEntryMut<'a, BackedEntry<E::OnceWrapper, E::Disk, E::Coder>>,
                    >(h)
                };
                h.c_get_mut(inner_pos)
                    .map(|h| Ok(CountedHandle::new(handle.clone(), h)))
            }
            Err(e) => {
                // The error is valid for the lifetime of the underlying
                // handle, which is guarded by an Arc, not the iterator struct.
                let e = unsafe { transmute::<&E::ReadError, &'a E::ReadError>(e) };
                Some(Err(e))
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.len - self.pos;
        (remaining, Some(remaining))
    }
}

impl<'a, K: Container<Data = Range<usize>> + 'a, E: BackedEntryContainerNestedAll + 'a>
    FusedIterator for BackedArrayGenericIterMut<'a, K, E>
where
    E::Unwrapped: 'a,
    E::ReadError: 'a,
    E::OnceWrapper: 'a,
{
}

impl<K: Container<Data = Range<usize>>, E: BackedEntryContainerNestedRead> BackedArray<K, E> {
    /// [`Self::iter`] that works on containers that don't directly
    /// dereference to slices.
    pub fn generic_iter(&self) -> BackedArrayGenericIter<'_, K, E> {
        BackedArrayGenericIter::new(self)
    }

    /// [`Self::chunk_iter`] that works on containers that don't directly
    /// dereference to slices.
    pub fn generic_chunk_iter(
        &self,
    ) -> impl Iterator<Item = Result<impl Deref<Target = &E::Unwrapped>, E::ReadError>> {
        self.entries.ref_iter().map(|arr| {
            BorrowExtender::try_new(arr, |arr| {
                let loaded_entry = arr.as_ref().get_ref().load()?;

                // The loaded value is valid as long as the BorrowExtender is
                // valid, since it keeps the entry alive.
                Ok(unsafe { transmute::<&E::Unwrapped, &'_ E::Unwrapped>(loaded_entry) })
            })
        })
    }
}

impl<K: Container<Data = Range<usize>>, E: BackedEntryContainerNestedAll> BackedArray<K, E> {
    /// [`Self::iter_mut`] that works on containers that don't directly
    /// dereference to slices.
    pub fn generic_iter_mut<'a>(&'a mut self) -> BackedArrayGenericIterMut<'a, K, E>
    where
        E::Unwrapped: 'a,
        E::ReadError: 'a,
    {
        BackedArrayGenericIterMut::new(self)
    }

    /// [`Self::chunk_mut_iter`] that works on containers that don't directly
    /// dereference to slices.
    pub fn generic_chunk_mut_iter(
        &mut self,
    ) -> impl Iterator<
        Item = Result<
            impl AsMut<BackedEntryMut<'_, BackedEntry<E::OnceWrapper, E::Disk, E::Coder>>>,
            E::ReadError,
        >,
    > {
        self.entries.mut_iter().map(|arr| {
            let arr = BorrowExtenderMut::new(arr, |arr| {
                let arr: *mut _ = arr.as_mut().get_mut();
                unsafe { &mut *arr }
            });
            BorrowExtenderMut::try_new(arr, |arr| {
                let arr: *mut _ = arr;
                unsafe { &mut *arr }.mut_handle()
            })
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

        assert_eq!(*backed.generic_get(0).unwrap(), 0);
        assert_eq!(*backed.generic_get(4).unwrap(), 3);
        assert_eq!(
            (
                *backed.generic_get(0).unwrap(),
                *backed.generic_get(4).unwrap()
            ),
            (0, 3)
        );

        [0, 2, 4, 5, 5, 1, 0, 5]
            .into_iter()
            .for_each(|x| assert_eq!(*backed.generic_get(x).unwrap(), combined[x]));
    }

    #[test]
    fn out_of_bounds_access() {
        let mut back_vector = &mut Cursor::new(Vec::with_capacity(3));
        let back_vector = CursorVec {
            inner: Mutex::new(&mut back_vector),
        };

        const INPUT: &[u8] = &[0, 1, 1];

        let mut backed = VecBackedArray::new();
        backed
            .append(INPUT, back_vector, BincodeCoder::default())
            .unwrap();

        assert!(backed.generic_get(0).is_ok());
        assert!(backed.generic_get(10).is_err());
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

        let collected = backed
            .generic_chunk_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
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
        let collected = backed
            .generic_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
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
            .generic_iter()
            .zip([20, 1, 30, 5, 7, 40, 5, 7])
            .for_each(|(back, x)| assert_eq!(*back.unwrap(), x));
    }

    #[test]
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
        assert_eq!(
            backed.generic_iter_mut().size_hint(),
            (FIB.len(), Some(FIB.len()))
        );
        assert_eq!(backed.generic_iter_mut().count(), FIB.len());
        assert_eq!(
            backed
                .generic_iter_mut()
                .map(|x| **x.unwrap())
                .collect_vec(),
            FIB
        );

        let mut backed_iter = backed.generic_iter_mut();
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
            backed
                .generic_iter_mut()
                .map(|ent| **ent.unwrap())
                .collect_vec(),
            after_mod
        );

        let mut backed_iter = backed.generic_iter_mut();
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
            backed
                .generic_iter_mut()
                .map(|x| **x.unwrap())
                .collect_vec(),
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
            backed
                .generic_iter_mut()
                .map(|x| **x.unwrap())
                .collect_vec(),
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
        backed.generic_get(0).unwrap();
        assert_eq!(backed.loaded_len(), 3);
        backed.clear_chunk(1);
        assert_eq!(backed.loaded_len(), 3);
        backed.clear_chunk(0);
        assert_eq!(backed.loaded_len(), 0);
        [0, 4]
            .into_iter()
            .map(|x| backed.generic_get(x))
            .collect_vec();
        assert_eq!(backed.loaded_len(), 6);
        backed.shrink_to_query(&[4]);
        assert_eq!(backed.loaded_len(), 3);
        backed.clear_memory();
        assert_eq!(backed.loaded_len(), 0);
    }
}
