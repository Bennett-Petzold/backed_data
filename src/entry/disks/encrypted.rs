/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/*!
Defines memory protected storage that encrypts to disk.
*/

use std::{
    cell::UnsafeCell,
    io::{Cursor, Read, Write},
    marker::PhantomData,
    mem::transmute,
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    sync::OnceLock,
};

use aes_gcm::{
    aead::{generic_array::GenericArray, AeadMutInPlace, Buffer, OsRng},
    AeadCore, Aes256Gcm, KeyInit,
};
use num_traits::CheckedAdd;
use secrets::{traits::Bytes, SecretBox, SecretVec};
use serde::{de::Visitor, Deserialize, Serialize};
use stable_deref_trait::StableDeref;

use crate::utils::{AsyncCompatCursor, BorrowExtender};

#[derive(Debug, Clone, Copy)]
struct KeyNonce {
    key: [u8; 32],
    nonce: [u8; 12],
}

unsafe impl Bytes for KeyNonce {}

/// Wraps [`secrets::SecretVec`].
///
/// Implements Serialize and Deserialize, but
/// serialization and deserialization leak information, so be careful. Using
/// [`Encrypted`] as the backing store can make sure protection is kept during
/// serialization, and the disk only sees encrypted data.
#[derive(Debug)]
pub struct SecretVecWrapper<T: Bytes + ?Sized>(pub secrets::SecretVec<T>);

#[derive(Debug)]
struct SecretVecVisitor<T: Bytes> {
    _phantom: PhantomData<T>,
}

impl<T: Bytes> Default for SecretVecVisitor<T> {
    fn default() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<'de, T: Bytes + Deserialize<'de>> Visitor<'de> for SecretVecVisitor<T> {
    type Value = SecretVecWrapper<T>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            formatter,
            "A sequence of T, representing the [T] wrapped by this type."
        )
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        // Arbitrary default size to buffer with
        const SIZE_GUESS: usize = 16;

        let size_hint = seq.size_hint().unwrap_or(SIZE_GUESS);
        let mut secret_vec = SecretVec::random(size_hint);

        let mut filled_len = 0;
        while let Some(next) = seq.next_element()? {
            // Double capacity if it's been filled. Arbitrary growth pattern.
            if filled_len >= secret_vec.len() {
                if secret_vec.len() == usize::MAX {
                    return Err(serde::de::Error::custom(
                        "sequence length is above usize limit",
                    ));
                }
                secret_vec = SecretVec::new(secret_vec.len().saturating_mul(2), |s| {
                    s[..secret_vec.len()].copy_from_slice(&secret_vec.borrow())
                });
            }

            SecretVec::borrow_mut(&mut secret_vec)[filled_len] = next;
            filled_len += 1;
        }

        // Move/shrink to an exact capacity allocation.
        if secret_vec.len() > filled_len {
            secret_vec = SecretVec::new(filled_len, |s| {
                s.copy_from_slice(&secret_vec.borrow()[..filled_len])
            });
        }

        Ok(SecretVecWrapper(secret_vec))
    }
}

impl<'de, T: Bytes + Deserialize<'de>> Deserialize<'de> for SecretVecWrapper<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(SecretVecVisitor::default())
    }
}

impl<T: Bytes + Serialize> Serialize for SecretVecWrapper<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Serializes [T].
        self.0.borrow().serialize(serializer)
    }
}

/// [`secrets::SecretVec::borrow`] is backed by a Box pointer, and meets the
/// [`StableDeref`] API requirements (all methods rely on this box pointer, or
/// non-addressed memory).
unsafe impl<T: Bytes> StableDeref for SecretVecWrapper<T> {}

impl<T: Bytes> Deref for SecretVecWrapper<T> {
    type Target = secrets::SecretVec<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Bytes> DerefMut for SecretVecWrapper<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T: Bytes, U: AsRef<[T]>> From<U> for SecretVecWrapper<T> {
    fn from(value: U) -> Self {
        let value = value.as_ref();
        Self(SecretVec::new(value.len(), |s| s.copy_from_slice(value)))
    }
}

#[derive(Debug)]
struct SecretBoxRef<'a, T: Bytes>(secrets::secret_box::Ref<'a, T>);

