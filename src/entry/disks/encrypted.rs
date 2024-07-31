use std::{
    borrow::BorrowMut,
    io::{Cursor, Read, Write},
    marker::PhantomData,
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    sync::OnceLock,
};

use aes_gcm::{
    aead::{generic_array::GenericArray, AeadMutInPlace, Buffer, OsRng},
    AeadCore, Aes256Gcm, KeyInit,
};
use secrets::{traits::Bytes, SecretBox, SecretVec};
use serde::{Deserialize, Serialize};

use crate::utils::BorrowExtender;

#[derive(Debug, Clone, Copy)]
struct KeyNonce {
    key: [u8; 32],
    nonce: [u8; 96],
}

unsafe impl Bytes for KeyNonce {}

fn random_key_nonce() -> SecretBox<KeyNonce> {
    SecretBox::random()
}

/// A resource encrypted with Aes256Gcm.
///
/// Secrets and decrypted data are stored in [`SecretBox`], which minimizes
/// the risk of memory being snooped (see [`secrets`] for details).
///
/// The secrets are excluded from serialization and deserialization, being
/// set with garbage data on deserialization. Use [`Self::set_key`] and
/// [`Self::set_nonce`] to initialize secrets after deserialization.
#[derive(Serialize, Deserialize)]
pub struct Encrypted<'a, B> {
    inner: B,
    #[serde(skip)]
    #[serde(default = "random_key_nonce")]
    secrets: SecretBox<KeyNonce>,
    _phantom: PhantomData<&'a ()>,
}

impl<B> From<Encrypted<'_, B>> for PathBuf
where
    PathBuf: From<B>,
{
    fn from(val: Encrypted<'_, B>) -> Self {
        Self::from(val.inner)
    }
}

impl<B: AsRef<Path>> AsRef<Path> for Encrypted<'_, B> {
    fn as_ref(&self) -> &Path {
        self.inner.as_ref()
    }
}

unsafe impl<B: Send> Send for Encrypted<'_, B> {}
unsafe impl<B: Sync> Sync for Encrypted<'_, B> {}

impl<B: Bytes> Encrypted<'_, B> {
    /// Create a new [`Encrypted`].
    ///
    /// # Parameters
    /// * `inner`: Underlying disk written to / read from
    /// * `key`: Aes256Gcm key. Generated with [`OsRng`] if not provided.
    /// * `nonce`: Aes256Gcm nonce. Generated with [`OsRng`] if not provided.
    pub fn new<K, N>(inner: B, key: Option<K>, nonce: Option<N>) -> Self
    where
        K: AsRef<[u8; 32]>,
        N: AsRef<[u8; 96]>,
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
            _phantom: PhantomData,
        }
    }

    pub fn set_key(&mut self, key: &SecretBox<[u8; 32]>) -> &mut Self {
        self.secrets
            .borrow_mut()
            .key
            .copy_from_slice(key.borrow().as_slice());
        self
    }

    pub fn set_nonce(&mut self, nonce: &SecretBox<[u8; 96]>) -> &mut Self {
        self.secrets
            .borrow_mut()
            .nonce
            .copy_from_slice(nonce.borrow().as_slice());
        self
    }

    pub fn get_key(&self) -> impl AsRef<[u8; 32]> + '_ {
        BorrowExtender::new(self.secrets.borrow(), |secrets| secrets.key)
    }

    pub fn get_nonce(&self) -> impl AsRef<[u8; 96]> + '_ {
        BorrowExtender::new(self.secrets.borrow(), |secrets| secrets.nonce)
    }
}

#[derive(Debug)]
struct SecretVecU8<'a>(BorrowExtender<SecretVec<u8>, secrets::secret_vec::Ref<'a, u8>>);

impl From<SecretVec<u8>> for SecretVecU8<'_> {
    fn from(value: SecretVec<u8>) -> Self {
        // Lifetime shenanigans
        Self(BorrowExtender::new(value, |value| {
            let value_ptr: *const _ = value;
            unsafe { &*value_ptr }.borrow()
        }))
    }
}

