use std::{
    fs::{copy, create_dir_all, remove_dir_all, remove_file, rename, File},
    io::{BufReader, Read, Seek, Write},
    ops::{Deref, DerefMut},
    path::PathBuf,
};

use derive_getters::Getters;
use serde::{
    de::{DeserializeOwned, Error},
    Deserialize, Serialize,
};
use uuid::Uuid;
use zstd::{Decoder, Encoder};

use crate::{array::sync_impl::BackedArray, meta::sync_impl::BackedArrayWrapper};

#[cfg(feature = "zstdmt")]
use super::ZSTD_MULTITHREAD;

/// File encoded with zstd
pub struct ZstdFile<'a> {
    file: File,
    path: PathBuf,
    #[allow(dead_code)]
    decoder: Decoder<'a, BufReader<File>>,
    #[allow(dead_code)]
    encoder: Encoder<'a, File>,
    zstd_level: i32,
}

#[derive(Serialize, Deserialize)]
struct ZstdFileSerialized {
    path: String,
    zstd_level: i32,
}

impl ZstdFile<'_> {
    /// Create a new ZstdFile
    ///
    /// * `path`: A valid filesystem path
    /// * `zstd_level`: An optional level bound [0-22]. 0 for library default.
    pub fn new(path: PathBuf, zstd_level: Option<i32>) -> std::io::Result<Self> {
        let zstd_level = zstd_level.unwrap_or(0);
        if !(0..=22).contains(&zstd_level) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("zstd_level ({zstd_level}) not [0, 22]"),
            ));
        };
        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path.clone())?;

        #[allow(unused_mut)]
        let mut encoder = Encoder::new(file.try_clone()?, zstd_level)?;
        #[cfg(feature = "zstdmt")]
        encoder.multithread(*ZSTD_MULTITHREAD.lock().unwrap())?;

        Ok(Self {
            file: file.try_clone()?,
            path,
            decoder: Decoder::new(file)?,
            encoder,
            zstd_level,
        })
    }
}

impl Serialize for ZstdFile<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let serial_form = ZstdFileSerialized {
            path: self.path.to_str().unwrap().to_string(),
            zstd_level: self.zstd_level,
        };
        serial_form.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ZstdFile<'_> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let ZstdFileSerialized { path, zstd_level } =
            ZstdFileSerialized::deserialize(deserializer)?;
        let file = File::options()
            .read(true)
            .write(true)
            .open(path.clone())
            .map_err(|err| D::Error::custom(format!("{:#?}", err)))?;

        #[allow(unused_mut)]
        let mut encoder = Encoder::new(file.try_clone().map_err(D::Error::custom)?, zstd_level)
            .map_err(D::Error::custom)?;
        #[cfg(feature = "zstdmt")]
        encoder
            .multithread(*ZSTD_MULTITHREAD.lock().unwrap())
            .map_err(D::Error::custom)?;

        Ok(Self {
            file: file.try_clone().map_err(D::Error::custom)?,
            path: path.into(),
            decoder: Decoder::new(file.try_clone().map_err(D::Error::custom)?)
                .map_err(D::Error::custom)?,
            encoder,
            zstd_level,
        })
    }
}

impl Read for ZstdFile<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.decoder.read(buf)
    }
    fn read_vectored(&mut self, bufs: &mut [std::io::IoSliceMut<'_>]) -> std::io::Result<usize> {
        self.decoder.read_vectored(bufs)
    }
}

impl Write for ZstdFile<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.encoder.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        // Use the flush call as an opportunity to call finish on encoding
        // Without calling finish, the metadata won't be complete
        self.encoder.flush()?;
        self.encoder.do_finish()?;
        self.encoder = Encoder::new(self.file.try_clone()?, self.zstd_level)?;
        #[cfg(feature = "zstdmt")]
        self.encoder
            .multithread(*ZSTD_MULTITHREAD.lock().unwrap())?;
        Ok(())
    }
    fn write_vectored(&mut self, bufs: &[std::io::IoSlice<'_>]) -> std::io::Result<usize> {
        self.encoder.write_vectored(bufs)
    }
}

impl Seek for ZstdFile<'_> {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.decoder.get_mut().seek(pos)
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
                    .join(Uuid::new_v4().to_string() + ".zstd"),
                self.zstd_level,
            )?,
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
                self.zstd_level,
            )?,
        )?;
        Ok(self)
    }

    fn append_array(&mut self, rhs: Self) -> Result<&mut Self, Self::BackingError> {
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
