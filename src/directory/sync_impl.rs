use std::{
    fs::{copy, create_dir_all, remove_dir_all, remove_file, rename, File},
    io::{BufReader, BufWriter, Read, Seek, Write},
    ops::{Deref, DerefMut},
    path::PathBuf,
};

use serde::{
    de::{DeserializeOwned, Error},
    Deserialize, Serialize,
};
use uuid::Uuid;

use crate::{array::sync_impl::BackedArray, meta::sync_impl::BackedArrayWrapper};

use super::PathBufVisitor;

/// File, but serializes based on path string
#[derive(Debug)]
pub struct SerialFile {
    read_file: BufReader<File>,
    write_file: BufWriter<File>,
    path: PathBuf,
}

impl SerialFile {
    pub fn new(path: PathBuf) -> Result<Self, std::io::Error> {
        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path.clone())?;
        Ok(Self {
            read_file: BufReader::new(file.try_clone()?),
            write_file: BufWriter::new(file),
            path,
        })
    }
}

impl Write for SerialFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.write_file.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.write_file.flush()
    }
    fn write_vectored(&mut self, bufs: &[std::io::IoSlice<'_>]) -> std::io::Result<usize> {
        self.write_file.write_vectored(bufs)
    }
    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        self.write_file.write_all(buf)
    }
}

impl Read for SerialFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.read_file.read(buf)
    }
    fn read_vectored(&mut self, bufs: &mut [std::io::IoSliceMut<'_>]) -> std::io::Result<usize> {
        self.read_file.read_vectored(bufs)
    }
}

impl Seek for SerialFile {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.read_file.seek(pos)
    }
}

impl Serialize for SerialFile {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.path.to_str().unwrap())
    }
}

impl<'de> Deserialize<'de> for SerialFile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let path = deserializer.deserialize_str(PathBufVisitor)?;
        let file = File::options()
            .read(true)
            .write(true)
            .open(path.clone())
            .map_err(|err| D::Error::custom(format!("{:#?}", err)))?;
        Ok(Self {
            read_file: BufReader::new(
                file.try_clone()
                    .map_err(|err| D::Error::custom(format!("{:#?}", err)))?,
            ),
            write_file: BufWriter::new(file),
            path,
        })
    }
}

/// [`BackedArray`] that uses a directory of plain files
#[derive(Debug, Serialize, Deserialize)]
pub struct DirectoryBackedArray<T> {
    array: BackedArray<T, SerialFile>,
    directory_root: PathBuf,
}

impl<T: Serialize + DeserializeOwned> BackedArrayWrapper<T> for DirectoryBackedArray<T> {
    type Storage = SerialFile;
    type BackingError = std::io::Error;

    fn remove(&mut self, entry_idx: usize) -> Result<&mut Self, std::io::Error> {
        remove_file(self.get_disks()[entry_idx].path.clone())?;
        self.array.remove(entry_idx);
        Ok(self)
    }

    fn append(&mut self, values: &[T]) -> bincode::Result<&mut Self> {
        let next_target = self.next_target().map_err(bincode::Error::custom)?;
        self.array.append(values, next_target)?;
        Ok(self)
    }

    fn append_memory(&mut self, values: Box<[T]>) -> bincode::Result<&mut Self> {
        let next_target = self.next_target().map_err(bincode::Error::custom)?;
        self.array.append_memory(values, next_target)?;
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

impl<T> Deref for DirectoryBackedArray<T> {
    type Target = BackedArray<T, SerialFile>;

    fn deref(&self) -> &Self::Target {
        &self.array
    }
}

impl<T> DerefMut for DirectoryBackedArray<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.array
    }
}

impl<T> DirectoryBackedArray<T> {
    /// Creates a new directory at `directory_root`.
    ///
    /// * `directory_root`: Valid read/write directory on system
    pub fn new(directory_root: PathBuf) -> std::io::Result<Self> {
        create_dir_all(directory_root.clone())?;
        Ok(DirectoryBackedArray {
            array: BackedArray::default(),
            directory_root,
        })
    }

    /// Updates the root of the directory backed array.
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

impl<T: Serialize> DirectoryBackedArray<T> {
    fn next_target(&self) -> std::io::Result<SerialFile> {
        SerialFile::new(self.directory_root.join(Uuid::new_v4().to_string()))
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
        let directory = temp_dir().join("directory_write");
        let _ = remove_dir_all(directory.clone());
        let mut arr = DirectoryBackedArray::new(directory.clone()).unwrap();
        let (values, second_values) = values();

        arr.append_memory(values.into()).unwrap();
        arr.append(&second_values).unwrap();
        assert_eq!(arr.get(100).unwrap(), &"TEST STRING");
        assert_eq!(arr.get(15_000).unwrap(), &"OTHER VALUE");

        remove_dir_all(directory).unwrap();
    }

    #[test]
    fn write_and_read() {
        let directory = temp_dir().join("directory_write_and_read");
        let _ = remove_dir_all(directory.clone());
        let mut arr = DirectoryBackedArray::new(directory.clone()).unwrap();
        let (values, second_values) = values();

        arr.append(&values).unwrap();
        arr.append_memory(second_values.into()).unwrap();
        arr.save_to_disk(File::create(directory.join("directory")).unwrap())
            .unwrap();
        drop(arr);

        let mut arr: DirectoryBackedArray<String> =
            DirectoryBackedArray::load(File::open(directory.join("directory")).unwrap()).unwrap();
        assert_eq!(arr.get(100).unwrap(), &"TEST STRING");
        assert_eq!(arr.get(15_000).unwrap(), &"OTHER VALUE");
        assert_eq!(arr.get(200).unwrap(), &"TEST STRING");
        assert_eq!(arr.get(1).unwrap(), &"TEST STRING");

        remove_dir_all(directory).unwrap();
    }
}
