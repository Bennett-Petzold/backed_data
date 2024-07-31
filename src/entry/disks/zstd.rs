use num_traits::Unsigned;
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fmt::{Debug, Display},
    io::{BufRead, Read, Seek},
    marker::PhantomData,
    ops::Deref,
    path::{Path, PathBuf},
};
use zstd::{Decoder, Encoder};

#[cfg(feature = "async_zstd")]
use crate::entry::disks::{AsyncReadDisk, AsyncWriteDisk};

use super::{ReadDisk, WriteDisk};

#[cfg(feature = "async_zstd")]
use {
    std::{pin::Pin, task::Context},
    tokio::io::{AsyncBufRead, AsyncRead, AsyncSeek},
};

#[cfg(feature = "async_zstdmt")]
use async_compression::zstd::CParameter;

#[cfg(any(feature = "zstdmt", feature = "async_zstdmt"))]
use {lazy_static::lazy_static, std::sync::Mutex};

#[cfg(any(feature = "zstdmt", feature = "async_zstdmt"))]
lazy_static! {
    pub static ref ZSTD_MULTITHREAD: Mutex<u32> = Mutex::new(1);
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

impl<N: TryInto<i32, Error: Debug> + Debug> Display for ZstdLevelError<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as Debug>::fmt(self, f)
    }
}

impl<N: TryInto<i32, Error: Debug> + Debug> Error for ZstdLevelError<N> {}

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

/// Uses ZSTD to encode/decode to an underlying disk.
///
/// ZSTD_LEVEL is bounded by [`ZstdLevel`]'s constraints, returning an error
/// when out of bounds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZstdDisk<'a, const ZSTD_LEVEL: u8, B> {
    inner: B,
    _phantom: PhantomData<&'a ()>,
}

impl<const ZSTD_LEVEL: u8, B: From<PathBuf>> From<PathBuf> for ZstdDisk<'_, ZSTD_LEVEL, B> {
    fn from(value: PathBuf) -> Self {
        Self {
            inner: value.into(),
            _phantom: PhantomData,
        }
    }
}

impl<const ZSTD_LEVEL: u8, B> From<ZstdDisk<'_, ZSTD_LEVEL, B>> for PathBuf
where
    PathBuf: From<B>,
{
    fn from(val: ZstdDisk<ZSTD_LEVEL, B>) -> Self {
        Self::from(val.inner)
    }
}

impl<const ZSTD_LEVEL: u8, B: AsRef<Path>> AsRef<Path> for ZstdDisk<'_, ZSTD_LEVEL, B> {
    fn as_ref(&self) -> &Path {
        self.inner.as_ref()
    }
}

impl<const ZSTD_LEVEL: u8, B> ZstdDisk<'_, ZSTD_LEVEL, B> {
    pub const fn new(inner: B) -> Self {
        Self {
            inner,
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
    fn read_vectored(&mut self, bufs: &mut [std::io::IoSliceMut<'_>]) -> std::io::Result<usize> {
        self.0.read_vectored(bufs)
    }
}

impl<B: BufRead + Seek> Seek for ZstdDecoderWrapper<'_, B> {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.0.get_mut().seek(pos)
    }
}

#[cfg(feature = "zstd")]
impl<'a, const ZSTD_LEVEL: u8, B: ReadDisk<ReadDisk: BufRead>> ReadDisk
    for ZstdDisk<'a, ZSTD_LEVEL, B>
{
    type ReadDisk = ZstdDecoderWrapper<'a, B::ReadDisk>;

    fn read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        Ok(ZstdDecoderWrapper(Decoder::with_buffer(
            self.inner.read_disk()?,
        )?))
    }
}

#[cfg(feature = "zstd")]
impl<'a, const ZSTD_LEVEL: u8, B: WriteDisk> WriteDisk for ZstdDisk<'a, ZSTD_LEVEL, B> {
    type WriteDisk = Encoder<'a, B::WriteDisk>;

    fn write_disk(&self) -> std::io::Result<Self::WriteDisk> {
        #[allow(unused_mut)]
        let mut encoder = Encoder::new(
            self.inner.write_disk()?,
            *ZstdLevel::new(ZSTD_LEVEL).map_err(std::io::Error::other)?,
        )?;

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
impl<const ZSTD_LEVEL: u8, B: AsyncReadDisk<ReadDisk: AsyncBufRead + Unpin> + Sync + Send>
    AsyncReadDisk for ZstdDisk<'_, ZSTD_LEVEL, B>
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
impl<B: AsyncWriteDisk + Send + Sync, const ZSTD_LEVEL: u8> AsyncWriteDisk
    for ZstdDisk<'_, ZSTD_LEVEL, B>
{
    type WriteDisk = async_compression::tokio::write::ZstdEncoder<B::WriteDisk>;

    async fn async_write_disk(&self) -> std::io::Result<Self::WriteDisk> {
        let disk = self.inner.async_write_disk().await?;

        #[cfg(feature = "async_zstdmt")]
        {
            Ok(
                async_compression::tokio::write::ZstdEncoder::with_quality_and_params(
                    disk,
                    async_compression::Level::Precise(
                        *ZstdLevel::new(ZSTD_LEVEL).map_err(std::io::Error::other)?,
                    ),
                    &[CParameter::nb_workers(*ZSTD_MULTITHREAD.lock().unwrap())],
                ),
            )
        }

        #[cfg(not(feature = "async_zstdmt"))]
        {
            Ok(async_compression::tokio::write::ZstdEncoder::with_quality(
                disk,
                async_compression::Level::Precise(
                    *ZstdLevel::new(ZSTD_LEVEL).map_err(std::io::Error::other)?,
                ),
            ))
        }
    }
}

/*
// Miri does not appreciate FFI calls
#[cfg(all(test, not(miri)))]
mod tests {
    use std::{io::Cursor, io::Write, sync::Mutex};

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

        let zstd = ZstdDisk::new(backing);
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
*/