impl From<SecretVecBuffer<'_>> for SecretVecU8<'_> {
    fn from(value: SecretVecBuffer) -> Self {
        let value: SecretVec<u8> = value.into();
        Self::from(value)
    }
}

impl AsRef<[u8]> for SecretVecU8<'_> {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

/// A SecretVec implementing [`Buffer`].
#[derive(Debug)]
pub struct SecretVecBuffer<'a> {
    inner: SecretVec<u8>,
    ref_handle: OnceLock<secrets::secret_vec::Ref<'a, u8>>,
    mut_handle: Option<secrets::secret_vec::RefMut<'a, u8>>,
}

impl From<SecretVec<u8>> for SecretVecBuffer<'_> {
    fn from(value: SecretVec<u8>) -> Self {
        // Lifetime shenanigans
        Self {
            inner: value,
            ref_handle: OnceLock::new(),
            mut_handle: None,
        }
    }
}

impl Default for SecretVecBuffer<'_> {
    fn default() -> Self {
        SecretVec::<u8>::zero(0).into()
    }
}

impl Deref for SecretVecBuffer<'_> {
    type Target = SecretVec<u8>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl From<SecretVecBuffer<'_>> for SecretVec<u8> {
    fn from(mut val: SecretVecBuffer<'_>) -> Self {
        // Clear handles
        val.ref_handle = OnceLock::new();
        val.mut_handle = None;

        val.inner
    }
}

impl AsRef<[u8]> for SecretVecBuffer<'_> {
    fn as_ref(&self) -> &[u8] {
        self.ref_handle.get_or_init(|| {
            let this: *const _ = self;
            unsafe { &*this }.inner.borrow()
        })
    }
}

impl AsMut<[u8]> for SecretVecBuffer<'_> {
    fn as_mut(&mut self) -> &mut [u8] {
        if self.mut_handle.is_none() {
            let this: *mut _ = self;
            self.mut_handle = Some(unsafe { &mut *this }.inner.borrow_mut());
        };
        self.mut_handle.as_mut().unwrap()
    }
}

impl Buffer for SecretVecBuffer<'_> {
    fn len(&self) -> usize {
        self.inner.len()
    }
    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
    fn extend_from_slice(&mut self, other: &[u8]) -> aes_gcm::aead::Result<()> {
        // Clear handles
        self.ref_handle = OnceLock::new();
        self.mut_handle = None;

        let prev_len = self.inner.len();
        let new_len = self
            .inner
            .len()
            .checked_add(other.len())
            .ok_or(aes_gcm::aead::Error)?;
        self.inner = SecretVec::new(new_len, |s| {
            s.copy_from_slice(&self.inner.borrow());
            s[prev_len..].copy_from_slice(other);
        });
        Ok(())
    }
    fn truncate(&mut self, _len: usize) {
        // Clear handles
        self.ref_handle = OnceLock::new();
        self.mut_handle = None;

        // Always at exact length
    }
}

#[derive(Debug)]
pub struct SecretReadVec<'a> {
    inner: Cursor<SecretVecU8<'a>>,
}

impl From<SecretVec<u8>> for SecretReadVec<'_> {
    fn from(value: SecretVec<u8>) -> Self {
        Self {
            inner: Cursor::new(value.into()),
        }
    }
}

impl<'a> From<SecretVecU8<'a>> for SecretReadVec<'a> {
    fn from(value: SecretVecU8<'a>) -> Self {
        Self {
            inner: Cursor::new(value),
        }
    }
}

impl<'a> From<SecretVecBuffer<'a>> for SecretReadVec<'a> {
    fn from(value: SecretVecBuffer<'a>) -> Self {
        let value: SecretVecU8 = value.into();
        Self::from(value)
    }
}

