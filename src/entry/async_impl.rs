/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use std::{
    fmt::Debug,
    marker::PhantomData,
    mem::transmute,
    ops::{Deref, DerefMut},
    pin::Pin,
    sync::Arc,
    task::{ready, Context, Poll},
};

use either::Either;
use futures::Future;
use pin_project::pin_project;
use serde::{Deserialize, Serialize};

use crate::utils::{
    blocking::BlockingFn,
    sync::{AsyncOnceLock, TryInitFuture},
    Once,
};

use super::{
    disks::{AsyncReadDisk, AsyncWriteDisk, ReadDisk, WriteDisk},
    formats::{AsyncDecoder, AsyncEncoder, Decoder, Encoder},
    BackedEntry, BackedEntryInner, BackedEntryInnerTrait, BackedEntryMutHandle, BackedEntryRead,
    BackedEntryWrite, MutHandle,
};

/// Asynchronous implementation for [`BackedEntry`].
///
/// Only implemented over [`AsyncOnceLock`], so it can be used within
/// a multi-threaded runtime without blocking.
///
/// # Generics
///
/// * `T`: the type to store.
/// * `Disk`: an implementor of [`AsyncReadDisk`](`super::disks::AsyncReadDisk`) and/or [`AsyncWriteDisk`](`super::disks::AsyncWriteDisk`).
/// * `Coder`: an implementor of [`AsyncEncoder`](`super::formats::AsyncEncoder`) and/or [`AsyncDecoder`](`super::formats::AsyncDecoder`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BackedEntryAsync<T, Disk, Coder>(BackedEntryInner<AsyncOnceLock<T>, Disk, Coder>);

// Copy over all the shared impls
impl<T, Disk, Coder> BackedEntry for BackedEntryAsync<T, Disk, Coder> {
    type OnceWrapper = AsyncOnceLock<T>;
    type Disk = Disk;
    type Coder = Coder;
    fn get_storage(&self) -> (&Self::Disk, &Self::Coder) {
        self.0.get_storage()
    }
    fn get_storage_mut(&mut self) -> (&mut Self::Disk, &mut Self::Coder) {
        self.0.get_storage_mut()
    }
    fn is_loaded(&self) -> bool {
        self.0.is_loaded()
    }
    fn unload(&mut self) {
        self.0.unload()
    }
    fn take(self) -> Option<<Self::OnceWrapper as Once>::Inner> {
        self.0.take()
    }
    fn new(disk: Self::Disk, coder: Self::Coder) -> Self
    where
        Self: Sized,
    {
        Self(BackedEntry::new(disk, coder))
    }
}

#[pin_project(project = BackedLoadStatePin)]
enum BackedLoadState<'a, Disk: AsyncReadDisk + 'a, Coder: AsyncDecoder<Disk::ReadDisk> + 'a> {
    Disk(#[pin] Disk::ReadFut),
    Decode(#[pin] Coder::DecodeFut<'a>),
}

impl<
        'a,
        Disk: AsyncReadDisk<ReadFut: Debug>,
        Coder: AsyncDecoder<Disk::ReadDisk, DecodeFut<'a>: Debug>,
    > Debug for BackedLoadState<'a, Disk, Coder>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disk(fut) => f.debug_tuple("BackedLoadState::Disk").field(fut).finish(),
            Self::Decode(fut) => f.debug_tuple("BackedLoadState::Decode").field(fut).finish(),
        }
    }
}

#[pin_project]
pub struct BackedLoadFut<'a, T, Disk: AsyncReadDisk, Coder: AsyncDecoder<Disk::ReadDisk>> {
    backed_entry: &'a BackedEntryInner<AsyncOnceLock<T>, Disk, Coder>,
    #[pin]
    state: BackedLoadState<'a, Disk, Coder>,
}

impl<
        'a,
        T: Debug,
        Disk: AsyncReadDisk<ReadFut: Debug> + Debug,
        Coder: AsyncDecoder<Disk::ReadDisk, DecodeFut<'a>: Debug> + Debug,
    > Debug for BackedLoadFut<'a, T, Disk, Coder>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackedLoadFut")
            .field("backed_entry", &self.backed_entry)
            .field("state", &self.state)
            .finish()
    }
}

