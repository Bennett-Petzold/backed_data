use std::{
    fs::File,
    io::{BufReader, BufWriter, Read, Write},
    path::PathBuf,
};

use serde::{Deserialize, Serialize};

#[cfg(feature = "async")]
use {
    futures::Future,
    tokio::io::{AsyncRead, AsyncWrite},
};

pub trait ReadDisk {
    type ReadDisk: Read;
    fn read_disk(&self) -> std::io::Result<Self::ReadDisk>;
}

pub trait WriteDisk {
    type WriteDisk: Write;
    fn write_disk(&self) -> std::io::Result<Self::WriteDisk>;
}

#[cfg(feature = "async")]
pub trait AsyncReadDisk: Unpin {
    type ReadDisk: AsyncRead + Unpin;
    fn async_read_disk(
        &self,
    ) -> impl Future<Output = std::io::Result<Self::ReadDisk>> + Send + Sync;
}

#[cfg(feature = "async")]
pub trait AsyncWriteDisk: Unpin {
    type WriteDisk: AsyncWrite + Unpin;
    fn async_write_disk(
        &self,
    ) -> impl Future<Output = std::io::Result<Self::WriteDisk>> + Send + Sync;
}

/// A regular file entry.
///
/// This is used to open a [`File`] on demand, but drop the handle when unused.
/// Large collections of [`BackedEntry`]s would otherwise risk overwhelming
/// OS limts on the number of open file descriptors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plainfile {
    /// File location.
    path: PathBuf,
}

impl From<PathBuf> for Plainfile {
    fn from(value: PathBuf) -> Self {
        Self { path: value }
    }
}

impl From<Plainfile> for PathBuf {
    fn from(val: Plainfile) -> Self {
        val.path
    }
}

impl Plainfile {
    pub fn new(path: PathBuf) -> Self {
        path.into()
    }
}

impl ReadDisk for Plainfile {
    type ReadDisk = BufReader<File>;

    fn read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        Ok(BufReader::new(File::open(self.path.clone())?))
    }
}

impl WriteDisk for Plainfile {
    type WriteDisk = BufWriter<File>;

    fn write_disk(&self) -> std::io::Result<Self::WriteDisk> {
        Ok(BufWriter::new(
            File::options()
                .write(true)
                .create(true)
                .truncate(true)
                .open(self.path.clone())?,
        ))
    }
}

#[cfg(feature = "async")]
impl AsyncReadDisk for Plainfile {
    type ReadDisk = tokio::io::BufReader<tokio::fs::File>;

    async fn async_read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        Ok(tokio::io::BufReader::new(
            tokio::fs::File::open(self.path.clone()).await?,
        ))
    }
}

#[cfg(feature = "async")]
impl AsyncWriteDisk for Plainfile {
    type WriteDisk = tokio::io::BufWriter<tokio::fs::File>;

    async fn async_write_disk(&self) -> std::io::Result<Self::WriteDisk> {
        Ok(tokio::io::BufWriter::new(
            tokio::fs::File::options()
                .write(true)
                .create(true)
                .truncate(true)
                .open(self.path.clone())
                .await?,
        ))
    }
}

#[cfg(any(feature = "zstd", feature = "async_zstd"))]
pub use zstd_disks::*;
#[cfg(any(feature = "zstd", feature = "async_zstd"))]
mod zstd_disks {
    use num_traits::Unsigned;
    use std::{
        fmt::Debug,
        io::{BufRead, Seek},
        marker::PhantomData,
        ops::Deref,
    };
    use zstd::{Decoder, Encoder};

    #[cfg(feature = "async_zstd")]
    use {
        std::{pin::Pin, task::Context},
        tokio::io::{AsyncBufRead, AsyncRead, AsyncSeek},
    };

    #[cfg(feature = "async_zstdmt")]
    use async_compression::zstd::CParameter;

    #[cfg(any(feature = "zstdmt", feature = "async_zstdmt"))]
    use {lazy_static::lazy_static, std::sync::Mutex};

    use super::*;

    #[cfg(any(feature = "zstdmt", feature = "async_zstdmt"))]
    lazy_static! {
        static ref ZSTD_MULTITHREAD: Mutex<u32> = Mutex::new(1);
    }

