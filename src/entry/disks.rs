use std::{
    fs::File,
    io::{BufReader, BufWriter, Read, Write},
    path::PathBuf,
};

use serde::{Deserialize, Serialize};

pub trait ReadDisk: Serialize + for<'de> Deserialize<'de> {
    type ReadDisk: Read;
    fn read_disk(&self) -> std::io::Result<Self::ReadDisk>;
}

pub trait WriteDisk: Serialize + for<'de> Deserialize<'de> {
    type WriteDisk: Write;
    fn write_disk(&self) -> std::io::Result<Self::WriteDisk>;
}

/// A regular file entry.
///
/// This is used to open a [`File`] on demand, but drop the handle when unused.
/// Large collections of [`BackedEntry`]s would otherwise risk overwhelming
/// OS limts on the number of open file descriptors.
///
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

#[cfg(any(feature = "zstd", feature = "async-zstd"))]
pub use zstd_disks::*;
#[cfg(any(feature = "zstd", feature = "async-zstd"))]
mod zstd_disks {
    use num_traits::Unsigned;
    use std::{
        fmt::Debug,
        io::{BufRead, Seek},
        marker::PhantomData,
        ops::Deref,
    };
    use zstd::{Decoder, Encoder};

    #[cfg(any(feature = "zstdmt", feature = "async-zstdmt"))]
    use {lazy_static::lazy_static, std::sync::Mutex};

    use super::*;

    #[cfg(any(feature = "zstdmt", feature = "async-zstdmt"))]
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

    #[cfg(test)]
    mod tests {
        use std::{env::temp_dir, io::Cursor, sync::Mutex};

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
            write.write_all(TEST_SEQUENCE);
            write.flush().unwrap();

            // Reading from same struct
            let mut read = Vec::default();
            zstd.read_disk().unwrap().read_to_end(&mut read);
            assert_eq!(read, TEST_SEQUENCE);

            // Reading after drop and rebuild at a different compression
            let mut read = Vec::default();
            ZstdDisk::new(zstd.into_inner(), Some(ZstdLevel::const_new(18).unwrap()))
                .read_disk()
                .unwrap()
                .read_to_end(&mut read);
            assert_eq!(read, TEST_SEQUENCE);
        }
    }
}