impl<'a, T, Disk, Coder> Future for BackedLoadFut<'a, T, Disk, Coder>
where
    Disk: AsyncReadDisk,
    Coder: AsyncDecoder<Disk::ReadDisk, T = T>,
{
    type Output = Result<T, Coder::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.as_mut().project();

        loop {
            match this.state.as_mut().project() {
                BackedLoadStatePin::Disk(read_fut) => {
                    let res = ready!(read_fut.poll(cx));
                    match res {
                        Ok(read_disk) => {
                            this.state.set(BackedLoadState::Decode(
                                this.backed_entry.coder.decode(read_disk),
                            ));
                        }
                        Err(e) => {
                            return Poll::Ready(Err(e.into()));
                        }
                    }
                }
                BackedLoadStatePin::Decode(decode_fut) => {
                    return decode_fut.poll(cx);
                }
            }
        }
    }
}

impl<'a, T, Disk: AsyncReadDisk, Coder: AsyncDecoder<Disk::ReadDisk>>
    BackedLoadFut<'a, T, Disk, Coder>
{
    pub fn new(backed_entry: &'a BackedEntryInner<AsyncOnceLock<T>, Disk, Coder>) -> Self {
        let state = BackedLoadState::Disk(backed_entry.disk.async_read_disk());
        Self {
            backed_entry,
            state,
        }
    }
}

/// Future that gets an already initialized value or loads it.
type BackedTryInit<'a, T, Err, Disk, Coder> =
    TryInitFuture<'a, T, Err, BackedLoadFut<'a, T, Disk, Coder>>;

impl<'b, T, Disk, Coder> BackedEntryRead for BackedEntryAsync<T, Disk, Coder>
where
    T: for<'de> Deserialize<'de> + 'b,
    Disk: AsyncReadDisk,
    Coder: AsyncDecoder<Disk::ReadDisk, T = T>,
{
    type LoadResult<'a> = BackedTryInit<'a, T, Coder::Error, Disk, Coder> where Self: 'a;
    fn load(&self) -> Self::LoadResult<'_> {
        self.0.value.get_or_try_init(BackedLoadFut::new(&self.0))
    }
}

#[derive(Debug)]
#[pin_project]
struct BackedUpdateDisk<'a, F, T> {
    #[pin]
    pub write_fut: F,
    pub value: &'a T,
}

#[pin_project(project = BackedUpdateStatePin)]
enum BackedUpdateState<
    'a,
    T,
    Disk: AsyncWriteDisk<WriteDisk: 'a>,
    Coder: AsyncEncoder<Disk::WriteDisk> + 'a,
> {
    Disk(#[pin] BackedUpdateDisk<'a, Disk::WriteFut, T>),
    Encode(#[pin] Coder::EncodeFut<'a>),
    /// When the value to update with is None
    Uninit,
}

impl<
        'a,
        T: Debug,
        Disk: AsyncWriteDisk<WriteFut: Debug>,
        Coder: AsyncEncoder<Disk::WriteDisk, EncodeFut<'a>: Debug> + 'a,
    > Debug for BackedUpdateState<'a, T, Disk, Coder>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disk(disk) => f
                .debug_tuple("BackedUpdateState::Disk")
                .field(disk)
                .finish(),
            Self::Encode(fut) => f
                .debug_tuple("BackedUpdateState::Encode")
                .field(fut)
                .finish(),
            Self::Uninit => f.debug_tuple("BackedUpdateState::Uninit").finish(),
        }
    }
}

#[pin_project]
pub struct BackedUpdateFut<'a, T, Disk: AsyncWriteDisk, Coder: AsyncEncoder<Disk::WriteDisk>> {
    coder: &'a mut Coder,
    #[pin]
    state: BackedUpdateState<'a, T, Disk, Coder>,
}

impl<'a, T, Disk, Coder> Debug for BackedUpdateFut<'a, T, Disk, Coder>
where
    Disk: AsyncWriteDisk,
    Coder: AsyncEncoder<Disk::WriteDisk> + Debug,
    BackedUpdateState<'a, T, Disk, Coder>: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackedUpdateFut")
            .field("coder", self.coder)
            .field("state", &self.state)
            .finish()
    }
}