    /// Zstd compression level (<https://facebook.github.io/zstd/zstd_manual.html>).
    ///
    /// Bounded [0-22]. 0 is a default compression level, 1-22 is from lower
    /// compression to higher compression. >=20 are `--ultra` in ZSTD CLI.
    #[derive(Debug, Clone, Copy, Serialize, Deserialize)]
    pub struct ZstdLevel {
        level: i32,
    }

    impl From<ZstdLevel> for i32 {
        fn from(val: ZstdLevel) -> Self {
            val.level
        }
    }

    impl Deref for ZstdLevel {
        type Target = i32;
        fn deref(&self) -> &Self::Target {
            &self.level
        }
    }

    #[derive(Debug)]
    pub enum ZstdLevelError<N: TryInto<i32, Error: Debug>> {
        OutOfBounds(OutOfBounds),
        ConversionError(N::Error),
    }

    #[derive(Debug)]
    pub struct OutOfBounds(String);

    impl From<i32> for OutOfBounds {
        fn from(value: i32) -> Self {
            Self(format!("{value} is outside of [0-22]"))
        }
    }

    impl From<OutOfBounds> for String {
        fn from(val: OutOfBounds) -> Self {
            val.0
        }
    }

    impl<N: TryInto<i32, Error: Debug>> From<OutOfBounds> for ZstdLevelError<N> {
        fn from(value: OutOfBounds) -> Self {
            Self::OutOfBounds(value)
        }
    }

    impl ZstdLevel {
        pub fn new<N>(value: N) -> Result<Self, ZstdLevelError<N>>
        where
            N: Unsigned,
            N: TryInto<i32, Error: Debug>,
        {
            let level: i32 = value
                .try_into()
                .map_err(|e| ZstdLevelError::ConversionError(e))?;
            if level < 23 {
                Ok(Self { level })
            } else {
                Err(ZstdLevelError::OutOfBounds(level.into()))
            }
        }

        /// Construct in a constant context. If this fails, `value` is outside
        /// of [0, 22].
        pub const fn const_new(value: i32) -> Result<Self, i32> {
            if value >= 0 && value < 23 {
                Ok(Self { level: value })
            } else {
                Err(value)
            }
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ZstdDisk<'a, B> {
        inner: B,
        pub zstd_level: ZstdLevel,
        _phantom: PhantomData<&'a ()>,
    }

    impl From<PathBuf> for ZstdDisk<'_, Plainfile> {
        fn from(value: PathBuf) -> Self {
            Self {
                inner: value.into(),
                zstd_level: ZstdLevel::const_new(0).unwrap(),
                _phantom: PhantomData,
            }
        }
    }

