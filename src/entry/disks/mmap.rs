use std::{
    cell::UnsafeCell,
    cmp::max,
    fs::File,
    io::{Cursor, ErrorKind, Write},
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use memmap2::MmapOptions;
use serde::{Deserialize, Serialize};

#[cfg(target_family = "unix")]
use memmap2::Advice;

#[cfg(target_os = "linux")]
use memmap2::RemapOptions;

use super::{ReadDisk, WriteDisk};

fn open_mmap_file<P: AsRef<Path>>(path: P) -> std::io::Result<File> {
    let f = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path.as_ref())?;

    Ok(f)
}

/// Read-only [`memmap2::Mmap`] that handles zero-length on Windows.
#[derive(Debug)]
pub enum ReadMmap {
    Empty,
    NonEmpty(memmap2::Mmap),
}

impl AsRef<[u8]> for ReadMmap {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Empty => &[],
            Self::NonEmpty(x) => x.as_ref(),
        }
    }
}

impl Deref for ReadMmap {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Empty => &[],
            Self::NonEmpty(x) => x.as_ref(),
        }
    }
}

impl ReadMmap {
    #[cfg(target_family = "unix")]
    pub fn advise(&self, advice: memmap2::Advice) -> std::io::Result<()> {
        if let Self::NonEmpty(mmap) = self {
            mmap.advise(advice)
        } else {
            Ok(())
        }
    }

    pub fn new_arc<P: AsRef<Path>>(path: P) -> std::io::Result<Arc<Self>> {
        let file = open_mmap_file(path)?;

        let mmap = if file.metadata()?.len() == 0 {
            Self::Empty
        } else {
            Self::NonEmpty(unsafe { MmapOptions::new().map(&file) }?)
        };

        #[cfg(target_family = "unix")]
        let _ = mmap.advise(Advice::PopulateRead);

        Ok(Arc::new(mmap))
    }
}

// ---------- Internal types ---------- //

#[derive(Debug)]
pub struct ReadMmapGuard(Arc<ReadMmap>);

impl AsRef<[u8]> for ReadMmapGuard {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl Deref for ReadMmapGuard {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

/// This is always issued so that the MmapWriter access is unique.
#[derive(Debug)]
pub struct WriteMmapGuard(Arc<WriteMmap>);

impl AsRef<[u8]> for WriteMmapGuard {
    fn as_ref(&self) -> &[u8] {
        let this = unsafe { &*self.0.get() };
        this.as_ref()
    }
}

impl AsMut<[u8]> for WriteMmapGuard {
    fn as_mut(&mut self) -> &mut [u8] {
        let this = unsafe { &mut *self.0.get() };
        this.as_mut()
    }
}

impl Deref for WriteMmapGuard {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl DerefMut for WriteMmapGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut()
    }
}

impl Write for WriteMmapGuard {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let this = unsafe { &mut *self.0.get() };
        this.write(buf)
    }
    fn write_vectored(&mut self, bufs: &[std::io::IoSlice<'_>]) -> std::io::Result<usize> {
        let this = unsafe { &mut *self.0.get() };
        this.write_vectored(bufs)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let this = unsafe { &mut *self.0.get() };
        this.flush()
    }
}

/// Adding [`Send`] and [`Sync`] to [`UnsafeCell`] on [`MmapWriter`].
///
/// The `!Send + !Sync` bounds are just a lint from the standard library, not
/// actual unsafety.
#[derive(Debug)]
pub struct WriteMmap(UnsafeCell<MmapWriter>);

impl From<WriteMmap> for UnsafeCell<MmapWriter> {
    fn from(val: WriteMmap) -> Self {
        val.0
    }
}

impl From<MmapWriter> for WriteMmap {
    fn from(value: MmapWriter) -> Self {
        Self(UnsafeCell::new(value))
    }
}

impl WriteMmap {
    pub fn into_inner(self) -> MmapWriter {
        self.0.into_inner()
    }

