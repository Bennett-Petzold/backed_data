use std::{iter::FusedIterator, ops::Range, pin::Pin, sync::Arc};

use futures::{stream, Future, Stream, StreamExt};
use serde::{Deserialize, Serialize};

use crate::{
    entry::{
        async_impl::OnceCellWrap,
        disks::{AsyncReadDisk, AsyncWriteDisk},
        formats::{AsyncDecoder, AsyncEncoder},
        BackedEntryAsync,
    },
    utils::BorrowExtender,
};

use super::{
    container::{BackedEntryContainer, Container, ResizingContainer},
    internal_idx, multiple_internal_idx_strict, BackedArray, BackedArrayError,
};

/// A [`BackedEntryContainer`] inside a [`Container`].
///
/// For internal use, reduces size of generics boilerplate.
pub trait BackedEntryContainerNestedAsync:
    Container<
    Data: BackedEntryContainer<
        Container = Self::OnceWrapper,
        Disk = Self::Disk,
        Coder = Self::Coder,
    > + From<BackedEntryAsync<Self::Unwrapped, Self::Disk, Self::Coder>>
              + AsRef<BackedEntryAsync<Self::Unwrapped, Self::Disk, Self::Coder>>
              + AsMut<BackedEntryAsync<Self::Unwrapped, Self::Disk, Self::Coder>>,
>
{
    type InnerData;
    type OnceWrapper: OnceCellWrap<T = Self::Unwrapped>;
    type Unwrapped: Container<Data = Self::InnerData>;
    type Disk;
    type Coder;
}

/// Auto-implement trait to wrap intended generics.
impl<T> BackedEntryContainerNestedAsync for T
where
    T: Container<Data: BackedEntryContainer>,
    <<T as Container>::Data as BackedEntryContainer>::Container: OnceCellWrap<T: Container>,
    <T as Container>::Data: From<
        BackedEntryAsync<
            <<T::Data as BackedEntryContainer>::Container as OnceCellWrap>::T,
            <T::Data as BackedEntryContainer>::Disk,
            <T::Data as BackedEntryContainer>::Coder,
        >,
    >,
    <T as Container>::Data: AsRef<
        BackedEntryAsync<
            <<T::Data as BackedEntryContainer>::Container as OnceCellWrap>::T,
            <T::Data as BackedEntryContainer>::Disk,
            <T::Data as BackedEntryContainer>::Coder,
        >,
    >,
    <T as Container>::Data: AsMut<
        BackedEntryAsync<
            <<T::Data as BackedEntryContainer>::Container as OnceCellWrap>::T,
            <T::Data as BackedEntryContainer>::Disk,
            <T::Data as BackedEntryContainer>::Coder,
        >,
    >,
{
    type InnerData =
        <<<T::Data as BackedEntryContainer>::Container as OnceCellWrap>::T as Container>::Data;
    type OnceWrapper = <T::Data as BackedEntryContainer>::Container;
    type Unwrapped = <<T::Data as BackedEntryContainer>::Container as OnceCellWrap>::T;
    type Disk = <T::Data as BackedEntryContainer>::Disk;
    type Coder = <T::Data as BackedEntryContainer>::Coder;
}

/// [`BackedEntryContainerNested`] variant.
///
/// For internal use, reduces size of generics boilerplate.
pub trait BackedEntryContainerNestedAsyncRead:
    BackedEntryContainerNestedAsync<
    Unwrapped: for<'de> Deserialize<'de> + Send + Sync,
    Disk: AsyncReadDisk,
    Coder: AsyncDecoder<<Self::Disk as AsyncReadDisk>::ReadDisk, Error = Self::AsyncReadError>,
>
{
    type AsyncReadError;
}

impl<T> BackedEntryContainerNestedAsyncRead for T
where
    T: BackedEntryContainerNestedAsync<
        Unwrapped: for<'de> Deserialize<'de> + Send + Sync,
        Disk: AsyncReadDisk,
        Coder: AsyncDecoder<<Self::Disk as AsyncReadDisk>::ReadDisk>,
    >,
{
    type AsyncReadError =
        <Self::Coder as AsyncDecoder<<Self::Disk as AsyncReadDisk>::ReadDisk>>::Error;
}

