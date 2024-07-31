use std::{
    fs::{copy, create_dir_all, remove_dir_all, remove_file, rename, File},
    io::{BufReader, Read, Seek},
    marker::PhantomData,
    ops::{Deref, DerefMut},
    path::PathBuf,
};

use derive_getters::Getters;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use uuid::Uuid;
use zstd::{Decoder, Encoder};

use crate::{
    array::sync_impl::BackedArray,
    entry::sync_impl::{ReadDisk, WriteDisk},
    meta::sync_impl::BackedArrayWrapper,
};

#[cfg(feature = "zstdmt")]
use super::ZSTD_MULTITHREAD;

/// File encoded with zstd
#[derive(Debug, Serialize, Deserialize)]
pub struct ZstdFile<'a> {
    path: PathBuf,
    zstd_level: i32,
    phantom_lifetime: PhantomData<&'a ()>,
}

impl ZstdFile<'_> {
    pub fn new(path: PathBuf, zstd_level: i32) -> Self {
        Self {
            path,
            zstd_level,
            phantom_lifetime: PhantomData,
        }
    }
}

impl<'a> ReadDisk for ZstdFile<'a> {
    type ReadDisk = ZstdDecoderWrapper<'a>;

    fn read_disk(&mut self) -> std::io::Result<Self::ReadDisk> {
        Ok(ZstdDecoderWrapper(Decoder::new(File::open(
            self.path.clone(),
        )?)?))
    }
}

impl<'a> WriteDisk for ZstdFile<'a> {
    type WriteDisk = Encoder<'a, File>;

    fn write_disk(&mut self) -> std::io::Result<Self::WriteDisk> {
        #[allow(unused_mut)]
        let mut encoder = Encoder::new(
            File::options()
                .write(true)
                .create(true)
                .truncate(true)
                .open(self.path.clone())?,
            self.zstd_level,
        )?;

        #[cfg(feature = "zstdmt")]
        encoder
            .multithread(*ZSTD_MULTITHREAD.lock().unwrap())
            .unwrap();

        Ok(encoder)
    }
}

pub struct ZstdDecoderWrapper<'a>(Decoder<'a, BufReader<File>>);

impl Read for ZstdDecoderWrapper<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }
    fn read_vectored(&mut self, bufs: &mut [std::io::IoSliceMut<'_>]) -> std::io::Result<usize> {
        self.0.read_vectored(bufs)
    }
}

impl Seek for ZstdDecoderWrapper<'_> {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.0.get_mut().seek(pos)
    }
}

/// [`BackedArray`] that uses a directory of zstd compressed files
#[derive(Serialize, Deserialize, Getters)]
pub struct ZstdDirBackedArray<'a, T> {
    array: BackedArray<T, ZstdFile<'a>>,
    directory_root: PathBuf,
    zstd_level: Option<i32>,
}

impl<T> ZstdDirBackedArray<'_, T> {
    /// Creates a new array backed by zstd compressed files in a directory
    ///
    /// * `directory_root`: base directory for every file
    /// * `zstd_level`: An optional level bound [0-22]. 0 for library default.
    pub fn new(directory_root: PathBuf, zstd_level: Option<i32>) -> std::io::Result<Self> {
        create_dir_all(directory_root.clone())?;
        Ok(ZstdDirBackedArray {
            array: BackedArray::default(),
            directory_root,
            zstd_level,
        })
    }

    /// Sets a new zstd_level for all future arrays
    ///
    /// Does not impact already-compressed arrays
    pub fn set_level(&mut self, zstd_level: i32) {
        self.zstd_level = Some(zstd_level);
    }
}

impl<'a, T: Serialize + DeserializeOwned> BackedArrayWrapper<T> for ZstdDirBackedArray<'a, T> {
    type Storage = ZstdFile<'a>;
    type BackingError = std::io::Error;

    fn remove(&mut self, entry_idx: usize) -> Result<&mut Self, std::io::Error> {
        remove_file(self.get_disks()[entry_idx].path.clone())?;
        self.array.remove(entry_idx);
        Ok(self)
    }

    fn append(&mut self, values: &[T]) -> bincode::Result<&mut Self> {
        self.array.append(
            values,
            ZstdFile::new(
                self.directory_root
                    .clone()
                    .join(Uuid::new_v4().to_string() + ".zst"),
                self.zstd_level.unwrap_or(0),
            ),
        )?;
        Ok(self)
    }