    pub fn get(&self) -> *mut MmapWriter {
        self.0.get()
    }
}

unsafe impl Send for WriteMmap {}
unsafe impl Sync for WriteMmap {}

/// Guards an inner mmap to match Rust mutability requirements.
///
/// Allows >= 1 outstanding mmap instances for immutable access, 1 outstanding
/// mmap instance for mutable access via a write lock, or no outstanding
/// accesses.
#[derive(Debug, Clone)]
pub enum SwitchingMmap {
    /// Previously failed irrecoverably, so no mmap in memory.
    Invalid,
    ReadMmap(Arc<ReadMmap>),
    WriteMmap(Arc<WriteMmap>),
}

impl SwitchingMmap {
    fn init_read<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        Ok(Self::ReadMmap(ReadMmap::new_arc(path)?))
    }

    /// Return self with either a valid guard, or an error.
    ///
    /// In the valid guard case, this is the
    /// [`ReadMmap`](`SwitchingMmap::ReadMmap`) variant. In the error case,
    /// this may be either [`WriteMmap`](`SwitchingMmap::WriteMmap`) or
    /// [`Invalid`](`SwitchingMmap::Invalid`).
    fn read_handle<P: AsRef<Path>>(
        self,
        path: P,
    ) -> Result<(Self, ReadMmapGuard), (Self, std::io::Error)> {
        let read_ret = |mmap: Arc<ReadMmap>| {
            let guard = ReadMmapGuard(mmap.clone());
            Ok((Self::ReadMmap(mmap), guard))
        };

        match self {
            Self::WriteMmap(x) => match Arc::try_unwrap(x) {
                Ok(x) => {
                    let writer = x.into_inner();
                    let mmap = Arc::new(if writer.written_len > 0 {
                        match writer.get_map() {
                            Ok(map) => match map.make_read_only() {
                                Ok(read_map) => ReadMmap::NonEmpty(read_map),
                                Err(e) => {
                                    return Err((Self::Invalid, e));
                                }
                            },
                            Err((writer, e)) => {
                                return Err((Self::WriteMmap(Arc::new(writer.into())), e));
                            }
                        }
                    } else {
                        ReadMmap::Empty
                    });

                    #[cfg(target_os = "linux")]
                    let _ = mmap.advise(Advice::PopulateRead);

                    read_ret(mmap)
                }
                Err(arc) => Err((
                    Self::WriteMmap(arc),
                    std::io::Error::new(ErrorKind::Other, "A write handle is currently open"),
                )),
            },
            Self::ReadMmap(x) => read_ret(x),
            Self::Invalid => match ReadMmap::new_arc(path) {
                Ok(x) => read_ret(x),
                Err(e) => Err((Self::Invalid, e)),
            },
        }
    }

    /// Return self with either a valid guard, or an error.
    ///
    /// In the valid guard case, this is the
    /// [`WriteMmap`](`SwitchingMmap::WriteMmap`) variant. In the error case,
    /// this may be either [`ReadMmap`](`SwitchingMmap::ReadMmap`) or
    /// [`Invalid`](`SwitchingMmap::Invalid`).
    fn write_handle<P: AsRef<Path>>(
        self,
        path: P,
    ) -> Result<(Self, WriteMmapGuard), (Self, std::io::Error)> {
        let writer = match self {
            Self::WriteMmap(x) => match Arc::try_unwrap(x) {
                Ok(mmap) => mmap,
                Err(arc) => {
                    return Err((
                        Self::WriteMmap(arc),
                        std::io::Error::new(ErrorKind::Other, "A write handle is currently open"),
                    ));
                }
            },
            Self::ReadMmap(x) => match Arc::try_unwrap(x) {
                Ok(mmap) => match mmap {
                    ReadMmap::NonEmpty(read_mmap) => match read_mmap
                        .make_mut()
                        .and_then(|write_mmap| MmapWriter::from_args(path, write_mmap))
                    {
                        Ok(write_mmap) => write_mmap.into(),
                        Err(e) => {
                            return Err((Self::Invalid, e));
                        }
                    },
                    ReadMmap::Empty => match MmapWriter::no_advise(path) {
                        Ok(mmap) => mmap.into(),
                        Err(e) => {
                            return Err((Self::Invalid, e));
                        }
                    },
                },
                Err(arc) => {
                    return Err((
                        Self::ReadMmap(arc),
                        std::io::Error::new(ErrorKind::Other, "A read handle is currently open"),
                    ));
                }
            },
            Self::Invalid => match MmapWriter::no_advise(path) {
                Ok(mmap) => mmap.into(),
                Err(e) => {
                    return Err((Self::Invalid, e));
                }
            },
        };

        let writer = Arc::new(writer);

        let guard = WriteMmapGuard(writer.clone());
        Ok((Self::WriteMmap(writer), guard))
    }
}