impl<'a, T, Disk, Coder> Future for BackedUpdateFut<'a, T, Disk, Coder>
where
    Disk: AsyncWriteDisk,
    Coder: AsyncEncoder<Disk::WriteDisk, T = T>,
{
    type Output = Result<(), Coder::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.as_mut().project();

        loop {
            match this.state.as_mut().project() {
                BackedUpdateStatePin::Disk(disk) => {
                    let disk = disk.project();
                    let res = ready!(disk.write_fut.poll(cx));
                    match res {
                        Ok(write_disk) => {
                            // The reference is valid for the lifetime of this struct,
                            // holds exclusive access, and it is only used mutably here.
                            // This extends the lifetime to reflect the pointer's lifetime,
                            // not this poll invocation.
                            let mut coder =
                                unsafe { transmute::<&mut Coder, &'a mut Coder>(this.coder) };

                            let new_state =
                                BackedUpdateState::Encode(coder.encode(disk.value, write_disk));
                            this.state.set(new_state);
                        }
                        Err(e) => {
                            return Poll::Ready(Err(e.into()));
                        }
                    }
                }
                BackedUpdateStatePin::Encode(encode_fut) => {
                    return encode_fut.poll(cx);
                }
                BackedUpdateStatePin::Uninit => {
                    return Poll::Ready(Ok(()));
                }
            }
        }
    }
}

impl<'a, T, Disk: AsyncWriteDisk, Coder: AsyncEncoder<Disk::WriteDisk>>
    BackedUpdateFut<'a, T, Disk, Coder>
{
    pub fn update(backed_entry: &'a mut BackedEntryInner<AsyncOnceLock<T>, Disk, Coder>) -> Self {
        let state = if let Some(value) = backed_entry.value.get() {
            BackedUpdateState::Disk(BackedUpdateDisk {
                write_fut: backed_entry.disk.async_write_disk(),
                value,
            })
        } else {
            BackedUpdateState::Uninit
        };

        Self {
            coder: &mut backed_entry.coder,
            state,
        }
    }

    pub fn write(
        backed_entry: &'a mut BackedEntryInner<AsyncOnceLock<T>, Disk, Coder>,
        new_value: T,
    ) -> Self {
        // Drop previous value and write in new.
        // value.set() only works when uninitialized.
        backed_entry.value = AsyncOnceLock::new();
        let _ = backed_entry.value.set(new_value);

        Self {
            coder: &mut backed_entry.coder,
            state:
            BackedUpdateState::Disk(BackedUpdateDisk {write_fut: backed_entry.disk.async_write_disk(), value: backed_entry.value.get().unwrap_or_else(|| panic!("This was previously initialized with exclusive access, it must be initialized!"))})
        }
    }

    pub fn write_unload(
        backed_entry: &'a mut BackedEntryInner<AsyncOnceLock<T>, Disk, Coder>,
        value: &'a T,
    ) -> Self {
        backed_entry.unload();

        Self {
            coder: &mut backed_entry.coder,
            state: BackedUpdateState::Disk(BackedUpdateDisk {
                write_fut: backed_entry.disk.async_write_disk(),
                value,
            }),
        }
    }

    pub fn noop(backed_entry: &'a mut BackedEntryInner<AsyncOnceLock<T>, Disk, Coder>) -> Self {
        Self {
            coder: &mut backed_entry.coder,
            state: BackedUpdateState::Uninit,
        }
    }
}

impl<'a, T, Disk, Coder> BackedEntryWrite<'a> for BackedEntryAsync<T, Disk, Coder>
where
    T: Serialize + 'a,
    Disk: AsyncWriteDisk<WriteDisk: 'a>,
    Coder: AsyncEncoder<Disk::WriteDisk, T = T> + 'a,
{
    type UpdateResult = BackedUpdateFut<'a, T, Disk, Coder>;
    fn update(&'a mut self) -> Self::UpdateResult {
        BackedUpdateFut::update(&mut self.0)
    }

    fn write<U>(&'a mut self, new_value: U) -> Self::UpdateResult
    where
        U: Into<<Self::OnceWrapper as Once>::Inner>,
    {
        BackedUpdateFut::write(&mut self.0, new_value.into())
    }

    fn write_ref(
        &'a mut self,
        new_value: &'a <Self::OnceWrapper as Once>::Inner,
    ) -> Self::UpdateResult {
        BackedUpdateFut::write_unload(&mut self.0, new_value)
    }
}

/// [`MutHandle`] for [`BackedEntryAsync`].
pub struct BackedEntryMutAsync<'a, T, Disk, Coder> {
    entry: &'a mut BackedEntryAsync<T, Disk, Coder>,
    modified: bool,
}