/// [`BackedEntryContainerNested`] variant.
///
/// For internal use, reduces size of generics boilerplate.
pub trait BackedEntryContainerNestedAsyncWrite:
    BackedEntryContainerNestedAsync<
    Unwrapped: Serialize + Send + Sync,
    Disk: AsyncWriteDisk,
    Coder: AsyncEncoder<<Self::Disk as AsyncWriteDisk>::WriteDisk, Error = Self::AsyncWriteError>,
>
{
    type AsyncWriteError;
}

impl<T> BackedEntryContainerNestedAsyncWrite for T
where
    T: BackedEntryContainerNestedAsync<
        Unwrapped: Serialize + Send + Sync,
        Disk: AsyncWriteDisk,
        Coder: AsyncEncoder<<Self::Disk as AsyncWriteDisk>::WriteDisk>,
    >,
{
    type AsyncWriteError =
        <Self::Coder as AsyncEncoder<<Self::Disk as AsyncWriteDisk>::WriteDisk>>::Error;
}

/// [`BackedEntryContainerNested`] variant.
///
/// For internal use, reduces size of generics boilerplate.
pub trait BackedEntryContainerNestedAsyncAll:
    BackedEntryContainerNestedAsyncRead + BackedEntryContainerNestedAsyncWrite
{
}

impl<T> BackedEntryContainerNestedAsyncAll for T where
    T: BackedEntryContainerNestedAsyncRead + BackedEntryContainerNestedAsyncWrite
{
}

/// Immutable open for a reference to a [`BackedEntryContainer`].
macro_rules! a_open_ref {
    ($x:expr) => {
        $x.as_ref().as_ref().as_ref()
    };
}

pub(crate) use a_open_ref;

pub type AsyncVecBackedArray<T, Disk, Coder> =
    BackedArray<Vec<Range<usize>>, Vec<BackedEntryAsync<Box<[T]>, Disk, Coder>>>;

/// Read implementations
impl<K: Container<Data = Range<usize>>, E: BackedEntryContainerNestedAsyncRead> BackedArray<K, E> {
    /// Async version of [`Self::get`].
    pub async fn a_get(
        &self,
        idx: usize,
    ) -> Result<<E::Unwrapped as Container>::Ref<'_>, BackedArrayError<E::AsyncReadError>> {
        let loc = internal_idx(self.keys.c_ref().as_ref(), idx)
            .ok_or(BackedArrayError::OutsideEntryBounds(idx))?;

        let entry_container = BorrowExtender::try_new(&self.entries, |entries| {
            entries
                .c_get(loc.entry_idx)
                .ok_or(BackedArrayError::OutsideEntryBounds(idx))
        })?;
        let entry = BorrowExtender::a_try_new(entry_container, |entry_container| async {
            a_open_ref!(entry_container)
                .a_load()
                .await
                .map_err(BackedArrayError::Coder)
        })
        .await?;
        entry
            .c_get(loc.inside_entry_idx)
            .ok_or(BackedArrayError::OutsideEntryBounds(idx))
    }

    /// Async version of [`Self::get_multiple`].
    pub fn a_get_multiple<'a, I>(
        &'a self,
        idxs: I,
    ) -> impl Stream<
        Item = Result<<E::Unwrapped as Container>::Ref<'_>, BackedArrayError<E::AsyncReadError>>,
    >
    where
        I: IntoIterator<Item = usize> + Clone + 'a,
    {
        stream::iter(multiple_internal_idx_strict(self.keys.c_ref(), idxs.clone()).zip(idxs)).then(
            move |(loc, idx)| async move {
                let loc = loc.ok_or(BackedArrayError::OutsideEntryBounds(idx))?;
                let entry_container = BorrowExtender::try_new(&self.entries, |entries| {
                    entries
                        .c_get(loc.entry_idx)
                        .ok_or(BackedArrayError::OutsideEntryBounds(idx))
                })?;
                let entry = BorrowExtender::a_try_new(entry_container, |entry_container| async {
                    a_open_ref!(entry_container)
                        .a_load()
                        .await
                        .map_err(BackedArrayError::Coder)
                })
                .await?;
                entry
                    .c_get(loc.inside_entry_idx)
                    .ok_or(BackedArrayError::OutsideEntryBounds(idx))
            },
        )
    }
}