impl Read for SecretReadVec<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        std::io::Read::read(&mut self.inner, buf)
    }
    fn read_vectored(&mut self, bufs: &mut [std::io::IoSliceMut<'_>]) -> std::io::Result<usize> {
        self.inner.borrow_mut().read_vectored(bufs)
    }
}

// Does not alias internal data, so ignore NonNull !Send + !Sync
unsafe impl Send for SecretReadVec<'_> {}
unsafe impl Sync for SecretReadVec<'_> {}

impl<'a, B: ReadDisk> ReadDisk for Encrypted<'a, B> {
    type ReadDisk = SecretReadVec<'a>;

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
pub struct EncryptedWriter<'a, B> {
    inner: B,
    secrets: SecretBox<KeyNonce>,
    buffer: SecretVecBuffer<'a>,
}

impl<B: Write> Write for EncryptedWriter<'_, B> {
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

impl<B> EncryptedWriter<'_, B> {
    fn new(inner: B, secrets: SecretBox<KeyNonce>) -> Self {
        Self {
            inner,
            secrets,
            buffer: SecretVecBuffer::default(),
        }
    }
}

impl<'a, B: WriteDisk> WriteDisk for Encrypted<'a, B> {
    type WriteDisk = EncryptedWriter<'a, B::WriteDisk>;

    fn write_disk(&self) -> std::io::Result<Self::WriteDisk> {
        Ok(EncryptedWriter::new(
            self.inner.write_disk()?,
            self.secrets.clone(),
        ))
    }
}

#[cfg(feature = "async")]
pub use async_impl::*;

use super::{ReadDisk, WriteDisk};
#[cfg(feature = "async")]
mod async_impl {

    use std::pin::Pin;
    use std::sync::{Arc, Mutex};
    use std::task::{Context, Poll};

    use crate::entry::disks::{AsyncReadDisk, AsyncWriteDisk};
    use crate::utils::blocking::BlockingFn;

    use super::*;

    use futures::Future;
    use tokio::io::{AsyncRead, AsyncWrite};
    use tokio::io::{AsyncReadExt, AsyncSeek};

