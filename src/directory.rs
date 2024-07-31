use std::{
    fs::{create_dir_all, remove_file, File},
    io::{Read, Seek, Write},
    ops::{Deref, DerefMut},
    path::PathBuf,
};

use bincode::serialize_into;
use serde::{
    de::{Error, Visitor},
    Deserialize, Serialize,
};
use uuid::Uuid;

use crate::array::sync_impl::BackedArray;

/// File, but serializes based on path string
#[derive(Debug)]
pub struct SerialFile {
    pub file: File,
    pub path: PathBuf,
}

impl SerialFile {
    pub fn new(path: PathBuf) -> Result<Self, std::io::Error> {
        Ok(Self {
            file: File::options()
                .read(true)
                .write(true)
                .create(true)
                .open(path.clone())?,
            path,
        })
    }
}

impl Deref for SerialFile {
    type Target = File;
    fn deref(&self) -> &Self::Target {
        &self.file
    }
}

impl DerefMut for SerialFile {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.file
    }
}

impl Write for SerialFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.file.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
    fn write_vectored(&mut self, bufs: &[std::io::IoSlice<'_>]) -> std::io::Result<usize> {
        self.file.write_vectored(bufs)
    }
}

impl Read for SerialFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.file.read(buf)
    }
    fn read_vectored(&mut self, bufs: &mut [std::io::IoSliceMut<'_>]) -> std::io::Result<usize> {
        self.file.read_vectored(bufs)
    }
}

impl Seek for SerialFile {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.file.seek(pos)
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

pub struct PathBufVisitor;

impl<'de> Visitor<'de> for PathBufVisitor {
    type Value = PathBuf;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("A valid path to a file")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(PathBuf::from(v))
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
        Ok(Self { file, path })
    }
}

/// [`BackedArray`] that uses a directory of plain files
#[derive(Debug, Serialize, Deserialize)]
pub struct DirectoryBackedArray<T> {
    array: BackedArray<T, SerialFile>,
    directory_root: PathBuf,
}

impl<T> DirectoryBackedArray<T> {
    pub fn new(directory_root: PathBuf) -> std::io::Result<Self> {
        create_dir_all(directory_root.clone())?;
        Ok(DirectoryBackedArray {
            array: BackedArray::default(),
            directory_root,
        })
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

impl<T: Serialize> DirectoryBackedArray<T> {
    /// Wraps [`BackedArray::save_to_disk`] to include its own metadata
    pub fn save_to_disk<W: Write>(&mut self, writer: W) -> bincode::Result<()> {
        self.array.clear_memory();
        serialize_into(writer, self)
    }
}

impl<T> DirectoryBackedArray<T> {
    /// Updates the root of the directory backed array.
    ///
    /// Does not move any files or directories, just changes pointers.
    pub fn update_root(&mut self, new_root: PathBuf) -> &Self {
        self.array.get_disks_mut().iter_mut().for_each(|disk| {
            disk.path = new_root.join(disk.path.file_name().unwrap());
        });
        self.directory_root = new_root;
        self
    }

    /// Wraps [`BackedArray::remove`] to delete the file
    pub fn remove(&mut self, entry_idx: usize) -> Result<&Self, std::io::Error> {
        remove_file(self.get_disks()[entry_idx].path.clone())?;
        self.array.remove(entry_idx);
        Ok(self)
    }
}

impl<T: Serialize> DirectoryBackedArray<T> {
    fn next_target(&self) -> std::io::Result<SerialFile> {
        SerialFile::new(self.directory_root.join(Uuid::new_v4().to_string()))
    }

    /// Wraps [`BackedArray::append`]
    pub fn append(&mut self, values: &[T]) -> bincode::Result<&Self> {
        let next_target = self.next_target().map_err(bincode::Error::custom)?;
        self.array.append(values, next_target)?;
        Ok(self)
    }

    /// Wraps [`BackedArray::append_memory`]
    pub fn append_memory(&mut self, values: Box<[T]>) -> bincode::Result<&Self> {
        let next_target = self.next_target().map_err(bincode::Error::custom)?;
        self.array.append_memory(values, next_target)?;
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use std::{env::temp_dir, fs::remove_dir_all};

    use bincode::deserialize_from;
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
            deserialize_from(File::open(directory.join("directory")).unwrap()).unwrap();
        assert_eq!(arr.get(100).unwrap(), &"TEST STRING");
        assert_eq!(arr.get(15_000).unwrap(), &"OTHER VALUE");
        assert_eq!(arr.get(200).unwrap(), &"TEST STRING");
        assert_eq!(arr.get(1).unwrap(), &"TEST STRING");

        remove_dir_all(directory).unwrap();
    }
}