/// Write implementations
impl<
        K: ResizingContainer<Data = Range<usize>>,
        E: BackedEntryContainerNestedAsyncWrite + ResizingContainer,
    > BackedArray<K, E>
{
    /// Adds new values by writing them to the backing store.
    ///
    /// Does not keep the values in memory.
    ///
    /// # Example
    /// ```rust
    /// #[cfg(feature = "async_bincode")] {
    /// use backed_data::{
    ///     array::async_impl::AsyncVecBackedArray,
    ///     entry::{
    ///         disks::Plainfile,
    ///         formats::AsyncBincodeCoder,
    ///     }
    /// };
    /// use std::fs::{File, create_dir_all, remove_dir_all, OpenOptions};
    ///
    /// use tokio::runtime::Builder;
    ///
    /// let rt = Builder::new_current_thread().build().unwrap();
    ///
    /// let FILENAME_BASE = std::env::temp_dir().join("example_async_array_append");
    /// let values = ([0, 1, 1],
    ///     [2, 3, 5]);
    ///
    /// create_dir_all(FILENAME_BASE.clone()).unwrap();
    /// let file_0 = FILENAME_BASE.clone().join("_0");
    /// let file_1 = FILENAME_BASE.join("_1");
    /// let mut array: AsyncVecBackedArray<u32, Plainfile, _> = AsyncVecBackedArray::default();
    /// rt.block_on(array.a_append(values.0, file_0.into(), AsyncBincodeCoder {}));
    /// rt.block_on(array.a_append(values.1, file_1.into(), AsyncBincodeCoder {}));
    ///
    /// assert_eq!(rt.block_on(array.a_get(4)).unwrap().as_ref(), &3);
    /// remove_dir_all(FILENAME_BASE).unwrap();
    /// }
    /// ```
    pub async fn a_append<U: Into<E::Unwrapped>>(
        &mut self,
        values: U,
        backing_store: E::Disk,
        coder: E::Coder,
    ) -> Result<&mut Self, E::AsyncWriteError> {
        let values = values.into();

        // End of a range is exclusive
        let start_idx = self
            .keys
            .c_ref()
            .as_ref()
            .last()
            .map(|key_range| key_range.end)
            .unwrap_or(0);
        self.keys.c_push(start_idx..(start_idx + values.c_len()));

        let mut entry = BackedEntryAsync::new(backing_store, coder);
        entry.a_write_unload(values).await?;
        self.entries.c_push(entry.into());
        Ok(self)
    }

    /// [`Self::append`], but keeps values in memory.
    ///
    /// # Example
    /// ```rust
    /// #[cfg(feature = "async_bincode")] {
    ///     use backed_data::{
    ///         array::async_impl::AsyncVecBackedArray,
    ///         entry::{
    ///             disks::Plainfile,
    ///             formats::AsyncBincodeCoder,
    ///         },
    ///     };
    ///     use std::fs::{File, remove_file, OpenOptions};
    ///
    ///     use tokio::runtime::Builder;
    ///
    ///     let rt = Builder::new_current_thread().build().unwrap();
    ///
    ///     let FILENAME = std::env::temp_dir().join("example_async_array_append_memory");
    ///     let values = ([0, 1, 1],
    ///         [2, 3, 5]);
    ///
    ///     let mut array: AsyncVecBackedArray<u32, Plainfile, _> = AsyncVecBackedArray::default();
    ///     rt.block_on(array.a_append_memory(values.0, FILENAME.clone().into(), AsyncBincodeCoder {}));
    ///
    ///     // Overwrite file, making disk pointer for first array invalid
    ///     rt.block_on(array.a_append_memory(values.1, FILENAME.clone().into(), AsyncBincodeCoder {}));
    ///
    ///     assert_eq!(rt.block_on(array.a_get(0)).unwrap().as_ref(), &0);
    ///     remove_file(FILENAME).unwrap();
    /// }
    /// ```
    pub async fn a_append_memory<U: Into<E::Unwrapped>>(
        &mut self,
        values: U,
        backing_store: E::Disk,
        coder: E::Coder,
    ) -> Result<&mut Self, E::AsyncWriteError> {
        let values = values.into();

        // End of a range is exclusive
        let start_idx = self
            .keys
            .c_ref()
            .as_ref()
            .last()
            .map(|key_range| key_range.end)
            .unwrap_or(0);
        self.keys.c_push(start_idx..(start_idx + values.c_len()));
        let mut entry = BackedEntryAsync::new(backing_store, coder);
        entry.a_write(values).await?;
        self.entries.c_push(entry.into());
        Ok(self)
    }
}