// ---------- END Internal types ---------- //

/// An mmaped file entry.
///
/// This is used to open a [`File`] on demand, but drop the handle when unused.
/// Large collections of [`BackedEntry`](`super::super::BackedEntry`)s would otherwise risk overwhelming
/// OS limts on the number of open file descriptors.
#[derive(Debug)]
pub struct Mmap {
    /// File location.
    path: PathBuf,
    mmap: Mutex<SwitchingMmap>,
}

// ---------- Serde impls ---------- //

impl Serialize for Mmap {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.path.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Mmap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let path = PathBuf::deserialize(deserializer)?;
        let mmap = SwitchingMmap::init_read(&path)
            .map_err(serde::de::Error::custom)?
            .into();

        Ok(Mmap { path, mmap })
    }
}

// ---------- END Serde impls ---------- //

impl TryFrom<PathBuf> for Mmap {
    type Error = std::io::Error;
    fn try_from(value: PathBuf) -> std::io::Result<Self> {
        let mmap = SwitchingMmap::init_read(&value)?.into();
        Ok(Self { path: value, mmap })
    }
}

impl From<Mmap> for PathBuf {
    fn from(val: Mmap) -> Self {
        val.path
    }
}

impl AsRef<Path> for Mmap {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl Mmap {
    pub fn new(path: PathBuf) -> std::io::Result<Self> {
        path.try_into()
    }
}

pub type ReadMmapCursor = Cursor<ReadMmapGuard>;

impl ReadDisk for Mmap {
    type ReadDisk = ReadMmapCursor;

    fn read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        // Shuffle out the current mmap value to replace later.
        let mut held_mmap = self.mmap.lock().unwrap();
        let cur_mmap = std::mem::replace(&mut *held_mmap, SwitchingMmap::Invalid);

        match cur_mmap.read_handle(&self.path) {
            Ok((mmap, guard)) => {
                *held_mmap = mmap;
                Ok(Cursor::new(guard))
            }
            Err((mmap, error)) => {
                *held_mmap = mmap;
                Err(error)
            }
        }
    }
}

#[derive(Debug)]
/// Wraps a mutable mmap,
pub struct MmapWriter {
    path: PathBuf,
    mmap: memmap2::MmapMut,
    written_len: usize,
    reserved_len: usize,
    #[cfg(target_os = "linux")]
    // True if this is a sparse file, to avoid excessive checks on failure
    sparse_file: bool,
}

impl MmapWriter {
    fn from_args<P: AsRef<Path>>(path: P, mmap: memmap2::MmapMut) -> std::io::Result<Self> {
        #[cfg(target_os = "linux")]
        let _ = mmap.advise(Advice::PopulateWrite);

        let reserved_len = mmap.len();

        Ok(MmapWriter {
            path: path.as_ref().to_path_buf(),
            mmap,
            written_len: 0,
            reserved_len,
            #[cfg(target_os = "linux")]
            sparse_file: true,
        })
    }

    fn no_advise<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let file = open_mmap_file(&path)?;

        // If the file is empty, we need to push in a garbage byte for mmap to
        // succeed.
        if file.metadata()?.len() == 0 {
            file.set_len(1)?;
        }

        let mmap = unsafe { MmapOptions::new().map_mut(&file) }?;

        let reserved_len = mmap.len();

