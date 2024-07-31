use std::{borrow::Borrow, iter::FusedIterator, mem::transmute, pin::Pin, sync::Arc};

use futures::Future;

use crate::entry::{disks::AsyncReadDisk, BackedEntryAsync};

use super::{
    super::{container::Container, internal_idx, BackedArray, BackedArrayError},
    BackedEntryContainerNestedAsyncRead,
};

/// Read implementations
impl<K: Container<Data = usize>, E: BackedEntryContainerNestedAsyncRead> BackedArray<K, E>
where
    E: AsRef<[E::Data]>,
    E::Unwrapped: AsRef<[E::InnerData]>,
{
    /// Async version of [`Self::get`].
    pub async fn a_get(
        &self,
        idx: usize,
    ) -> Result<&E::InnerData, BackedArrayError<E::AsyncReadError>> {
        let loc = internal_idx(
            self.key_starts.c_ref().as_ref(),
            self.key_ends.c_ref().as_ref(),
            idx,
        )
        .ok_or(BackedArrayError::OutsideEntryBounds(idx))?;

        let wrapped_container = &self.entries.as_ref()[loc.entry_idx];
        let backed_entry: &BackedEntryAsync<_, _, _> = wrapped_container.as_ref();
        let entry = backed_entry
            .a_load()
            .await
            .map_err(BackedArrayError::Coder)?;
        Ok(&entry.as_ref()[loc.inside_entry_idx])
    }
}

// ---------- Stream Returns ---------- //

/// Iterates over a backed array, returning each item future in order.
///
/// See [`BackedArrayFutIterSend`] for the [`Send`] version.
///
/// To keep an accurate size count, failed reads will not be retried.
/// This will keep each disk loaded after pulling data from it.
/// Stepping by > 1 with `nth` implementation may skip loading a disk.
#[derive(Debug)]
pub struct BackedArrayFutIter<'a, K, E> {
    backed: &'a BackedArray<K, E>,
    pos: usize,
}

/// [`BackedArrayFutIter`], but returns are [`Send`].
///
/// Since closure returns are anonymous, and Iterator requires a concrete type,
/// the future is returned as a [`Box<dyn Future>`]. This strips type information,
/// so having a `+ Send` version requires a different return type than a
/// `Send?` version.
#[derive(Debug)]
pub struct BackedArrayFutIterSend<K, E> {
    backed: Arc<BackedArray<K, E>>,
    pos: usize,
}

impl<'a, K, E> BackedArrayFutIter<'a, K, E> {
    fn new(backed: &'a BackedArray<K, E>) -> Self {
        Self { backed, pos: 0 }
    }
}

impl<K, E> BackedArrayFutIterSend<K, E> {
    fn new(backed: Arc<BackedArray<K, E>>) -> Self {
        Self { backed, pos: 0 }
    }
}

impl<'a, K: Container<Data = usize>, E: BackedEntryContainerNestedAsyncRead> Iterator
    for BackedArrayFutIter<'a, K, E>
where
    E: AsRef<[E::Data]>,
    E::Unwrapped: AsRef<[E::InnerData]>,
{
    type Item = Pin<Box<dyn Future<Output = Result<&'a E::InnerData, E::AsyncReadError>> + 'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        self.nth(0)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        let cur_pos = self.pos + n;
        self.pos += n + 1;
        let fut = self.backed.a_get(cur_pos);
        if cur_pos < self.backed.len() {
            Some(Box::pin(async move {
                match fut.await {
                    Ok(val) => Ok(val),
                    Err(e) => match e {
                        BackedArrayError::OutsideEntryBounds(_) => {
                            panic!("Not possible to be outside entry bounds")
                        }
                        BackedArrayError::Coder(c) => Err(c),
                    },
                }
            }))
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.backed.len() - self.pos;
        (remaining, Some(remaining))
    }
}

impl<K: Container<Data = usize>, E: BackedEntryContainerNestedAsyncRead> Iterator
    for BackedArrayFutIterSend<K, E>