    impl From<ZstdDisk<'_, Plainfile>> for PathBuf {
        fn from(val: ZstdDisk<Plainfile>) -> Self {
            Self::from(val.inner)
        }
    }

    impl<B> ZstdDisk<'_, B> {
        pub const fn new(inner: B, level: Option<ZstdLevel>) -> Self {
            Self {
                inner,
                zstd_level: match level {
                    Some(x) => x,
                    None => ZstdLevel { level: 0 },
                },
                _phantom: PhantomData,
            }
        }

        pub fn into_inner(self) -> B {
            self.inner
        }
    }

    pub struct ZstdDecoderWrapper<'a, B: BufRead>(Decoder<'a, B>);

    impl<B: BufRead> Read for ZstdDecoderWrapper<'_, B> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.0.read(buf)
        }
        fn read_vectored(
            &mut self,
            bufs: &mut [std::io::IoSliceMut<'_>],
        ) -> std::io::Result<usize> {
            self.0.read_vectored(bufs)
        }
    }

    impl<B: BufRead + Seek> Seek for ZstdDecoderWrapper<'_, B> {
        fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
            self.0.get_mut().seek(pos)
        }
    }

    #[cfg(feature = "zstd")]
    impl<'a, B: ReadDisk<ReadDisk: BufRead>> ReadDisk for ZstdDisk<'a, B> {
        type ReadDisk = ZstdDecoderWrapper<'a, B::ReadDisk>;

        fn read_disk(&self) -> std::io::Result<Self::ReadDisk> {
            Ok(ZstdDecoderWrapper(Decoder::with_buffer(
                self.inner.read_disk()?,
            )?))
        }
    }

    #[cfg(feature = "zstd")]
    impl<'a, B: WriteDisk> WriteDisk for ZstdDisk<'a, B> {
        type WriteDisk = Encoder<'a, B::WriteDisk>;

        fn write_disk(&self) -> std::io::Result<Self::WriteDisk> {
            #[allow(unused_mut)]
            let mut encoder = Encoder::new(self.inner.write_disk()?, *self.zstd_level)?;

            #[cfg(feature = "zstdmt")]
            encoder
                .multithread(*ZSTD_MULTITHREAD.lock().unwrap())
                .unwrap();

            Ok(encoder)
        }
    }

    #[cfg(feature = "async_zstd")]
    pub struct AsyncZstdDecoderWrapper<B: AsyncBufRead>(
        async_compression::tokio::bufread::ZstdDecoder<B>,
    );

    #[cfg(feature = "async_zstd")]
    impl<B: AsyncBufRead + Unpin> AsyncRead for AsyncZstdDecoderWrapper<B> {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut tokio::io::ReadBuf,
        ) -> std::task::Poll<std::io::Result<()>> {
            Pin::new(&mut (self.get_mut()).0).poll_read(cx, buf)
        }
    }

    #[cfg(feature = "async_zstd")]
    impl<B: AsyncBufRead + AsyncSeek + Unpin> AsyncSeek for AsyncZstdDecoderWrapper<B> {
        fn poll_complete(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<u64>> {
            Pin::new(&mut (self.get_mut()).0.get_mut()).poll_complete(cx)
        }
        fn start_seek(
            self: std::pin::Pin<&mut Self>,
            position: std::io::SeekFrom,
        ) -> std::io::Result<()> {
            Pin::new(&mut (self.get_mut()).0.get_mut()).start_seek(position)
        }
    }

    #[cfg(feature = "async_zstd")]
    impl<B: AsyncReadDisk<ReadDisk: AsyncBufRead + Unpin> + Sync + Send> AsyncReadDisk
        for ZstdDisk<'_, B>
    {
        type ReadDisk = AsyncZstdDecoderWrapper<B::ReadDisk>;

        async fn async_read_disk(&self) -> std::io::Result<Self::ReadDisk> {
            Ok(AsyncZstdDecoderWrapper(
                async_compression::tokio::bufread::ZstdDecoder::new(
                    self.inner.async_read_disk().await?,
                ),
            ))
        }
    }

    #[cfg(feature = "async_zstd")]
    impl<B: AsyncWriteDisk + Send + Sync> AsyncWriteDisk for ZstdDisk<'_, B> {
        type WriteDisk = async_compression::tokio::write::ZstdEncoder<B::WriteDisk>;

        async fn async_write_disk(&self) -> std::io::Result<Self::WriteDisk> {
            let disk = self.inner.async_write_disk().await?;

            #[cfg(feature = "async_zstdmt")]
            {
                Ok(
                    async_compression::tokio::write::ZstdEncoder::with_quality_and_params(
                        disk,
                        async_compression::Level::Precise(*self.zstd_level),
                        &[CParameter::nb_workers(*ZSTD_MULTITHREAD.lock().unwrap())],
                    ),
                )
            }

            #[cfg(not(feature = "async_zstdmt"))]
            {
                Ok(async_compression::tokio::write::ZstdEncoder::with_quality(
                    disk,
                    async_compression::Level::Precise(*self.zstd_level),
                ))
            }
        }
    }

    // Miri does not appreciate FFI calls
    #[cfg(all(test, not(miri)))]
    mod tests {
        use std::{io::Cursor, sync::Mutex};

        use crate::test_utils::CursorVec;

        use super::*;

        #[test]
        fn sync_zstd() {
            const TEST_SEQUENCE: &[u8] = &[39, 3, 6, 7, 5];
            let mut cursor = Cursor::default();
            let backing = CursorVec {
                inner: Mutex::new(&mut cursor),
            };
            assert!(backing.get_ref().is_empty());

            let zstd = ZstdDisk::new(backing, None);
            let mut write = zstd.write_disk().unwrap();
            write.write_all(TEST_SEQUENCE).unwrap();
            write.flush().unwrap();

            // Reading from same struct
            let mut read = Vec::default();
            let _ = zstd.read_disk().unwrap().read_to_end(&mut read);
            assert_eq!(read, TEST_SEQUENCE);

            // Reading after drop and rebuild at a different compression
            let mut read = Vec::default();
            let _ = ZstdDisk::new(zstd.into_inner(), Some(ZstdLevel::const_new(18).unwrap()))
                .read_disk()
                .unwrap()
                .read_to_end(&mut read);
            assert_eq!(read, TEST_SEQUENCE);
        }

        #[cfg(feature = "async_zstd")]
        #[tokio::test]
        async fn async_zstd() {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};

            const TEST_SEQUENCE: &[u8] = &[39, 3, 6, 7, 5];
            let mut cursor = Cursor::default();
            let backing = CursorVec {
                inner: Mutex::new(&mut cursor),
            };
            assert!(backing.get_ref().is_empty());

            let zstd = ZstdDisk::new(backing, None);
            let mut write = zstd.async_write_disk().await.unwrap();
            write.write_all(TEST_SEQUENCE).await.unwrap();
            write.flush().await.unwrap();
            write.shutdown().await.unwrap();

            // Reading from same struct
            let mut read = Vec::default();
            zstd.async_read_disk()
                .await
                .unwrap()
                .read_to_end(&mut read)
                .await
                .unwrap();
            assert_eq!(read, TEST_SEQUENCE);

            // Reading after drop and rebuild at a different compression
            let mut read = Vec::default();
            ZstdDisk::new(zstd.into_inner(), Some(ZstdLevel::const_new(18).unwrap()))
                .async_read_disk()
                .await
                .unwrap()
                .read_to_end(&mut read)
                .await
                .unwrap();
            assert_eq!(read, TEST_SEQUENCE);
        }
    }
}

