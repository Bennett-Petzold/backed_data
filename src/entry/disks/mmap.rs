use std::{
    cmp::max,
    fs::File,
    io::{Cursor, ErrorKind, Write},
    mem::transmute,
    ops::Deref,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard, TryLockError},
};

use memmap2::{Advice, MmapOptions};
use serde::{Deserialize, Serialize};

#[cfg(target_os = "linux")]
use memmap2::RemapOptions;

use crate::utils::BorrowExtender;

use super::{ReadDisk, WriteDisk};

// ---------- Internal types ---------- //

#[derive(Debug)]
pub enum SwitchingMmap {
    ReadMmap(memmap2::Mmap),
    WriteMmap(MmapWriter),
}

fn open_mmap<P: AsRef<Path>>(path: P) -> std::io::Result<File> {
    File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path.as_ref())
}

impl SwitchingMmap {
    fn init_read<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let mmap = unsafe { MmapOptions::new().map(&open_mmap(path)?) }?;
        #[cfg(target_os = "linux")]
        let _ = mmap.advise(Advice::PopulateRead);

        Ok(Self::ReadMmap(mmap))
    }

    fn conv_to_read(self) -> std::io::Result<Self> {
        match self {
            Self::ReadMmap(x) => Ok(Self::ReadMmap(x)),
            Self::WriteMmap(mut x) => {
                let mmap = x.get_map()?.make_read_only()?;
                #[cfg(target_os = "linux")]
                let _ = mmap.advise(Advice::PopulateRead);

                Ok(Self::ReadMmap(mmap))
            }
        }
    }

    fn conv_to_write<P: AsRef<Path>>(self, path: P) -> std::io::Result<Self> {
        match self {
            Self::ReadMmap(x) => Ok(Self::WriteMmap(MmapWriter::from_args(path, x.make_mut()?)?)),
            Self::WriteMmap(x) => Ok(Self::WriteMmap(x)),
        }
    }
}

impl AsRef<[u8]> for SwitchingMmap {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::ReadMmap(x) => x.as_ref(),
            Self::WriteMmap(x) => x.as_ref(),
        }
    }
}

impl Write for SwitchingMmap {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::ReadMmap(_) => Err(std::io::Error::new(
                ErrorKind::Other,
                "Cannot write to ReadMmap",
            )),
            Self::WriteMmap(x) => x.write(buf),
        }
    }

    fn write_vectored(&mut self, bufs: &[std::io::IoSlice<'_>]) -> std::io::Result<usize> {
        match self {
            Self::ReadMmap(_) => Err(std::io::Error::new(
                ErrorKind::Other,
                "Cannot write to ReadMmap",
            )),
            Self::WriteMmap(x) => x.write_vectored(bufs),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::ReadMmap(_) => Ok(()),
            Self::WriteMmap(x) => x.flush(),
        }
    }
}