where
    K: Send + Sync + 'static,
    for<'b> K::Ref<'b>: Send + Sync,
    E: Send + Sync + 'static,
    E::Coder: Send + Sync,
    E::Disk: Send + Sync,
    <E::Disk as AsyncReadDisk>::ReadDisk: Send + Sync,
    E::OnceWrapper: Send + Sync,
    for<'b> E::Ref<'b>: Send + Sync,
    for<'b> <E::Unwrapped as Container>::Ref<'b>: Send + Sync,
    E: AsRef<[E::Data]>,
    E::Unwrapped: AsRef<[E::InnerData]>,
{
    type Item = Pin<
        Box<dyn Future<Output = Result<&'static E::InnerData, E::AsyncReadError>> + Send + 'static>,
    >;

    fn next(&mut self) -> Option<Self::Item> {
        self.nth(0)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        let cur_pos = self.pos + n;
        self.pos += n + 1;

        // This version of the function clones the Arc and moves it inside
        // the async closure, in order to ensure lifetime validity.
        let backed = self.backed.clone();
        if cur_pos < self.backed.len() {
            Some(Box::pin(async move {
                // Lifetime shenanigans: since this uses an owned Arc and the
                // underlying data is static, this data will be valid for this
                // future. But the compiler can't solve for that.
                let backed = unsafe {
                    transmute::<&BackedArray<K, E>, &'static BackedArray<K, E>>(backed.borrow())
                };
                let fut = backed.a_get(cur_pos);

                match fut.await {
                    Ok(val) => Ok(val),
                    Err(e) => match e {
                        BackedArrayError::OutsideEntryBounds(_) => {
                            panic!("Not possible to be outside entry bounds")
                        }
                        BackedArrayError::Coder(c) => Err(c),
                    },
                }
            }))
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.backed.len() - self.pos;
        (remaining, Some(remaining))
    }
}

impl<'a, K: Container<Data = usize>, E: BackedEntryContainerNestedAsyncRead> FusedIterator
    for BackedArrayFutIter<'a, K, E>
where
    E: AsRef<[E::Data]>,
    E::Unwrapped: AsRef<[E::InnerData]>,
{
}

impl<K: Container<Data = usize>, E: BackedEntryContainerNestedAsyncRead> FusedIterator
    for BackedArrayFutIterSend<K, E>
where
    K: Send + Sync + 'static,
    for<'b> K::Ref<'b>: Send + Sync,
    E: Send + Sync + 'static,
    E::Coder: Send + Sync,
    E::Disk: Send + Sync,
    <E::Disk as AsyncReadDisk>::ReadDisk: Send + Sync,
    E::OnceWrapper: Send + Sync,
    for<'b> E::Ref<'b>: Send + Sync,
    for<'b> <E::Unwrapped as Container>::Ref<'b>: Send + Sync,
    E: AsRef<[E::Data]>,
    E::Unwrapped: AsRef<[E::InnerData]>,
{
}

impl<K: Container<Data = usize>, E: BackedEntryContainerNestedAsyncRead> BackedArray<K, E>
where
    E: AsRef<[E::Data]>,
    E::Unwrapped: AsRef<[E::InnerData]>,
{
    /// Future iterator over each backed item.
    ///
    /// Can be converted to a [`Stream`](`futures::Stream`) with [`futures::stream::iter`]. This is not a
    /// stream by default because [`Stream`](`futures::Stream`) does not have the iterator methods
    /// that allow efficient implementations.
    ///
    /// Use [`Self::stream_send`] for `+ Send` bounds.
    pub fn stream(&self) -> BackedArrayFutIter<K, E> {
        BackedArrayFutIter::new(self)
    }

    /// Version of [`Self::stream`] with `+ Send` bounds.
    ///
    /// If this type owns the disk (all library disks are owned), wrapping
    /// in an Arc and calling with `BackedArray::stream_send(&this)` satisfies
    /// all of the bounds.
    pub fn stream_send(this: &Arc<Self>) -> BackedArrayFutIterSend<K, E>
    where
        K: Send + Sync + 'static,
        for<'b> K::Ref<'b>: Send + Sync,
        E: Send + Sync + 'static,
        E::Coder: Send + Sync,
        E::Disk: Send + Sync,
        <E::Disk as AsyncReadDisk>::ReadDisk: Send + Sync,
        E::OnceWrapper: Send + Sync,
        for<'b> E::Ref<'b>: Send + Sync,
        for<'b> <E::Unwrapped as Container>::Ref<'b>: Send + Sync,
    {
        BackedArrayFutIterSend::new(this.clone())
    }
}

impl<K: Container<Data = usize>, E: BackedEntryContainerNestedAsyncRead> BackedArray<K, E>
where
    E: AsRef<[E::Data]>,
{
    /// Future iterator over each chunk.
    ///
    /// Can be converted to a [`Stream`](`futures::Stream`) with [`futures::stream::iter`]. This is not a
    /// stream by default because [`Stream`](`futures::Stream`) does not have the iterator methods
    /// that allow efficient implementations.
    pub fn chunk_stream(
        &self,
    ) -> impl Iterator<Item = impl Future<Output = Result<&E::Unwrapped, E::AsyncReadError>>> {
        self.entries
            .as_ref()
            .iter()
            .map(|arr| async { arr.as_ref().a_load().await })
    }
}

#[cfg(test)]
#[cfg(feature = "async_bincode")]
mod tests {
    use std::{
        io::Cursor,
        sync::{Arc, Mutex},
    };

    use futures::{stream, StreamExt, TryStreamExt};
    use tokio::join;

    use crate::{
        entry::formats::AsyncBincodeCoder,
        test_utils::{CursorVec, OwnedCursorVec},
    };

    use super::super::*;

    #[tokio::test]
    async fn multiple_retrieve() {
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

        let mut backed = AsyncVecBackedArray::new();
        backed
            .a_append(INPUT_0, back_vector_0, AsyncBincodeCoder::default())
            .await
            .unwrap();
        backed
            .a_append_memory(INPUT_1, back_vector_1, AsyncBincodeCoder::default())
            .await
            .unwrap();

        assert_eq!(*backed.a_get(0).await.unwrap(), 0);
        assert_eq!(*backed.a_get(4).await.unwrap(), 3);

        let (first, second) = join!(async { backed.a_get(0).await.unwrap() }, async {
            backed.a_get(4).await.unwrap()
        });
        assert_eq!((*first, *second), (0, 3));

        for x in [0, 2, 4, 5, 5, 2, 0, 5] {
            assert_eq!(*backed.a_get(x).await.unwrap(), combined[x])
        }
    }

    #[tokio::test]
    async fn out_of_bounds_access() {
        let mut back_vector = &mut Cursor::new(Vec::with_capacity(3));
        let back_vector = CursorVec {
            inner: Mutex::new(&mut back_vector),
        };

        const INPUT: &[u8] = &[0, 1, 1];

        let mut backed = AsyncVecBackedArray::new();
        backed
            .a_append(INPUT, back_vector, AsyncBincodeCoder::default())
            .await
            .unwrap();

        assert!(backed.a_get(0).await.is_ok());
        assert!(backed.a_get(10).await.is_err());
    }

    #[tokio::test]
    async fn chunk_iteration() {
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

        let mut backed = AsyncVecBackedArray::new();
        backed
            .a_append(INPUT_0, back_vector_0, AsyncBincodeCoder::default())
            .await
            .unwrap();
        backed
            .a_append(INPUT_1, back_vector_1, AsyncBincodeCoder::default())
            .await
            .unwrap();

        let collected = stream::iter(backed.chunk_stream())
            .then(|x| x)
            .try_collect::<Vec<_>>()
            .await
            .unwrap();
        assert_eq!(collected[0].as_ref(), INPUT_0);
        assert_eq!(collected[1].as_ref(), INPUT_1);
        assert_eq!(collected.len(), 2);
    }

    #[tokio::test]
    async fn item_iteration() {
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

        let mut backed = AsyncVecBackedArray::new();
        backed
            .a_append(INPUT_0, back_vector_0, AsyncBincodeCoder::default())
            .await
            .unwrap();
        backed
            .a_append(INPUT_1, back_vector_1, AsyncBincodeCoder::default())
            .await
            .unwrap();
        let collected = stream::iter(backed.stream())
            .then(|x| x)
            .try_collect::<Vec<&u8>>()
            .await
            .unwrap();
        assert_eq!(collected[5], &7);
        assert_eq!(collected[2], &1);
        assert_eq!(collected.len(), 6);
    }

    #[test]
    fn parallel_item_iteration() {
        // Need to avoid use a runtime without I/O for Miri compatibility
        tokio::runtime::Builder::new_multi_thread()
            .build()
            .unwrap()
            .block_on(async {
                // Leak cursors to get static lifetimes
                let back_0 = Cursor::new(Vec::new());
                let back_vector_0 = OwnedCursorVec::new(back_0);
                let back_1 = Cursor::new(Vec::new());
                let back_vector_1 = OwnedCursorVec::new(back_1);

                const INPUT_0: &[u8] = &[0, 1, 1];
                const INPUT_1: &[u8] = &[2, 5, 7];

                let mut backed = AsyncVecBackedArray::new();
                backed
                    .a_append(INPUT_0, back_vector_0, AsyncBincodeCoder::default())
                    .await
                    .unwrap();
                backed
                    .a_append(INPUT_1, back_vector_1, AsyncBincodeCoder::default())
                    .await
                    .unwrap();
                let backed = Arc::new(backed);
                let collected = stream::iter(BackedArray::stream_send(&backed))
                    .map(|x| async { tokio::task::spawn(x).await.unwrap() })
                    .buffered(backed.len())
                    .try_collect::<Vec<&u8>>()
                    .await
                    .unwrap();
                assert_eq!(collected[5], &7);
                assert_eq!(collected[2], &1);
                assert_eq!(collected.len(), 6);
            });
    }

    #[tokio::test]
    async fn length_checking() {
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

        let mut backed = AsyncVecBackedArray::new();
        backed
            .a_append(INPUT_0, back_vector_0, AsyncBincodeCoder::default())
            .await
            .unwrap();
        backed
            .a_append_memory(INPUT_1, back_vector_1, AsyncBincodeCoder::default())
            .await
            .unwrap();

        assert_eq!(backed.len(), 6);
        assert_eq!(backed.loaded_len(), 3);
        backed.shrink_to_query(&[0]);
        assert_eq!(backed.loaded_len(), 0);
        backed.a_get(0).await.unwrap();
        assert_eq!(backed.loaded_len(), 3);
        backed.clear_chunk(1);
        assert_eq!(backed.loaded_len(), 3);
        backed.clear_chunk(0);
        assert_eq!(backed.loaded_len(), 0);
        for x in [0, 4] {
            backed.a_get(x).await.unwrap();
        }
        assert_eq!(backed.loaded_len(), 6);
        backed.shrink_to_query(&[4]);
        assert_eq!(backed.loaded_len(), 3);
        backed.clear_memory();
        assert_eq!(backed.loaded_len(), 0);

        // Future should not load anything until actually executed.
        let fut = backed.a_get(0);
        assert_eq!(backed.loaded_len(), 0);
        fut.await.unwrap();
    }
}