/// [`secrets::secret_box::Ref`] is backed by a Box pointer, and meets the
/// [`StableDeref`] API requirements.
unsafe impl<T: Bytes> StableDeref for SecretBoxRef<'_, T> {}

impl<T: Bytes> Deref for SecretBoxRef<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

fn random_key_nonce() -> SecretBox<KeyNonce> {
    SecretBox::random()
}

/// A resource encrypted with Aes256Gcm on some underlying [`disk`][`super`].
///
/// Secrets and intermediate processing are stored in [`SecretBox`], which minimizes
/// the risk of memory being snooped (see [`secrets`] for details). This does not
/// protect the code read from this disk, so read into [`SecretVecWrapper`] to keep
/// memory guards in place beyond encryption and decryption.
///
/// The secrets are excluded from serialization and deserialization, being
/// set with garbage data on deserialization. Use [`Self::set_key`] and
/// [`Self::set_nonce`] to initialize secrets after deserialization.
///
/// Since encryption uses an internal buffer for reading/writing,
/// [`Unbuffered`][`super::Unbuffered`] is more efficient than
/// [`Plainfile`][`super::Plainfile`].
#[derive(Serialize, Deserialize)]
pub struct Encrypted<B> {
    inner: B,
    #[serde(skip)]
    #[serde(default = "random_key_nonce")]
    secrets: SecretBox<KeyNonce>,
}

impl<B> From<Encrypted<B>> for PathBuf
where
    PathBuf: From<B>,
{
    fn from(val: Encrypted<B>) -> Self {
        Self::from(val.inner)
    }
}

impl<B: AsRef<Path>> AsRef<Path> for Encrypted<B> {
    fn as_ref(&self) -> &Path {
        self.inner.as_ref()
    }
}

unsafe impl<B: Send> Send for Encrypted<B> {}
unsafe impl<B: Sync> Sync for Encrypted<B> {}

impl<B> Encrypted<B> {
    /// Create a new [`Encrypted`].
    ///
    /// # Parameters
    /// * `inner`: Underlying disk written to / read from
    /// * `key`: Aes256Gcm key. Generated with [`OsRng`] if not provided.
    /// * `nonce`: Aes256Gcm nonce. Generated with [`OsRng`] if not provided.
    pub fn new<K, N>(inner: B, key: Option<K>, nonce: Option<N>) -> Self
    where
        K: AsRef<[u8; 32]>,
        N: AsRef<[u8; 12]>,
    {
        Self {
            inner,
            secrets: SecretBox::new(|s| {
                *s = KeyNonce {
                    key: key
                        .map(|k| *k.as_ref())
                        .unwrap_or(Aes256Gcm::generate_key(OsRng).into()),
                    nonce: nonce.map(|n| *n.as_ref()).unwrap_or(
                        Aes256Gcm::generate_nonce(OsRng)
                            .as_slice()
                            .try_into()
                            .unwrap(),
                    ),
                };
            }),
        }
    }

    /// To avoid explicit typing every time with [`Self::new`] and `None`s.
    pub fn new_random(inner: B) -> Self {
        Self::new::<Box<[u8; 32]>, Box<[u8; 12]>>(inner, None, None)
    }

    pub fn set_key(&mut self, key: &SecretBox<[u8; 32]>) -> &mut Self {
        self.secrets
            .borrow_mut()
            .key
            .copy_from_slice(key.borrow().as_slice());
        self
    }

    pub fn set_nonce(&mut self, nonce: &SecretBox<[u8; 12]>) -> &mut Self {
        self.secrets
            .borrow_mut()
            .nonce
            .copy_from_slice(nonce.borrow().as_slice());
        self
    }