        Ok(MmapWriter {
            path: path.as_ref().to_path_buf(),
            mmap,
            written_len: 0,
            reserved_len,
            #[cfg(target_os = "linux")]
            sparse_file: true,
        })
    }

    fn file(&self) -> std::io::Result<File> {
        open_mmap_file(&self.path)
    }

    /// Test if a newly `ftruncate`-allocated file as a hole in it, to
    /// determine if this filesystem supports holes.
    ///
    /// Returns true if there is a hole before the end, false otherwise.
    /// May return an I/O failure instead of a result.
    #[cfg(target_os = "linux")]
    fn test_for_hole(file: &File) -> std::io::Result<bool> {
        use std::os::fd::AsRawFd;

        use nix::unistd::{lseek, Whence};

        let hole_pos = lseek(file.as_raw_fd(), 0, Whence::SeekHole)?;
        let end_pos = lseek(file.as_raw_fd(), 0, Whence::SeekEnd)?;
        Ok(hole_pos != end_pos)
    }

    /// Use an ftruncate call on Linux, to attempt a small sparse insert.
    #[cfg(target_os = "linux")]
    #[inline]
    fn reserve_ftruncate(&mut self, file: &File, new_reserved_len: usize) -> std::io::Result<()> {
        use std::os::fd::AsFd;

        use nix::unistd::ftruncate;

        // Make sure the reservation length is on filesystem blocks
        let new_reserved_len_i64: i64 = new_reserved_len.try_into().map_err(|e| {
            std::io::Error::new(
                ErrorKind::OutOfMemory,
                format!("Cannot convert {} to an i64: {:#?}", self.reserved_len, e),
            )
        })?;

        ftruncate(file.as_fd(), new_reserved_len_i64)?;

        self.reserved_len = new_reserved_len;

        Ok(())
    }

    /// Linux optimization for reserve, when the filesystem has holes.
    ///
    /// Takes the chance to make a much larger reservation, knowing that holes
    /// will prevent all that space from actually being allocated until used.
    #[cfg(target_os = "linux")]
    #[inline]
    fn reserve_sparse(&mut self, file: &File) -> std::io::Result<()> {
        use std::os::fd::AsFd;

        use nix::unistd::ftruncate;

        // Chosen based on smallest max file size for largest EXT3.
        // This library does not support filesystems with less available space
        // than EXT3.
        const EIGHT_GIB: i64 = 8 * 1024 * 1024 * 1024;

        match Self::test_for_hole(file) {
            // Found a hole, try to add a much larger hole to
            // perform fewer file resizes during mmap write.
            Ok(true) => {
                // If the previous step succeeded, this must be valid.
                let reserved_len_i64: i64 = self.reserved_len as i64;

                let add_gib: i64 =
                    reserved_len_i64
                        .checked_add(EIGHT_GIB)
                        .ok_or(std::io::Error::new(
                            ErrorKind::OutOfMemory,
                            "Reserved length + 1 TiB overflows usize: {:#?}",
                        ))?;
                let add_gib_usize: usize = add_gib.try_into().map_err(|e| {
                    std::io::Error::new(
                        ErrorKind::OutOfMemory,
                        format!("Reserved length + 1 TiB overflows usize: {:#?}", e),
                    )
                })?;

                ftruncate(file.as_fd(), add_gib)?;
                self.reserved_len = add_gib_usize;
                Ok(())
            }
            Ok(false) => {
                self.sparse_file = false;
                Ok(())
            }
            // May retry this test later, but the prior allocation
            // succeeded, so no need to err out.
            Err(_) => Ok(()),
        }
    }

    /// Extend the underlying file to fit `buf_len` new bytes.
    fn reserve_for(&mut self, buf_len: usize) -> std::io::Result<()> {
        // 8 KiB, matching capacity for `std::io::BufWriter` as of 1.8.0.
        const MIN_RESERVE_LEN: usize = 8 * 1024;

        let remaining_len = self.reserved_len - self.written_len;
        if buf_len > remaining_len {
            let extend_len = buf_len - remaining_len;

            // Use a minimum reallocation step size to avoid thrashing.
            let new_reservation = self.reserved_len + max(extend_len, MIN_RESERVE_LEN);

            let file = self.file()?;

            // Make one big sparse allocation, if the filesystem supports it.
            #[cfg(target_os = "linux")]
            {
                let sparse_alloc = self.sparse_file
                    && self.reserve_ftruncate(&file, new_reservation).is_ok()
                    && self.reserve_sparse(&file).is_ok();
                if !sparse_alloc {
                    file.set_len(new_reservation as u64)?;
                    self.reserved_len = new_reservation;
                }
            }

            #[cfg(not(target_os = "linux"))]
            {
                file.set_len(new_reservation as u64)?;
                self.reserved_len = new_reservation;
            }

            // Increase mapping size to file size.
            // Don't advise write, as that may produce an arbitrarily long
            // load (especially if a huge sparse file is allocated).
            self.remap()?;
        }
        Ok(())
    }

    /// Write buffer into the mmap.
    ///
    /// [`Self::reserve_for`] MUST be called first, otherwise this may write
    /// beyond the length of the file/mapping.
    fn write_data(&mut self, buf: &[u8]) {
        let new_len = self.written_len + buf.len();
        self.mmap[self.written_len..new_len].copy_from_slice(buf);
        self.written_len = new_len;
    }

    /// Shrink to the actually written size, discarding garbage at the end.
    fn truncate(&mut self) -> std::io::Result<()> {
        if self.written_len < self.reserved_len {
            self.file()?.set_len(self.written_len as u64).unwrap();

            self.reserved_len = self.written_len;
            self.remap()?;
        }

        Ok(())
    }

    fn get_map(mut self) -> Result<memmap2::MmapMut, (Self, std::io::Error)> {
        let exec = |this: &mut Self| {
            this.flush()?;

            let stub_mmap = unsafe { MmapOptions::new().map_mut(&this.file()?) }?;

            let map = std::mem::replace(&mut this.mmap, stub_mmap);

            Ok(map)
        };

        exec(&mut self).map_err(|e| (self, e))
    }

    /// Remap to the current reserved len.
    ///
    /// Uses a remap call on Linux, has to create a new map on other systems.
    fn remap(&mut self) -> std::io::Result<()> {
        #[cfg(target_os = "linux")]
        {
            unsafe {
                self.mmap
                    .remap(self.reserved_len, RemapOptions::new().may_move(true))
            }?;
        }

        #[cfg(not(target_os = "linux"))]
        {
            self.mmap = unsafe { MmapOptions::new().map_mut(&self.file()?) }?;
        }

        Ok(())
    }
}

