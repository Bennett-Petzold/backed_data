use num_traits::Unsigned;
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fmt::{Debug, Display},
    io::{BufRead, Write},
    marker::PhantomData,
    ops::Deref,
    path::{Path, PathBuf},
};
use zstd::{Decoder, Encoder};

#[cfg(feature = "async_zstd")]
use crate::entry::disks::{AsyncReadDisk, AsyncWriteDisk};

use super::{ReadDisk, WriteDisk};

#[cfg(feature = "async_zstd")]
use tokio::io::AsyncBufRead;

#[cfg(feature = "async_zstdmt")]
use async_compression::zstd::CParameter;

#[cfg(any(feature = "zstdmt", feature = "async_zstdmt"))]
use std::sync::{LazyLock, Mutex};

#[cfg(any(feature = "zstdmt", feature = "async_zstdmt"))]
pub static ZSTD_MULTITHREAD: LazyLock<Mutex<u32>> = LazyLock::new(|| Mutex::new(0));

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

#[cfg(feature = "zstd")]
impl<'a, const ZSTD_LEVEL: u8, B: ReadDisk<ReadDisk: BufRead>> ReadDisk
    for ZstdDisk<'a, ZSTD_LEVEL, B>
{
    type ReadDisk = zstd::Decoder<'a, B::ReadDisk>;

    fn read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        Decoder::with_buffer(self.inner.read_disk()?)
    }
}

pub struct ZstdEncoderWrapper<'a, T: Write> {
    encoder: Encoder<'a, T>,
}

impl<T: Write> Write for ZstdEncoderWrapper<'_, T> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.encoder.write(buf)
    }
    fn write_vectored(&mut self, bufs: &[std::io::IoSlice<'_>]) -> std::io::Result<usize> {
        self.encoder.write_vectored(bufs)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.encoder.do_finish()?;
        self.encoder.flush()?;
        self.encoder.get_mut().flush()
    }
}

#[cfg(feature = "zstd")]
impl<'a, const ZSTD_LEVEL: u8, B: WriteDisk> WriteDisk for ZstdDisk<'a, ZSTD_LEVEL, B> {
    type WriteDisk = ZstdEncoderWrapper<'a, B::WriteDisk>;

    fn write_disk(&self) -> std::io::Result<Self::WriteDisk> {
        #[allow(unused_mut)]
        let mut encoder = Encoder::new(
            self.inner.write_disk()?,
            *ZstdLevel::new(ZSTD_LEVEL).map_err(std::io::Error::other)?,
        )?;

        #[cfg(feature = "zstdmt")]
        {
            let level = *ZSTD_MULTITHREAD.lock().unwrap();
            if level > 0 {
                encoder.multithread(level).unwrap();
            }
        }

        Ok(ZstdEncoderWrapper { encoder })
    }
}

#[cfg(feature = "async_zstd")]
impl<const ZSTD_LEVEL: u8, B: AsyncReadDisk<ReadDisk: AsyncBufRead + Unpin> + Sync + Send>
    AsyncReadDisk for ZstdDisk<'_, ZSTD_LEVEL, B>
{
    type ReadDisk = async_compression::tokio::bufread::ZstdDecoder<B::ReadDisk>;

    async fn async_read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        Ok(async_compression::tokio::bufread::ZstdDecoder::new(
            self.inner.async_read_disk().await?,
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
            let level = *ZSTD_MULTITHREAD.lock().unwrap();
            let params: &[_] = if level > 0 {
                &[CParameter::nb_workers(*ZSTD_MULTITHREAD.lock().unwrap())]
            } else {
                &[]
            };

            Ok(
                async_compression::tokio::write::ZstdEncoder::with_quality_and_params(
                    disk,
                    async_compression::Level::Precise(
                        *ZstdLevel::new(ZSTD_LEVEL).map_err(std::io::Error::other)?,
                    ),
                    params,
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

// Miri does not appreciate FFI calls
#[cfg(all(test, not(miri)))]
mod tests {
    use std::{io::Cursor, io::Read, io::Write, sync::Mutex};

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

        let zstd = ZstdDisk::<0, _>::new(backing);
        let mut write = zstd.write_disk().unwrap();
        write.write_all(TEST_SEQUENCE).unwrap();
        write.flush().unwrap();

        // Reading from same struct
        let mut read = Vec::default();
        let _ = zstd.read_disk().unwrap().read_to_end(&mut read);
        assert_eq!(read, TEST_SEQUENCE);

        // Reading after drop and rebuild at a different compression
        let mut read = Vec::default();
        let _ = ZstdDisk::<18, _>::new(zstd.into_inner())
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

        let zstd = ZstdDisk::<0, _>::new(backing);
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
        ZstdDisk::<18, _>::new(zstd.into_inner())
            .async_read_disk()
            .await
            .unwrap()
            .read_to_end(&mut read)
            .await
            .unwrap();
        assert_eq!(read, TEST_SEQUENCE);
    }

    #[cfg(feature = "bincode")]
    #[test]
    fn encoded_zstd() {
        use std::io::Write;

        use crate::{
            entry::formats::{BincodeCoder, Decoder, Encoder},
            test_utils::OwnedCursorVec,
        };

        const TEST_SEQUENCE: &[u8] = &[39, 3, 6, 7, 5];
        let backing = OwnedCursorVec::new(Cursor::default());

        let zstd = ZstdDisk::<0, _>::new(backing);
        let mut write = zstd.write_disk().unwrap();
        BincodeCoder::<&[u8]>::default()
            .encode(&TEST_SEQUENCE, &mut write)
            .unwrap();
        write.flush().unwrap();

        // Reading from same struct
        let mut read = zstd.read_disk().unwrap();
        let bytes: Box<[u8]> = BincodeCoder::default().decode(&mut read).unwrap();
        assert_eq!(&*bytes, TEST_SEQUENCE);

        // Reading after drop and rebuild at a different compression
        let mut read = ZstdDisk::<18, _>::new(zstd.into_inner())
            .read_disk()
            .unwrap();
        let bytes: Box<[u8]> = BincodeCoder::default().decode(&mut read).unwrap();
        assert_eq!(&*bytes, TEST_SEQUENCE);
    }
}