#[derive(Debug)]
pub struct ReadMmapGuard(BorrowExtender<Arc<GuardedMmap>, RwLockReadGuard<'static, SwitchingMmap>>);

impl AsRef<[u8]> for ReadMmapGuard {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl Deref for ReadMmapGuard {
    type Target = SwitchingMmap;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug)]
pub struct WriteMmapGuard(
    BorrowExtender<Arc<GuardedMmap>, RwLockWriteGuard<'static, SwitchingMmap>>,
);

impl AsRef<[u8]> for WriteMmapGuard {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl Deref for WriteMmapGuard {
    type Target = SwitchingMmap;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Write for WriteMmapGuard {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }
    fn write_vectored(&mut self, bufs: &[std::io::IoSlice<'_>]) -> std::io::Result<usize> {
        self.0.write_vectored(bufs)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

/// Guards an inner mmap to match Rust mutability requirements.
///
/// Allows >= 1 outstanding mmap instances for immutable access via read locks,
/// 1 outstanding mmap instance for mutable access via a write lock, or
/// no outstanding accesses.
///
/// `access_lock` is claimed for the full duration of methods getting a read
/// or write handle, so [`Self::get_read`] can transition from a read to write
/// access without another thread potentially claiming a lock inbetween.
#[derive(Debug)]
struct GuardedMmap {
    inner: RwLock<SwitchingMmap>,
    access_lock: Mutex<()>,
}

impl GuardedMmap {
    fn new<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        Ok(Self {
            inner: RwLock::new(SwitchingMmap::init_read(path)?),
            access_lock: Mutex::new(()),
        })
    }

    /// Fails when an outstanding write is held, or on lock panic.
    fn get_read<P: AsRef<Path>>(this: Arc<Self>, path: P) -> std::io::Result<ReadMmapGuard> {
        // Hold exclusive access for getting locks `self.inner` during the
        // execution of this function.
        let _access = this
            .access_lock
            .lock()
            .map_err(|e| std::io::Error::new(ErrorKind::Other, format!("{:#?}", e)))?;

        let gen_read_lock = |this: &Arc<Self>| {
            BorrowExtender::try_new(this.clone(), |this| {
                let this = this.as_ref();
                let this = unsafe { transmute::<&GuardedMmap, &'static GuardedMmap>(this) };
                this.inner.try_read().map_err(|e| match e {
                    TryLockError::WouldBlock => {
                        std::io::Error::new(ErrorKind::WouldBlock, "Read lock is unavailable.")
                    }
                    x => std::io::Error::new(ErrorKind::Other, format!("{:#?}", x)),
                })
            })
        };

        let read_lock = gen_read_lock(&this)?;

        let is_write = matches!(**read_lock, SwitchingMmap::WriteMmap(_));

        if is_write {
            drop(read_lock);
            let write_lock = this.inner.write();

            match write_lock {
                Ok(mut mmap) => {
                    let temp_mmap = std::mem::replace(
                        &mut *mmap,
                        SwitchingMmap::ReadMmap(unsafe {
                            MmapOptions::new().map(&open_mmap(&path)?)?
                        }),
                    );
                    *mmap = temp_mmap.conv_to_read()?;

                    drop(mmap);
                    Ok(ReadMmapGuard(gen_read_lock(&this)?))
                }
                Err(e) => Err(std::io::Error::new(ErrorKind::Other, format!("{:#?}", e))),
            }
        } else {
            Ok(ReadMmapGuard(read_lock))
        }
    }

    /// Fails when an outstanding write is held, or on lock panic.
    fn get_write<P: AsRef<Path>>(this: Arc<Self>, path: P) -> std::io::Result<WriteMmapGuard> {
        // Hold exclusive access for getting locks `self.inner` during the
        // execution of this function.
        let _access = this
            .access_lock
            .lock()
            .map_err(|e| std::io::Error::new(ErrorKind::Other, format!("{:#?}", e)))?;

        let mut write_lock = BorrowExtender::try_new(this.clone(), |this| {
            let this = this.as_ref();
            let this = unsafe { transmute::<&GuardedMmap, &'static GuardedMmap>(this) };
            this.inner.try_write().map_err(|e| match e {
                TryLockError::WouldBlock => {
                    std::io::Error::new(ErrorKind::WouldBlock, "Write lock is unavailable.")
                }
                x => std::io::Error::new(ErrorKind::Other, format!("{:#?}", x)),
            })
        })?;

        let is_read = matches!(**write_lock, SwitchingMmap::ReadMmap(_));

        if is_read {
            let temp_mmap = std::mem::replace(
                &mut **write_lock,
                SwitchingMmap::WriteMmap(MmapWriter::no_advise(&path)?),
            );
            **write_lock = temp_mmap.conv_to_write(path)?;
        }

        Ok(WriteMmapGuard(write_lock))
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
    mmap: Arc<GuardedMmap>,
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
        let mmap = GuardedMmap::new(&path)
            .map_err(serde::de::Error::custom)?
            .into();

        Ok(Mmap { path, mmap })
    }
}

// ---------- END Serde impls ---------- //

impl From<PathBuf> for Mmap {
    fn from(value: PathBuf) -> Self {
        let mmap = GuardedMmap::new(&value)
            .unwrap_or_else(|_| panic!("Path could not be opened through mmap: {:#?}", &value))
            .into();
        Self { path: value, mmap }
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
    pub fn new(path: PathBuf) -> Self {
        path.into()
    }
}

impl ReadDisk for Mmap {
    type ReadDisk = Cursor<ReadMmapGuard>;

    fn read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        let mmap = GuardedMmap::get_read(self.mmap.clone(), &self.path)?;

        Ok(Cursor::new(mmap))
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
        let _ = mmap.advise(Advice::Sequential);
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
        let file = open_mmap(&path)?;

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
        open_mmap(&self.path)
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

            // Increase mapping size to file size and re-advise.
            // Don't advise write, as that may produce an arbitrarily long
            // load (especially if a huge sparse file is allocated).
            self.remap()?;
            let _ = self.mmap.advise(Advice::Sequential);
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
        // Shrink to the actually written size, discarding garbage at the end.
        if self.written_len < self.reserved_len {
            self.file()?.set_len(self.written_len as u64)?;

            self.reserved_len = self.written_len;
            self.remap()?;
        }

        Ok(())
    }

    fn get_map(&mut self) -> std::io::Result<memmap2::MmapMut> {
        self.flush()?;

        let file = self.file()?;
        Ok(std::mem::replace(&mut self.mmap, unsafe {
            MmapOptions::new().map_mut(&file)
        }?))
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
        if self.written_len > 0 {
            // Skip if this drop is for an temporary
            // instance that existed for memory
            // consistency. Otherwise the truncation can
            // trigger SIGBUS on later writes.
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
        GuardedMmap::get_write(self.mmap.clone(), &self.path)
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

        let mmap = Mmap::new(temp_file.clone());

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

        let mmap = Mmap::new(temp_file.clone());

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