impl Write for MmapWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let buf_len = buf.len();

        self.reserve_for(buf_len)?;
        self.write_data(buf);

        Ok(buf_len)
    }

    fn write_vectored(&mut self, bufs: &[std::io::IoSlice<'_>]) -> std::io::Result<usize> {
        let bufs_len = bufs.iter().map(|buf| buf.len()).sum();

        self.reserve_for(bufs_len)?;
        for buf in bufs {
            self.write_data(buf)
        }

        Ok(bufs_len)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.truncate()?;
        self.mmap.flush()
    }
}

impl Drop for MmapWriter {
    fn drop(&mut self) {
        self.truncate().unwrap_or_else(|x| {
            panic!(
                "{} {}\n Error: {:#?}",
                "Failed to truncate on drop, call `self.flush` first to avoid",
                "using this panicking drop implementation.",
                x
            )
        })
    }
}

impl AsRef<[u8]> for MmapWriter {
    fn as_ref(&self) -> &[u8] {
        &self.mmap[0..self.written_len]
    }
}

impl AsMut<[u8]> for MmapWriter {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.mmap[0..self.written_len]
    }
}

impl WriteDisk for Mmap {
    type WriteDisk = WriteMmapGuard;

    fn write_disk(&self) -> std::io::Result<Self::WriteDisk> {
        // Shuffle out the current mmap value to replace later.
        let mut held_mmap = self.mmap.lock().unwrap();
        let cur_mmap = std::mem::replace(&mut *held_mmap, SwitchingMmap::Invalid);

        match cur_mmap.write_handle(&self.path) {
            Ok((mmap, guard)) => {
                *held_mmap = mmap;
                Ok(guard)
            }
            Err((mmap, error)) => {
                *held_mmap = mmap;
                Err(error)
            }
        }
    }
}

#[cfg(test)]
#[cfg(not(miri))] // Miri does not support file-backed memory mappings
mod tests {
    use std::{
        env::temp_dir,
        fs::remove_file,
        io::{IoSlice, Read},
    };

    use uuid::Uuid;

    use super::*;

    const SEQUENCE: &[u8] = &[32, 74, 91, 3, 8, 90];