impl<T, Disk, Coder> Deref for BackedEntryMutAsync<'_, T, Disk, Coder> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.entry.0.value.get().unwrap()
    }
}

impl<T, Disk, Coder> DerefMut for BackedEntryMutAsync<'_, T, Disk, Coder> {
    /// [`DerefMut::deref_mut`] that sets a modified flag.
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.modified = true;
        self.entry.0.value.get_mut().unwrap()
    }
}

#[derive(Debug)]
#[pin_project(project = ProjOptionPin)]
enum ProjOption<T> {
    Some(#[pin] T),
    None,
}

#[pin_project]
pub struct BackedFlushFut<'a, T, Disk: AsyncWriteDisk, Coder: AsyncEncoder<Disk::WriteDisk>> {
    modified: &'a mut bool,
    #[pin]
    update_fut: ProjOption<BackedUpdateFut<'a, T, Disk, Coder>>,
}

impl<'a, T, Disk, Coder> Debug for BackedFlushFut<'a, T, Disk, Coder>
where
    Disk: AsyncWriteDisk,
    Coder: AsyncEncoder<Disk::WriteDisk>,
    BackedUpdateFut<'a, T, Disk, Coder>: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackedFlushFut")
            .field("modified", self.modified)
            .field("update_fut", &self.update_fut)
            .finish()
    }
}

impl<T, Disk, Coder> Future for BackedFlushFut<'_, T, Disk, Coder>
where
    Disk: AsyncWriteDisk,
    Coder: AsyncEncoder<Disk::WriteDisk, T = T>,
{
    type Output = Result<(), Coder::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.as_mut().project();

        loop {
            if let ProjOptionPin::Some(fut) = this.update_fut.as_mut().project() {
                if let Err(e) = ready!(fut.poll(cx)) {
                    return Poll::Ready(Err(e));
                } else {
                    this.update_fut.set(ProjOption::None);
                }
            } else {
                **this.modified = false;
                return Poll::Ready(Ok(()));
            }
        }
    }
}

impl<'a, T, Disk, Coder> BackedFlushFut<'a, T, Disk, Coder>
where
    Disk: AsyncWriteDisk,
    Coder: AsyncEncoder<Disk::WriteDisk, T = T>,
{
    pub fn new(
        backing: &'a mut BackedEntryInner<AsyncOnceLock<T>, Disk, Coder>,
        modified: &'a mut bool,
    ) -> Self {
        let update_fut = if *modified {
            ProjOption::Some(BackedUpdateFut::update(backing))
        } else {
            ProjOption::None
        };

        Self {
            update_fut,
            modified,
        }
    }
}

impl<T, Disk, Coder> MutHandle<T> for BackedEntryMutAsync<'_, T, Disk, Coder>
where
    T: Serialize,
    Disk: AsyncWriteDisk,
    Coder: AsyncEncoder<Disk::WriteDisk, T = T>,
{
    fn is_modified(&self) -> bool {
        self.modified
    }

    type FlushResult<'a> = BackedFlushFut<'a, T, Disk, Coder>
        where
            Self: 'a;
    fn flush(&mut self) -> Self::FlushResult<'_> {
        BackedFlushFut::new(&mut self.entry.0, &mut self.modified)
    }
}

impl<T, Disk, Coder> Drop for BackedEntryMutAsync<'_, T, Disk, Coder> {
    /// [`Drop::drop`] that panics if modified is set.
    ///
    /// Flushing is an async action, and cannot be run via sync drop.
    fn drop(&mut self) {
        if self.modified {
            panic!("BackedEntryMutAsync dropped while modified");
        }
    }
}

#[pin_project]
pub struct GenMutHandle<'a, T, Disk: AsyncReadDisk, Coder: AsyncDecoder<Disk::ReadDisk, T = T>> {
    backed: *mut BackedEntryAsync<T, Disk, Coder>,
    #[pin]
    init: BackedTryInit<'a, T, Coder::Error, Disk, Coder>,
    _phantom: PhantomData<&'a ()>,
}