#[cfg(feature = "encrypted")]
pub use encrypted_disks::*;
#[cfg(feature = "encrypted")]
mod encrypted_disks {
    use std::{
        borrow::BorrowMut,
        io::Cursor,
        marker::PhantomData,
        ops::{Deref, DerefMut},
        sync::OnceLock,
    };

    use aes_gcm::{
        aead::{generic_array::GenericArray, AeadMutInPlace, Buffer, OsRng},
        AeadCore, Aes256Gcm, KeyInit,
    };
    use secrets::{traits::Bytes, SecretBox, SecretVec};

    use crate::utils::BorrowExtender;

    use super::*;

    #[derive(Debug, Clone, Copy)]
    struct KeyNonce {
        key: [u8; 32],
        nonce: [u8; 96],
    }

    unsafe impl Bytes for KeyNonce {}

    /// A resource encrypted with Aes256Gcm.
    ///
    /// Secrets and decrypted data are stored in [`SecretBox`], which minimizes
    /// the risk of memory being snooped (see [`secrets`] for details).
    pub struct Encrypted<'a, B> {
        inner: B,
        secrets: SecretBox<KeyNonce>,
        _phantom: PhantomData<&'a ()>,
    }

    unsafe impl<B: Send> Send for Encrypted<'_, B> {}
    unsafe impl<B: Sync> Sync for Encrypted<'_, B> {}

    impl<B: Bytes> Encrypted<'_, B> {
        pub fn new(
            inner: B,
            key: Option<&SecretBox<[u8; 32]>>,
            nonce: Option<&SecretBox<[u8; 96]>>,
        ) -> Self {
            Self {
                inner,
                secrets: SecretBox::new(|s| {
                    *s = KeyNonce {
                        key: key
                            .map(|k| *k.borrow())
                            .unwrap_or(Aes256Gcm::generate_key(OsRng).into()),
                        nonce: nonce.map(|n| *n.borrow()).unwrap_or(
                            Aes256Gcm::generate_nonce(&mut OsRng)
                                .as_slice()
                                .try_into()
                                .unwrap(),
                        ),
                    };
                }),
                _phantom: PhantomData,
            }
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
        fn read_vectored(
            &mut self,
            bufs: &mut [std::io::IoSliceMut<'_>],
        ) -> std::io::Result<usize> {
            self.inner.borrow_mut().read_vectored(bufs)
        }
    }

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
    #[cfg(feature = "async")]
    mod async_impl {

        use std::pin::Pin;
        use std::sync::{Arc, Mutex};
        use std::task::{Context, Poll};

        use crate::utils::blocking::BlockingFn;

        use super::*;

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
            pub fn new<F, R>(
                inner: B,
                key: Option<&SecretBox<[u8; 32]>>,
                nonce: Option<&SecretBox<[u8; 96]>>,
                decrypt_handle: FD,
                encrypt_handle: FE,
            ) -> Self {
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
                        Pin::new(&mut self.inner).poll_write(cx, &buf?.borrow()[slice_start..]);

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
}