    fn append_memory(&mut self, values: Box<[T]>) -> bincode::Result<&mut Self> {
        self.array.append_memory(
            values,
            ZstdFile::new(
                self.directory_root
                    .clone()
                    .join(Uuid::new_v4().to_string() + ".zstd"),
                self.zstd_level.unwrap_or(0),
            ),
        )?;
        Ok(self)
    }

    fn append_array(&mut self, rhs: Self) -> Result<&mut Self, Self::BackingError> {
        if self.directory_root != rhs.directory_root {
            rhs.array
                .get_disks()
                .iter()
                .map(|disk| {
                    copy(
                        disk.path.clone(),
                        self.directory_root.join(disk.path.file_name().unwrap()),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            remove_dir_all(rhs.directory_root)?;
        }
        self.array.append_array(rhs.array);
        Ok(self)
    }

    fn delete(self) -> Result<(), Self::BackingError> {
        remove_dir_all(self.directory_root)
    }
}

impl<'a, T> Deref for ZstdDirBackedArray<'a, T> {
    type Target = BackedArray<T, ZstdFile<'a>>;

    fn deref(&self) -> &Self::Target {
        &self.array
    }
}

impl<T> DerefMut for ZstdDirBackedArray<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.array
    }
}

impl<T> ZstdDirBackedArray<'_, T> {
    /// Updates the root of the zstd directory backed array.
    ///
    /// Does not move any files or directories, just changes pointers.
    pub fn update_root(&mut self, new_root: PathBuf) -> &mut Self {
        self.array.get_disks_mut().iter_mut().for_each(|disk| {
            disk.path = new_root.join(disk.path.file_name().unwrap());
        });
        self.directory_root = new_root;
        self
    }

    /// Moves the directory to a new location wholesale.
    pub fn move_root(&mut self, new_root: PathBuf) -> std::io::Result<&mut Self> {
        if rename(self.directory_root.clone(), new_root.clone()).is_err() {
            create_dir_all(new_root.clone())?;
            self.array
                .get_disks()
                .iter()
                .map(|disk| {
                    copy(
                        disk.path.clone(),
                        new_root.join(disk.path.file_name().unwrap()),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
        }
        Ok(self.update_root(new_root))
    }
}

#[cfg(test)]
mod tests {
    use std::{env::temp_dir, fs::remove_dir_all};

    use itertools::Itertools;

    use super::*;

    fn values() -> (Vec<String>, Vec<String>) {
        (
            ["TEST STRING".to_string()]
                .into_iter()
                .cycle()
                .take(10_000)
                .collect_vec(),
            ["OTHER VALUE".to_string()]
                .into_iter()
                .cycle()
                .take(10_000)
                .collect_vec(),
        )
    }

    #[test]
    fn write() {
        let directory = temp_dir().join("zstd_directory_write");
        let _ = remove_dir_all(directory.clone());
        let mut arr = ZstdDirBackedArray::new(directory.clone(), None).unwrap();
        let (values, second_values) = values();

        arr.append_memory(values.into()).unwrap();
        arr.append(&second_values).unwrap();
        assert_eq!(arr.get(100).unwrap(), &"TEST STRING");
        assert_eq!(arr.get(200).unwrap(), &"TEST STRING");
        assert_eq!(arr.get(150).unwrap(), &"TEST STRING");
        assert_eq!(arr.get(15_000).unwrap(), &"OTHER VALUE");

        remove_dir_all(directory).unwrap();
    }

    #[test]
    fn write_and_read() {
        let directory = temp_dir().join("zstd_directory_write_and_read");
        let _ = remove_dir_all(directory.clone());
        let mut arr = ZstdDirBackedArray::new(directory.clone(), None).unwrap();
        let (values, second_values) = values();

        arr.append(&values).unwrap();
        arr.append_memory(second_values.into()).unwrap();
        arr.save_to_disk(File::create(directory.join("directory")).unwrap())
            .unwrap();
        drop(arr);

        let mut arr: ZstdDirBackedArray<String> =
            ZstdDirBackedArray::load(File::open(directory.join("directory")).unwrap()).unwrap();
        assert_eq!(arr.get(100).unwrap(), &"TEST STRING");
        assert_eq!(arr.get(15_000).unwrap(), &"OTHER VALUE");
        assert_eq!(arr.get(200).unwrap(), &"TEST STRING");
        assert_eq!(arr.get(1).unwrap(), &"TEST STRING");

        remove_dir_all(directory).unwrap();
    }
}