// ---------- Stream Returns ---------- //

/// Iterates over a backed array, returning each item future in order.
///
/// See [`BackedArrayFutIterSend`] for the Send + Sync version.
///
/// To keep an accurate size count, failed reads will not be retried.
/// This will keep each disk loaded after pulling data from it.
/// Stepping by > 1 with `nth` implementation may skip loading a disk.
#[derive(Debug)]
pub struct BackedArrayFutIter<'a, K, E> {
    backed: &'a BackedArray<K, E>,
    pos: usize,
}

/// [`BackedArrayFutIter`], but returns are `+ Send`.
///
/// Since closure returns are anonymous, and Iterator requires a concrete type,
/// the future is returned as a Box<dyn Future>. This strips type information,
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

impl<'a, K: Container<Data = Range<usize>>, E: BackedEntryContainerNestedAsyncRead> Iterator
    for BackedArrayFutIter<'a, K, E>
{
    type Item = Pin<
        Box<
            dyn Future<Output = Result<<E::Unwrapped as Container>::Ref<'a>, E::AsyncReadError>>
                + 'a,
        >,
    >;

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

impl<K: Container<Data = Range<usize>>, E: BackedEntryContainerNestedAsyncRead> Iterator
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
{
    type Item = Pin<
        Box<
            dyn Future<
                    Output = Result<<E::Unwrapped as Container>::Ref<'static>, E::AsyncReadError>,
                > + Send
                + 'static,
        >,
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
                let backed_ptr: *const _ = &backed;
                let fut = unsafe { &*backed_ptr }.a_get(cur_pos);

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

impl<'a, K: Container<Data = Range<usize>>, E: BackedEntryContainerNestedAsyncRead> FusedIterator
    for BackedArrayFutIter<'a, K, E>
{
}

impl<K: Container<Data = Range<usize>>, E: BackedEntryContainerNestedAsyncRead> FusedIterator
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
{
}

impl<K: Container<Data = Range<usize>>, E: BackedEntryContainerNestedAsyncRead> BackedArray<K, E> {
    /// Future iterator over each backed item.
    ///
    /// Can be converted to a [`Stream`] with [`stream::iter`]. This is not a
    /// stream by default because [`Stream`] does not have the iterator methods
    /// that allow efficient implementations.
    ///
    /// Use [`Self::stream_send`] for `+ Send` bounds.
    pub fn stream(
        &self,
    ) -> impl Iterator<
        Item = impl Future<Output = Result<<E::Unwrapped as Container>::Ref<'_>, E::AsyncReadError>>,
    > + '_ {
        BackedArrayFutIter::new(self)
    }

    /// Version of [`Self::stream`] with `+ Send` bounds.
    ///
    /// If this type owns the disk (all library disks are owned), wrapping
    /// in an Arc and calling with `BackedArray::stream_send(&this)` satisfies
    /// all of the bounds.
    pub fn stream_send(
        this: &Arc<Self>,
    ) -> impl Iterator<
        Item = impl Future<
            Output = Result<<E::Unwrapped as Container>::Ref<'static>, E::AsyncReadError>,
        > + Send,
    > + '_
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

    /// Future iterator over each chunk.
    ///
    /// Can be converted to a [`Stream`] with [`stream::iter`]. This is not a
    /// stream by default because [`Stream`] does not have the iterator methods
    /// that allow efficient implementations.
    pub fn chunk_stream(
        &self,
    ) -> impl Iterator<Item = impl Future<Output = Result<&E::Unwrapped, E::AsyncReadError>>> {
        self.entries.ref_iter().map(|ent| {
            let ent = BorrowExtender::new(ent, |ent| {
                let ent_ptr: *const _ = ent;
                let ent = unsafe { &*ent_ptr }.as_ref();
                BorrowExtender::new(ent, |ent| {
                    let ent_ptr: *const _ = ent;
                    unsafe { &*ent_ptr }.as_ref()
                })
            });
            ent.a_load()
        })
    }
}

#[cfg(test)]
#[cfg(feature = "async_bincode")]
mod tests {
    use std::{future, io::Cursor, sync::Mutex};

    use stream::TryStreamExt;
    use tokio::join;

    use crate::{
        entry::formats::AsyncBincodeCoder,
        test_utils::{CursorVec, OwnedCursorVec},
    };

    use super::*;

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

        let mut backed = AsyncVecBackedArray::new();
        backed
            .a_append(INPUT_0, back_vector_0, AsyncBincodeCoder {})
            .await
            .unwrap();
        backed
            .a_append_memory(INPUT_1, back_vector_1, AsyncBincodeCoder {})
            .await
            .unwrap();

        assert_eq!(backed.a_get(0).await.unwrap().as_ref(), &0);
        assert_eq!(backed.a_get(4).await.unwrap().as_ref(), &3);

        let (first, second) = join!(async { backed.a_get(0).await.unwrap() }, async {
            backed.a_get(4).await.unwrap()
        });
        assert_eq!((first.as_ref(), second.as_ref()), (&0, &3));

        assert_eq!(
            backed
                .a_get_multiple([0, 2, 4, 5])
                .try_collect::<Vec<_>>()
                .await
                .unwrap(),
            [&0, &1, &3, &5]
        );
        assert_eq!(
            backed
                .a_get_multiple([5, 2, 0, 5])
                .try_collect::<Vec<_>>()
                .await
                .unwrap(),
            [&5, &1, &0, &5]
        );
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
            .a_append(INPUT, back_vector, AsyncBincodeCoder {})
            .await
            .unwrap();

        assert!(backed.a_get(0).await.is_ok());
        assert!(backed.a_get(10).await.is_err());
        assert!(backed
            .a_get_multiple([0, 10])
            .try_collect::<Vec<_>>()
            .await
            .is_err());
        assert_eq!(
            backed
                .a_get_multiple([0, 10])
                .filter(|x| future::ready(x.is_ok()))
                .count()
                .await,
            1
        );
        assert_eq!(
            backed
                .a_get_multiple([20, 10])
                .filter(|x| future::ready(x.is_ok()))
                .count()
                .await,
            0
        );
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
            .a_append(INPUT_0, back_vector_0, AsyncBincodeCoder {})
            .await
            .unwrap();
        backed
            .a_append(INPUT_1, back_vector_1, AsyncBincodeCoder {})
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
            .a_append(INPUT_0, back_vector_0, AsyncBincodeCoder {})
            .await
            .unwrap();
        backed
            .a_append(INPUT_1, back_vector_1, AsyncBincodeCoder {})
            .await
            .unwrap();
        let collected = stream::iter(backed.stream())
            .then(|x| x)
            .try_collect::<Vec<_>>()
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
                    .a_append(INPUT_0, back_vector_0, AsyncBincodeCoder {})
                    .await
                    .unwrap();
                backed
                    .a_append(INPUT_1, back_vector_1, AsyncBincodeCoder {})
                    .await
                    .unwrap();
                let backed = Arc::new(backed);
                let collected = stream::iter(BackedArray::stream_send(&backed))
                    .map(|x| async { tokio::task::spawn(x).await.unwrap() })
                    .buffered(backed.len())
                    .try_collect::<Vec<_>>()
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
            .a_append(INPUT_0, back_vector_0, AsyncBincodeCoder {})
            .await
            .unwrap();
        backed
            .a_append_memory(INPUT_1, back_vector_1, AsyncBincodeCoder {})
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
        backed.a_get_multiple([0, 4]).collect::<Vec<_>>().await;
        assert_eq!(backed.loaded_len(), 6);
        backed.shrink_to_query(&[4]);
        assert_eq!(backed.loaded_len(), 3);
        backed.clear_memory();
        assert_eq!(backed.loaded_len(), 0);
    }
}