    pub fn get_key(&self) -> impl Deref<Target = [u8; 32]> + '_ {
        BorrowExtender::new(SecretBoxRef(self.secrets.borrow()), |secrets| secrets.key)
    }

    pub fn get_nonce(&self) -> impl Deref<Target = [u8; 12]> + '_ {
        BorrowExtender::new(SecretBoxRef(self.secrets.borrow()), |secrets| secrets.nonce)
    }
}

/// Beware, this generates a random key and nonce.
impl From<Plainfile> for Encrypted<Plainfile> {
    fn from(value: Plainfile) -> Self {
        Self::new::<Box<[u8; 32]>, Box<[u8; 12]>>(value, None, None)
    }
}

/// Beware, this generates a random key and nonce.
impl From<PathBuf> for Encrypted<Plainfile> {
    fn from(value: PathBuf) -> Self {
        let value: Plainfile = value.into();
        Self::from(value)
    }
}

#[derive(Debug)]
struct SecretVecU8(BorrowExtender<SecretVecWrapper<u8>, secrets::secret_vec::Ref<'static, u8>>);

impl From<SecretVec<u8>> for SecretVecU8 {
    fn from(value: SecretVec<u8>) -> Self {
        // Lifetime shenanigans, this is 'static because it owns the underlying
        // data for its own lifetime
        Self(BorrowExtender::new(SecretVecWrapper(value), |value| {
            let value =
                unsafe { transmute::<&SecretVecWrapper<u8>, &'static SecretVecWrapper<u8>>(value) };
            value.borrow()
        }))
    }
}

impl From<SecretVecBuffer> for SecretVecU8 {
    fn from(value: SecretVecBuffer) -> Self {
        let value: SecretVec<u8> = value.into();
        Self::from(value)
    }
}

impl AsRef<[u8]> for SecretVecU8 {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

/// Secret borrow handle, with three possible states.
#[derive(Debug)]
enum SecretVecBufferHandle<'a> {
    Ref(secrets::secret_vec::Ref<'a, u8>),
    Mut(secrets::secret_vec::RefMut<'a, u8>),
    None,
}

/// A SecretVec implementing [`Buffer`].
#[derive(Debug)]
pub struct SecretVecBuffer {
    inner: SecretVec<u8>,
    /// Tracks `inner` length.
    len: usize,
    /// Reference to the owned `inner` value.
    ///
    /// The usage of UnsafeCell is entirely constrained by Rust's borrowing
    /// rules at function calls. The 'static lifetime is valid because
    /// SecretVec's borrows point to heap allocated data, which do not change
    /// address when SecretVec is moved. Then we know the borrows are valid
    /// while the SecretVec is valid, which is for this struct lifetime.
    handle: UnsafeCell<SecretVecBufferHandle<'static>>,
}

impl From<SecretVec<u8>> for SecretVecBuffer {
    fn from(value: SecretVec<u8>) -> Self {
        let len = value.len();
        Self {
            inner: value,
            len,
            handle: SecretVecBufferHandle::None.into(),
        }
    }
}

impl Default for SecretVecBuffer {
    fn default() -> Self {
        SecretVec::<u8>::zero(0).into()
    }
}

impl Deref for SecretVecBuffer {
    type Target = SecretVec<u8>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl From<SecretVecBuffer> for SecretVec<u8> {
    fn from(val: SecretVecBuffer) -> Self {
        let mut inner = val.inner;

        // Shrink the SecretVec to initialized size
        if inner.len() != val.len {
            inner = SecretVec::new(val.len, |s| s.copy_from_slice(&inner.borrow()[..val.len]));
        }

        inner
    }
}

impl AsRef<[u8]> for SecretVecBuffer {
    fn as_ref(&self) -> &[u8] {
        // Taken mutable so we can initialize, if necessary.
        let handle = unsafe { &mut *self.handle.get() };

        // Intialize ref, if that isn't the current enum state
        if matches!(handle, SecretVecBufferHandle::Ref(_)) {
            let inner: *const _ = &self.inner;
            *handle = SecretVecBufferHandle::Ref(unsafe { &*inner }.borrow());
        }

        if let SecretVecBufferHandle::Ref(x) = handle {
            // Only give a slice to the initialized part
            &(&*x)[..self.len]
        } else {
            panic!("This was previously initialized as Ref!")
        }
    }
}

impl AsMut<[u8]> for SecretVecBuffer {
    fn as_mut(&mut self) -> &mut [u8] {
        let handle = self.handle.get_mut();

        // Intialize mut, if that isn't the current enum state
        if matches!(handle, SecretVecBufferHandle::Mut(_)) {
            let inner: *mut _ = &mut self.inner;
            *handle = SecretVecBufferHandle::Mut(unsafe { &mut *inner }.borrow_mut());
        }

        if let SecretVecBufferHandle::Mut(x) = handle {
            // Only give a slice to the initialized part
            &mut (&mut *x)[..self.len]
        } else {
            panic!("This was previously initialized as Mut!")
        }
    }
}

impl Buffer for SecretVecBuffer {
    fn len(&self) -> usize {
        self.len
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn extend_from_slice(&mut self, other: &[u8]) -> aes_gcm::aead::Result<()> {
        // Clear handle, since we're updating underlying state
        *self.handle.get_mut() = SecretVecBufferHandle::None;

        // Multiply size if other is beyond capacity
        let mut capacity = self.inner.len();
        if (self.len + other.len()) > capacity {
            // Need special handling for zero case, multiplies by zero get us
            // nowhere.
            if capacity == 0 {
                capacity = 1;
            }

            // factor = [(len + other - 1) / capacity] + 1;
            // Since len + other is (almost) always > capacity, this is >= 2.
            // If other's length is 1 and capacity was zero, this will be
            //     [(0 + 1 - 1) / 1] + 1 = 1, an exact allocation.
            let mult_factor = (self
                .len
                .checked_add(other.len())
                .ok_or(aes_gcm::aead::Error)?
                - 1)
                / capacity;
            let mult_factor = mult_factor.checked_add(1).ok_or(aes_gcm::aead::Error)?;

            let new_capacity = capacity
                .checked_mul(mult_factor)
                .ok_or(aes_gcm::aead::Error)?;

            self.inner = SecretVec::new(new_capacity, |s| {
                s[..self.len].copy_from_slice(&self.inner.borrow()[..self.len])
            });
        }

        // Copy over data and update length.
        let new_len = self
            .len
            .checked_add(other.len())
            .ok_or(aes_gcm::aead::Error)?;
        self.inner.borrow_mut()[self.len..new_len].copy_from_slice(other);
        self.len = new_len;

        Ok(())
    }

    fn truncate(&mut self, len: usize) {
        // Take the chance to clear the handle, for minimum exposure
        *self.handle.get_mut() = SecretVecBufferHandle::None;

        self.len = len
    }
}

// Does not alias internal data, so ignore inherited !Send + !Sync
unsafe impl Send for SecretVecBuffer {}
unsafe impl Sync for SecretVecBuffer {}

#[derive(Debug)]
pub struct SecretReadVec {
    inner: AsyncCompatCursor<SecretVecU8>,
}

impl From<SecretVec<u8>> for SecretReadVec {
    fn from(value: SecretVec<u8>) -> Self {
        Self {
            inner: Cursor::new(value.into()).into(),
        }
    }
}

impl From<SecretVecU8> for SecretReadVec {
    fn from(value: SecretVecU8) -> Self {
        Self {
            inner: Cursor::new(value).into(),
        }
    }
}

impl From<SecretVecBuffer> for SecretReadVec {
    fn from(value: SecretVecBuffer) -> Self {
        let value: SecretVecU8 = value.into();
        Self::from(value)
    }
}

impl Read for SecretReadVec {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buf)
    }
    fn read_vectored(&mut self, bufs: &mut [std::io::IoSliceMut<'_>]) -> std::io::Result<usize> {
        self.inner.read_vectored(bufs)
    }
}

// Does not alias internal data, so ignore inherited !Send + !Sync
unsafe impl Send for SecretReadVec {}
unsafe impl Sync for SecretReadVec {}

impl<B: ReadDisk> ReadDisk for Encrypted<B> {
    type ReadDisk = SecretReadVec;

    fn read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        let mut loaded = Vec::new();
        let mut read_disk = self.inner.read_disk()?;
        read_disk.read_to_end(&mut loaded)?;

        let mut decrypted: SecretVecBuffer =
            SecretVec::new(loaded.len(), |s| s.copy_from_slice(&loaded)).into();
        let mut aes = Aes256Gcm::new(&self.secrets.borrow().key.into());
        aes.decrypt_in_place(
            GenericArray::from_slice(&self.secrets.borrow().nonce),
            &[],
            &mut decrypted,
        )
        .map_err(|_| std::io::ErrorKind::Other)?;

        Ok(decrypted.into())
    }
}

#[derive(Debug)]
pub struct EncryptedWriter<B> {
    inner: B,
    secrets: SecretBox<KeyNonce>,
    buffer: SecretVecBuffer,
}

impl<B: Write> Write for EncryptedWriter<B> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buffer
            .extend_from_slice(buf)
            .map_err(|_| std::io::ErrorKind::OutOfMemory)?;
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let mut buf: SecretVecBuffer = self.buffer.clone().into();

        let mut aes = Aes256Gcm::new(&self.secrets.borrow().key.into());
        aes.encrypt_in_place(
            GenericArray::from_slice(&self.secrets.borrow().nonce),
            &[],
            &mut buf,
        )
        .map_err(|_| std::io::ErrorKind::Other)?;

        let buf: SecretVec<u8> = buf.into();
        let res = self.inner.write_all(&buf.borrow());
        res
    }
}

impl<B> EncryptedWriter<B> {
    fn new(inner: B, secrets: SecretBox<KeyNonce>) -> Self {
        Self {
            inner,
            secrets,
            buffer: SecretVecBuffer::default(),
        }
    }
}

impl<B: WriteDisk> WriteDisk for Encrypted<B> {
    type WriteDisk = EncryptedWriter<B::WriteDisk>;

    fn write_disk(&mut self) -> std::io::Result<Self::WriteDisk> {
        Ok(EncryptedWriter::new(
            self.inner.write_disk()?,
            self.secrets.clone(),
        ))
    }
}

#[cfg(feature = "async")]
pub use async_impl::*;

use super::{Plainfile, ReadDisk, WriteDisk};
#[cfg(feature = "async")]
mod async_impl {

    use std::io::SeekFrom;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};
    use std::task::{ready, Context, Poll};

    use crate::entry::disks::{AsyncReadDisk, AsyncWriteDisk};
    use crate::utils::blocking::BlockingFn;

    use super::*;

    use futures::io::{AsyncRead, AsyncWrite};
    use futures::io::{AsyncReadExt, AsyncSeek};
    use futures::Future;
    use pin_project::pin_project;

    /// Async wrapper of [`Encrypted`], which derefs to the sync variant.
    ///
    /// Adds handles for blocking operations, see [`crate::utils::blocking`]
    /// for convenience functions.
    pub struct AsyncEncrypted<B, FD, FE> {
        sync: Encrypted<B>,
        decrypt_handle: FD,
        encrypt_handle: FE,
    }

    impl<B: Bytes, FD, FE> AsyncEncrypted<B, FD, FE> {
        pub fn new<K, N>(
            inner: B,
            key: Option<K>,
            nonce: Option<N>,
            decrypt_handle: FD,
            encrypt_handle: FE,
        ) -> Self
        where
            K: AsRef<[u8; 32]>,
            N: AsRef<[u8; 12]>,
        {
            Self {
                sync: Encrypted::new(inner, key, nonce),
                decrypt_handle,
                encrypt_handle,
            }
        }
    }

    impl<B, FD, FE> Deref for AsyncEncrypted<B, FD, FE> {
        type Target = Encrypted<B>;
        fn deref(&self) -> &Self::Target {
            &self.sync
        }
    }

    impl<B, FD, FE> DerefMut for AsyncEncrypted<B, FD, FE> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.sync
        }
    }

    impl<B: ReadDisk, FE, FD> ReadDisk for AsyncEncrypted<B, FE, FD> {
        type ReadDisk = SecretReadVec;

        fn read_disk(&self) -> std::io::Result<Self::ReadDisk> {
            self.sync.read_disk()
        }
    }

    impl<B: WriteDisk, FE, FD> WriteDisk for AsyncEncrypted<B, FE, FD> {
        type WriteDisk = EncryptedWriter<B::WriteDisk>;

        fn write_disk(&mut self) -> std::io::Result<Self::WriteDisk> {
            self.sync.write_disk()
        }
    }

    impl AsyncRead for SecretReadVec {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<std::io::Result<usize>> {
            Pin::new(&mut self.get_mut().inner).poll_read(cx, buf)
        }
    }

    impl AsyncSeek for SecretReadVec {
        fn poll_seek(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            position: SeekFrom,
        ) -> Poll<std::io::Result<u64>> {
            Pin::new(&mut self.get_mut().inner).poll_seek(cx, position)
        }
    }

    /// [`BlockingFn`] that runs AES decryption.
    #[derive(Debug)]
    pub struct DecryptBlocking {
        loaded: Vec<u8>,
        secrets: SecretBox<KeyNonce>,
    }

    impl BlockingFn for DecryptBlocking {
        type Output = std::io::Result<SecretReadVec>;

        fn call(self) -> Self::Output {
            let mut decrypted: SecretVecBuffer =
                SecretVec::new(self.loaded.len(), |s| s.copy_from_slice(&self.loaded)).into();
            let mut aes = Aes256Gcm::new(&self.secrets.borrow().key.into());
            aes.decrypt_in_place(
                GenericArray::from_slice(&self.secrets.borrow().nonce),
                &[],
                &mut decrypted,
            )
            .map_err(|_| std::io::ErrorKind::Other)?;
            Ok(decrypted.into())
        }
    }

    #[derive(Debug)]
    #[pin_project]
    struct DecryptDiskRead<R> {
        #[pin]
        disk: R,
        storage: Vec<u8>,
    }

    #[derive(Debug)]
    #[pin_project]
    struct DecryptGenHandle<H> {
        handle: H,
        storage: Vec<u8>,
    }

    #[derive(Debug)]
    #[pin_project(project = DecryptGenPin)]
    enum DecryptGenState<F, R, H> {
        /// Initialize the read disk
        Disk(#[pin] F),
        /// Read from disk to a storage vector
        Read(#[pin] DecryptDiskRead<R>),
        /// Wait for the processing handle to complete
        Handle(#[pin] H),
    }

    #[derive(Debug)]
    #[pin_project]
    pub struct DecryptGen<'h, F, R, FD, H> {
        #[pin]
        state: DecryptGenState<F, R, H>,
        handle: &'h FD,
        secrets: &'h SecretBox<KeyNonce>,
    }

    /// Arbitrary initial size for the local vector to start with.
    ///
    /// 8 KiB, based on [`std::io::BufReader`] size as of Rust 1.80.
    const STORAGE_INIT_SIZE: usize = 8 * 1024;

    impl<F, R, FD, H> Future for DecryptGen<'_, F, R, FD, H>
    where
        R: AsyncRead,
        F: Future<Output = std::io::Result<R>>,
        FD: Fn(DecryptBlocking) -> H,
        H: Future<Output = std::io::Result<SecretReadVec>>,
    {
        type Output = H::Output;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            loop {
                let mut this = self.as_mut().project();
                let state = this.state.as_mut().project();

                match state {
                    DecryptGenPin::Disk(disk_fut) => {
                        let disk_res = ready!(disk_fut.poll(cx));
                        match disk_res {
                            Ok(disk) => {
                                this.state.set(DecryptGenState::Read(DecryptDiskRead {
                                    disk,
                                    storage: Vec::with_capacity(STORAGE_INIT_SIZE),
                                }));
                            }
                            Err(e) => {
                                return Poll::Ready(Err(e));
                            }
                        }
                    }
                    DecryptGenPin::Read(disk_read) => {
                        let disk_read = disk_read.project();
                        let disk = disk_read.disk;
                        let storage = disk_read.storage;

                        match ready!(disk.poll_read(cx, storage))? {
                            // Read complete
                            0 => {
                                let decrypted = (this.handle)(DecryptBlocking {
                                    loaded: storage.clone(),
                                    secrets: this.secrets.clone(),
                                });
                                this.state.set(DecryptGenState::Handle(decrypted));
                            }
                            // Read ongoing (> 0)
                            _ => {
                                // Double capacity if it's been filled
                                if storage.len() == storage.capacity() {
                                    storage.reserve(storage.capacity());
                                }
                            }
                        }
                    }
                    DecryptGenPin::Handle(handle_fut) => {
                        return handle_fut.poll(cx);
                    }
                }
            }
        }
    }

    impl<'h, F, R, FD, H> DecryptGen<'h, F, R, FD, H> {
        fn new(disk_fut: F, handle: &'h FD, secrets: &'h SecretBox<KeyNonce>) -> Self {
            Self {
                state: DecryptGenState::Disk(disk_fut),
                handle,
                secrets,
            }
        }
    }

    impl<'a, B, FD, FE, R> AsyncReadDisk for AsyncEncrypted<B, FD, FE>
    where
        B: AsyncReadDisk,
        FD: Fn(DecryptBlocking) -> R,
        R: Future<Output = std::io::Result<SecretReadVec>>,
    {
        type ReadDisk<'r> = SecretReadVec where Self: 'r;
        type ReadFut<'f> = DecryptGen<'f, B::ReadFut<'f>, B::ReadDisk<'f>, FD, R> where Self: 'f;

        fn async_read_disk(&self) -> Self::ReadFut<'_> {
            DecryptGen::new(
                self.inner.async_read_disk(),
                &self.decrypt_handle,
                &self.secrets,
            )
        }
    }

    /// [`BlockingFn`] that runs AES encryption.
    #[derive(Debug)]
    pub struct EncryptBlocking<'a> {
        buffer: SecretVecBuffer,
        secrets: &'a SecretBox<KeyNonce>,
    }

    impl BlockingFn for EncryptBlocking<'_> {
        type Output = std::io::Result<SecretVec<u8>>;

        fn call(mut self) -> Self::Output {
            let mut aes = Aes256Gcm::new(&self.secrets.borrow().key.into());
            aes.encrypt_in_place(
                GenericArray::from_slice(&self.secrets.borrow().nonce),
                &[],
                &mut self.buffer,
            )
            .map_err(|_| std::io::ErrorKind::Other)?;

            Ok(self.buffer.into())
        }
    }

    #[derive(Debug)]
    #[pin_project(project = AltOptionPin)]
    enum AltOption<T> {
        Some(#[pin] T),
        None,
    }

    #[pin_project]
    pub struct AsyncEncryptedWriter<'a, B, F, R> {
        #[pin]
        inner: B,
        buffer: SecretVecBuffer,
        secrets: &'a SecretBox<KeyNonce>,
        handle: &'a F,
        #[pin]
        flush_fut: AltOption<R>,
        flush_slice_start: usize,
    }

    impl<B, F, R> AsyncWrite for AsyncEncryptedWriter<'_, B, F, R>
    where
        B: AsyncWrite,
        F: Fn(EncryptBlocking) -> R,
        R: Future<Output = std::io::Result<SecretVecBuffer>>,
    {
        /// Stores bytes into an internal protected buffer.
        ///
        /// All bytes prior to flush need to be held in memory.
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<Result<usize, std::io::Error>> {
            self.project()
                .buffer
                .extend_from_slice(buf)
                .map_err(|_| std::io::ErrorKind::OutOfMemory)?;
            Poll::Ready(Ok(buf.len()))
        }

        /// Does nothing.
        ///
        /// The underlying encryption lib can only write in one go, so we wait
        /// until `poll_close` to pass over the buffer (since that invalidates
        /// the writer).
        fn poll_flush(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), std::io::Error>> {
            Poll::Ready(Ok(()))
        }

        /// Pass the buffer to the processing thread and write out encrypted.
        fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            let mut this = self.as_mut().project();

            loop {
                if let AltOptionPin::Some(fut) = this.flush_fut.as_mut().project() {
                    let buf = ready!(fut.poll(cx))?;

                    // Write in additional bytes until we've gone through
                    // the entire buffer.
                    if *this.flush_slice_start < buf.len() {
                        let slice_start = *this.flush_slice_start;
                        let advance_size = ready!(this
                            .inner
                            .as_mut()
                            .poll_write(cx, &buf.as_ref()[slice_start..]))?;

                        *this.flush_slice_start += advance_size;
                    }

                    ready!(this.inner.poll_close(cx))?;

                    // Clear out this future, non-flush ops can proceed
                    this.flush_fut.set(AltOption::None);

                    return Poll::Ready(Ok(()));
                } else {
                    // Initialize encryption on a new flush
                    this.flush_fut
                        .set(AltOption::Some((this.handle)(EncryptBlocking {
                            buffer: std::mem::take(&mut this.buffer),
                            secrets: this.secrets,
                        })));
                    *this.flush_slice_start = 0;
                }
            }
        }
    }

    impl<'a, B, F, R> AsyncEncryptedWriter<'a, B, F, R> {
        fn new(inner: B, secrets: &'a SecretBox<KeyNonce>, handle: &'a F) -> Self {
            Self {
                inner,
                buffer: SecretVecBuffer::default(),
                secrets,
                handle,
                flush_fut: AltOption::None,
                flush_slice_start: 0,
            }
        }
    }

    #[derive(Debug)]
    #[pin_project]
    pub struct AsyncEncryptedWriterGen<'a, F, H, R> {
        #[pin]
        fut: F,
        secrets: &'a SecretBox<KeyNonce>,
        handle: &'a H,
        _phantom: PhantomData<R>,
    }

    impl<'a, F, D, H, R> Future for AsyncEncryptedWriterGen<'a, F, H, R>
    where
        D: AsyncWrite,
        F: Future<Output = std::io::Result<D>>,
    {
        type Output = std::io::Result<AsyncEncryptedWriter<'a, D, H, R>>;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let disk = ready!(self.as_mut().project().fut.poll(cx))?;
            Poll::Ready(Ok(AsyncEncryptedWriter::new(
                disk,
                self.secrets,
                self.handle,
            )))
        }
    }

    impl<'a, F, H, R> AsyncEncryptedWriterGen<'a, F, H, R> {
        fn new(fut: F, secrets: &'a SecretBox<KeyNonce>, handle: &'a H) -> Self {
            Self {
                fut,
                secrets,
                handle,
                _phantom: PhantomData,
            }
        }
    }

    impl<B, FD, FE, R> AsyncWriteDisk for AsyncEncrypted<B, FD, FE>
    where
        B: AsyncWriteDisk,
        FE: Fn(EncryptBlocking) -> R,
        R: Future<Output = std::io::Result<SecretVecBuffer>>,
    {
        type WriteDisk<'w> = AsyncEncryptedWriter<'w, B::WriteDisk<'w>, FE, R> where Self: 'w;
        type WriteFut<'f> = AsyncEncryptedWriterGen<'f, B::WriteFut<'f>, FE, R> where Self: 'f;

        fn async_write_disk(&mut self) -> Self::WriteFut<'_> {
            // These are all separate fields, so the borrows don't overlap,
            // but the borrow checker can't figure this one out quite yet.

            let inner_ptr: *mut _ = &mut self.inner;
            let async_write_disk = unsafe { &mut *inner_ptr }.async_write_disk();

            let secrets = &self.secrets;
            let encrypt_handle = &self.encrypt_handle;

            AsyncEncryptedWriterGen::new(async_write_disk, secrets, encrypt_handle)
        }
    }
}