    /// Async wrapper of [`Encrypted`], which derefs to the sync variant.
    ///
    /// Adds handles for blocking operations, see [`crate::utils::blocking`]
    /// for convenience functions.
    pub struct AsyncEncrypted<'a, B, FD, FE> {
        sync: Encrypted<'a, B>,
        decrypt_handle: FD,
        encrypt_handle: FE,
    }

    impl<B: Bytes, FD, FE> AsyncEncrypted<'_, B, FD, FE> {
        pub fn new<K, N>(
            inner: B,
            key: Option<K>,
            nonce: Option<N>,
            decrypt_handle: FD,
            encrypt_handle: FE,
        ) -> Self
        where
            K: AsRef<[u8; 32]>,
            N: AsRef<[u8; 96]>,
        {
            Self {
                sync: Encrypted::new(inner, key, nonce),
                decrypt_handle,
                encrypt_handle,
            }
        }
    }

    impl<'a, B, FD, FE> Deref for AsyncEncrypted<'a, B, FD, FE> {
        type Target = Encrypted<'a, B>;
        fn deref(&self) -> &Self::Target {
            &self.sync
        }
    }

    impl<B, FD, FE> DerefMut for AsyncEncrypted<'_, B, FD, FE> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.sync
        }
    }

    impl<'a, B: ReadDisk, FE, FD> ReadDisk for AsyncEncrypted<'a, B, FE, FD> {
        type ReadDisk = SecretReadVec<'a>;

        fn read_disk(&self) -> std::io::Result<Self::ReadDisk> {
            self.sync.read_disk()
        }
    }

    impl<'a, B: WriteDisk, FE, FD> WriteDisk for AsyncEncrypted<'a, B, FE, FD> {
        type WriteDisk = EncryptedWriter<'a, B::WriteDisk>;

        fn write_disk(&self) -> std::io::Result<Self::WriteDisk> {
            self.sync.write_disk()
        }
    }

    impl AsyncRead for SecretReadVec<'_> {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut tokio::io::ReadBuf,
        ) -> std::task::Poll<std::io::Result<()>> {
            Pin::new(&mut self.get_mut().inner).poll_read(cx, buf)
        }
    }

    impl AsyncSeek for SecretReadVec<'_> {
        fn poll_complete(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<u64>> {
            Pin::new(&mut self.get_mut().inner).poll_complete(cx)
        }
        fn start_seek(
            self: std::pin::Pin<&mut Self>,
            position: std::io::SeekFrom,
        ) -> std::io::Result<()> {
            Pin::new(&mut self.get_mut().inner).start_seek(position)
        }
    }

    /// [`BlockingFn`] that runs AES decryption.
    #[derive(Debug)]
    pub struct DecryptBlocking<'a> {
        loaded: Vec<u8>,
        secrets: SecretBox<KeyNonce>,
        _phantom: PhantomData<&'a ()>,
    }

    impl<'a> BlockingFn for DecryptBlocking<'a> {
        type Output = std::io::Result<SecretVecBuffer<'a>>;

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
            Ok(decrypted)
        }
    }

    impl<'a, B, FD, FE, R> AsyncReadDisk for AsyncEncrypted<'a, B, FD, FE>
    where
        B: AsyncReadDisk<ReadDisk: Sync + Send> + Sync + Send,
        FE: Send + Sync + Unpin,
        FD: Send + Sync + Unpin,
        R: Send + Sync + Unpin,
        FD: Fn(DecryptBlocking) -> R,
        FD: Fn(DecryptBlocking) -> R,
        R: Future<Output = std::io::Result<SecretVecBuffer<'a>>>,
    {
        type ReadDisk = SecretReadVec<'a>;

        async fn async_read_disk(&self) -> std::io::Result<Self::ReadDisk> {
            let mut loaded = Vec::new();
            let mut read_disk = self.inner.async_read_disk().await?;
            read_disk.read_to_end(&mut loaded).await?;

            let decrypted = (self.decrypt_handle)(DecryptBlocking {
                loaded,
                secrets: self.secrets.clone(),
                _phantom: PhantomData,
            })
            .await?;

            Ok(decrypted.into())
        }
    }

    /// [`BlockingFn`] that runs AES encryption.
    #[derive(Debug)]
    pub struct EncryptBlocking<'a> {
        buffer: Arc<Mutex<SecretVecBuffer<'a>>>,
        secrets: SecretBox<KeyNonce>,
    }

    impl<'a> BlockingFn for EncryptBlocking<'a> {
        type Output = std::io::Result<SecretVec<u8>>;

        fn call(self) -> Self::Output {
            let buffer = self.buffer.lock().unwrap();
            let mut buf: SecretVecBuffer = (**buffer).clone().into();

            let mut aes = Aes256Gcm::new(&self.secrets.borrow().key.into());
            aes.encrypt_in_place(
                GenericArray::from_slice(&self.secrets.borrow().nonce),
                &[],
                &mut buf,
            )
            .map_err(|_| std::io::ErrorKind::Other)?;

            Ok(buf.into())
        }
    }

    pub struct AsyncEncryptedWriter<'a, B, F, R> {
        inner: B,
        buffer: Arc<Mutex<SecretVecBuffer<'a>>>,
        secrets: SecretBox<KeyNonce>,
        handle: &'a F,
        flush_fut: R,
        flush_slice_start: usize,
    }

    impl<'a, B, F, R> AsyncWrite for AsyncEncryptedWriter<'a, B, F, R>
    where
        B: AsyncWrite + Unpin,
        R: Sync + Send + Unpin,
        F: Fn(EncryptBlocking) -> R,
        R: Future<Output = std::io::Result<SecretVecBuffer<'a>>>,
    {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> std::task::Poll<Result<usize, std::io::Error>> {
            self.get_mut()
                .buffer
                .lock()
                .unwrap()
                .extend_from_slice(buf)
                .map_err(|_| std::io::ErrorKind::OutOfMemory)?;
            Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Result<(), std::io::Error>> {
            let reset = |this: &mut Self| {
                this.flush_fut = (this.handle)(EncryptBlocking {
                    buffer: this.buffer.clone(),
                    secrets: this.secrets.clone(),
                });
                this.flush_slice_start = 0
            };

            let flush_fut_res = Pin::new(&mut self.flush_fut).poll(cx);
            if let Poll::Ready(buf) = flush_fut_res {
                let slice_start = self.flush_slice_start;
                let write_res =
                    Pin::new(&mut self.inner).poll_write(cx, &buf?.as_ref()[slice_start..]);

                match write_res {
                    Poll::Ready(Ok(0)) => {
                        // Done
                        let disk_progress = Pin::new(&mut self.inner).poll_flush(cx);
                        if matches!(disk_progress, Poll::Ready(_)) {
                            reset(&mut self);
                        }
                        disk_progress
                    }
                    Poll::Ready(Ok(x)) => {
                        // More bytes to go
                        self.flush_slice_start += x;
                        Poll::Pending
                    }
                    Poll::Ready(Err(x)) if x.kind() == std::io::ErrorKind::Interrupted => {
                        // Non-terminal error, does not produce its own wake
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    }
                    Poll::Ready(Err(x)) => {
                        // Error that invalidates flush
                        reset(&mut self);
                        Poll::Ready(Err(x))
                    }
                    Poll::Pending => Poll::Pending,
                }
            } else {
                Poll::Pending
            }
        }

        fn poll_shutdown(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Result<(), std::io::Error>> {
            if let Poll::Ready(poll_status) = self.as_mut().poll_flush(cx) {
                poll_status?;
                Pin::new(&mut self.inner).poll_shutdown(cx)
            } else {
                Poll::Pending
            }
        }
    }

    impl<'a, B, F, R> AsyncEncryptedWriter<'a, B, F, R> {
        fn new(inner: B, secrets: SecretBox<KeyNonce>, handle: &'a F) -> Self
        where
            B: AsyncWrite + Unpin,
            R: Sync + Send + Unpin,
            F: Fn(EncryptBlocking) -> R,
            R: Future,
        {
            #[allow(clippy::arc_with_non_send_sync)]
            let buffer = Arc::new(Mutex::default());
            Self {
                inner,
                buffer: buffer.clone(),
                secrets: secrets.clone(),
                handle,
                flush_fut: (handle)(EncryptBlocking { buffer, secrets }),
                flush_slice_start: 0,
            }
        }
    }

    // All internal data aliasing is via Mutex, so ignore NonNull !Send + !Sync
    unsafe impl<A, B, C> Send for AsyncEncryptedWriter<'_, A, B, C> {}
    unsafe impl<A, B, C> Sync for AsyncEncryptedWriter<'_, A, B, C> {}

    impl<'a, B, FD, FE, R> AsyncWriteDisk for AsyncEncrypted<'a, B, FD, FE>
    where
        B: AsyncWriteDisk<WriteDisk: Sync + Send> + Sync + Send,
        FE: Send + Sync + Unpin + 'a,
        FD: Send + Sync + Unpin,
        R: Sync + Send + Unpin,
        FE: Fn(EncryptBlocking) -> R,
        R: Future<Output = std::io::Result<SecretVecBuffer<'a>>> + Send + Sync,
    {
        type WriteDisk = AsyncEncryptedWriter<'a, B::WriteDisk, FE, R>;

        async fn async_write_disk(&self) -> std::io::Result<Self::WriteDisk> {
            let write_disk = self.inner.async_write_disk().await?;
            let handle_ptr: *const _ = &self.encrypt_handle;
            Ok(AsyncEncryptedWriter::new(
                write_disk,
                self.secrets.clone(),
                unsafe { &*handle_ptr },
            ))
        }
    }
}