    #[test]
    fn read_empty() {
        let temp_file = temp_dir().join(Uuid::new_v4().to_string());
        let _ = remove_file(&temp_file);
        File::create(&temp_file).unwrap();

        let mut read_data = Vec::new();
        Mmap::new(temp_file.clone())
            .unwrap()
            .read_disk()
            .unwrap()
            .read_to_end(&mut read_data)
            .unwrap();
        assert_eq!(read_data, &[0; 0]);

        let _ = remove_file(temp_file);
    }

    #[test]
    fn read_extern_write() {
        let temp_file = temp_dir().join(Uuid::new_v4().to_string());
        let _ = remove_file(&temp_file);
        let mut file_mut = File::create(&temp_file).unwrap();
        file_mut.write_all(SEQUENCE).unwrap();
        file_mut.flush().unwrap();

        let mut read_data = Vec::new();
        Mmap::new(temp_file.clone())
            .unwrap()
            .read_disk()
            .unwrap()
            .read_to_end(&mut read_data)
            .unwrap();
        assert_eq!(read_data, SEQUENCE);

        let _ = remove_file(temp_file);
    }

    #[test]
    fn write_extern_read() {
        let temp_file = temp_dir().join(Uuid::new_v4().to_string());
        let _ = remove_file(&temp_file);

        assert_eq!(
            Mmap::new(temp_file.clone())
                .unwrap()
                .write_disk()
                .unwrap()
                .write(SEQUENCE)
                .unwrap(),
            SEQUENCE.len()
        );

        let mut read_data = Vec::new();
        let mut file = File::open(&temp_file).unwrap();
        file.read_to_end(&mut read_data).unwrap();
        assert_eq!(read_data, SEQUENCE);

        let _ = remove_file(temp_file);
    }

    #[test]
    fn write_and_read_interleave() {
        let temp_file = temp_dir().join(Uuid::new_v4().to_string());
        let _ = remove_file(&temp_file);

        let mmap = Mmap::new(temp_file.clone()).unwrap();

        let mut write_disk = mmap.write_disk().unwrap();
        assert_eq!(
            write_disk
                .write_vectored(&[IoSlice::new(SEQUENCE); 8])
                .unwrap(),
            SEQUENCE.len() * 8
        );
        write_disk.flush().unwrap();

        // Should see all eight copies, no more and no less.
        assert_eq!(write_disk.as_ref(), ([SEQUENCE; 8].concat()));

        assert_eq!(write_disk.write(SEQUENCE).unwrap(), SEQUENCE.len());
        write_disk.flush().unwrap();

        drop(write_disk);

        // Should see an extra copy.
        let mut read_data = Vec::new();
        mmap.read_disk()
            .unwrap()
            .read_to_end(&mut read_data)
            .unwrap();
        assert_eq!(read_data.len(), SEQUENCE.len() * 9);
        assert_eq!(read_data, [SEQUENCE; 9].concat());

        let _ = remove_file(temp_file);
    }

    #[test]
    fn write_flush_truncation() {
        const GARBAGE_LEN: usize = SEQUENCE.len() * 10;

        let temp_file = temp_dir().join(Uuid::new_v4().to_string());
        let _ = remove_file(&temp_file);
        let mut file_mut = File::create(&temp_file).unwrap();
        file_mut.set_len(GARBAGE_LEN as u64).unwrap();
        file_mut.flush().unwrap();

        let mmap = Mmap::new(temp_file.clone()).unwrap();

        let read_check = |expected| {
            let mut read_data = Vec::new();
            mmap.read_disk()
                .unwrap()
                .read_to_end(&mut read_data)
                .unwrap();
            assert_eq!(read_data.len(), expected);
        };

        let mut write_disk = mmap.write_disk().unwrap();
        assert_eq!(write_disk.write(SEQUENCE).unwrap(), SEQUENCE.len());

        // Should see the extra garbage prior to flush.
        {
            let mut read_data = Vec::new();
            File::open(&temp_file)
                .unwrap()
                .read_to_end(&mut read_data)
                .unwrap();
            assert_eq!(read_data.len(), GARBAGE_LEN);
        }

        drop(write_disk);

        // Should see an only the actual data after flush.
        read_check(SEQUENCE.len());

        let _ = remove_file(temp_file);
    }
}