impl<'a, T, Disk, Coder> Debug for GenMutHandle<'a, T, Disk, Coder>
where
    Disk: AsyncReadDisk,
    Coder: AsyncDecoder<Disk::ReadDisk, T = T>,
    BackedEntryAsync<T, Disk, Coder>: Debug,
    BackedTryInit<'a, T, Coder::Error, Disk, Coder>: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GenMutHandle")
            .field("backed", unsafe { &*self.backed })
            .field("init", &self.init)
            .finish()
    }
}

impl<'a, T, Disk, Coder> Future for GenMutHandle<'a, T, Disk, Coder>
where
    Disk: AsyncReadDisk,
    Coder: AsyncDecoder<Disk::ReadDisk, T = T>,
{
    type Output = Result<
        BackedEntryMutAsync<'a, T, Disk, Coder>,
        <Coder as AsyncDecoder<Disk::ReadDisk>>::Error,
    >;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.as_mut().project();
        match ready!(this.init.poll(cx)) {
            Ok(_) => {
                // The pointer is valid for the lifetime of this struct
                // (and its calling method), holds exclusive access, and
                // is only modified by the prior init future.
                let entry = unsafe { &mut **this.backed };

                Poll::Ready(Ok(BackedEntryMutAsync {
                    entry,
                    modified: false,
                }))
            }
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}

impl<'a, T, Disk, Coder> GenMutHandle<'a, T, Disk, Coder>
where
    T: for<'de> Deserialize<'de>,
    Disk: AsyncReadDisk,
    Coder: AsyncDecoder<Disk::ReadDisk, T = T>,
{
    pub fn new(backed: &'a mut BackedEntryAsync<T, Disk, Coder>) -> Self {
        // The eventual result type needs `&mut backed`, but we need to load in
        // a value first. The pointer is used to achieve the temporary aliasing.
        let backed: *mut _ = backed;
        let init = unsafe { &*backed }.load();

        GenMutHandle {
            backed,
            init,
            _phantom: PhantomData,
        }
    }
}

impl<'b, T, Disk, Coder> BackedEntryMutHandle<'b> for BackedEntryAsync<T, Disk, Coder>
where
    T: Serialize + for<'de> Deserialize<'de> + 'b,
    Disk: AsyncWriteDisk<WriteDisk: 'b> + AsyncReadDisk,
    Coder: AsyncEncoder<Disk::WriteDisk, T = T> + AsyncDecoder<Disk::ReadDisk, T = T> + 'b,
{
    type MutHandleResult<'a> = GenMutHandle<'a, T, Disk, Coder>
        where
            Self: 'a;
    fn mut_handle(&mut self) -> Self::MutHandleResult<'_> {
        GenMutHandle::new(self)
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, io::Cursor, sync::Arc, thread::scope};

    use crate::{
        entry::{BackedEntryArr, BackedEntryArrLock, BackedEntryCell},
        test_utils::cursor_vec,
        test_utils::CursorVec,
    };

    #[cfg(feature = "async_bincode")]
    use crate::entry::formats::AsyncBincodeCoder;

    use super::*;

    #[cfg(feature = "async_bincode")]
    #[tokio::test]
    async fn mutate() {
        use std::pin::pin;

        const FIB: &[u8] = &[0, 1, 1, 5, 7];
        cursor_vec!(back_vec, backing_store);

        // Intentional unsafe access to later peek underlying storage
        let mut backed_entry = unsafe {
            BackedEntryAsync::<Box<[_]>, _, _>::new(
                &mut *back_vec.get(),
                AsyncBincodeCoder::default(),
            )
        };
        pin!(backed_entry.write_ref(&FIB.into())).await.unwrap();

        assert_eq!(backed_entry.load().await.unwrap().as_ref(), FIB);

        #[cfg(not(miri))]
        assert_eq!(&backing_store[backing_store.len() - FIB.len()..], FIB);

        let mut handle = backed_entry.mut_handle().await.unwrap();
        handle[0] = 20;
        handle[2] = 30;

        #[cfg(not(miri))]
        assert_eq!(backing_store[backing_store.len() - FIB.len()], FIB[0]);

        assert_eq!(handle[0], 20);
        assert_eq!(handle[2], 30);

        handle.flush().await.unwrap();

        #[cfg(not(miri))]
        {
            assert_eq!(backing_store[backing_store.len() - FIB.len()], 20);
            assert_eq!(backing_store[backing_store.len() - FIB.len() + 2], 30);
            assert_eq!(backing_store[backing_store.len() - FIB.len() + 1], FIB[1]);
        }

        drop(handle);
        assert_eq!(
            backed_entry.load().await.unwrap().as_ref(),
            [20, 1, 30, 5, 7]
        );
    }

    #[cfg(feature = "async_bincode")]
    #[tokio::test]
    async fn mutate_option() {
        let mut input: HashMap<String, u128> = HashMap::new();
        input.insert("THIS IS A STRING".to_string(), 55);
        input.insert("THIS IS ALSO A STRING".to_string(), 23413);

        let mut binding = Cursor::new(Vec::with_capacity(10));
        let mut back_vec = CursorVec {
            inner: (&mut binding).into(),
        };

        // Intentional unsafe access to later peek underlying storage
        let mut backed_entry = BackedEntryAsync::new(&mut back_vec, AsyncBincodeCoder::default());
        backed_entry.write_ref(&input.clone()).await.unwrap();

        assert_eq!(&input, backed_entry.load().await.unwrap());
        let mut handle = backed_entry.mut_handle().await.unwrap();
        handle
            .deref_mut()
            .insert("EXTRA STRING".to_string(), 234137);
        handle.flush().await.unwrap();

        drop(handle);
        assert_eq!(
            backed_entry
                .load()
                .await
                .unwrap()
                .get("EXTRA STRING")
                .unwrap(),
            &234137
        );
    }

    #[cfg(feature = "async_bincode")]
    #[tokio::test]
    async fn write() {
        const VALUE: &[u8] = &[5];
        const NEW_VALUE: &[u8] = &[7];

        cursor_vec!(back_vec, back_vec_inner);

        let mut backed_entry = BackedEntryAsync::<Box<[_]>, _, _>::new(
            unsafe { &mut *back_vec.get() },
            AsyncBincodeCoder::default(),
        );

        backed_entry.write_ref(&VALUE.into()).await.unwrap();
        assert!(!backed_entry.is_loaded());

        #[cfg(not(miri))]
        assert_eq!(&back_vec_inner[back_vec_inner.len() - VALUE.len()..], VALUE);

        assert_eq!(backed_entry.load().await.unwrap().as_ref(), VALUE);

        backed_entry.write(NEW_VALUE).await.unwrap();
        assert!(backed_entry.is_loaded());

        #[cfg(all(test, not(miri)))]
        assert_eq!(
            &back_vec_inner[back_vec_inner.len() - NEW_VALUE.len()..],
            NEW_VALUE
        );

        assert_eq!(backed_entry.load().await.unwrap().as_ref(), NEW_VALUE);
    }

    #[cfg(all(feature = "async_bincode", feature = "tokio"))]
    #[tokio::test]
    async fn read_threaded() {
        use futures::StreamExt;

        use crate::test_utils::StaticCursorVec;

        const VALUES: &[u8] = &[0, 1, 3, 5, 7];
        const NEW_VALUES: &[u8] = &[17, 15, 13, 11, 10];

        let binding = StaticCursorVec::new(Cursor::new(Vec::with_capacity(10)));

        let mut backed_entry =
            BackedEntryAsync::<Box<[_]>, _, _>::new(binding, AsyncBincodeCoder::default());

        backed_entry.write_ref(&VALUES.into()).await.unwrap();
        assert!(!backed_entry.is_loaded());

        {
            let (mut scope, _) = unsafe {
                async_scoped::TokioScope::scope(|s| {
                    let backed_share = Arc::new(&backed_entry);
                    (0..VALUES.len()).for_each(|idx| {
                        let backed_share = backed_share.clone();
                        s.spawn(async move {
                            assert_eq!(backed_share.load().await.unwrap()[idx], VALUES[idx])
                        })
                    });
                })
            };
            while scope.next().await.is_some() {}
        }

        /*
            backed_entry.write(NEW_VALUES).await.unwrap();
            assert!(backed_entry.is_loaded());

            {
                let (mut scope, _) = unsafe {
                    async_scoped::TokioScope::scope(|s| {
                        let backed_share = Arc::new(&backed_entry);
                        (0..VALUES.len()).for_each(|idx| {
                            let backed_share = backed_share.clone();
                            s.spawn(async move {
                                assert_eq!(backed_share.load().await.unwrap()[idx], VALUES[idx])
                            })
                        });
                    })
                };
                while scope.next().await.is_some() {}
            }
        */
    }
}
