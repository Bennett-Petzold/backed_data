use std::{
    fs::{copy, create_dir_all, remove_dir_all, remove_file, rename},
    ops::{Deref, DerefMut},
    path::PathBuf,
};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use uuid::Uuid;

use crate::{array::sync_impl::BackedArray, meta::sync_impl::BackedArrayWrapper};

/// [`BackedArray`] that uses a directory of plain files
#[derive(Debug, Serialize, Deserialize)]
pub struct DirectoryBackedArray<T> {
    array: BackedArray<T, PathBuf>,
    directory_root: PathBuf,
}

impl<T: Serialize + DeserializeOwned> BackedArrayWrapper<T> for DirectoryBackedArray<T> {
    type Storage = PathBuf;
    type BackingError = std::io::Error;

    fn remove(&mut self, entry_idx: usize) -> Result<&mut Self, std::io::Error> {
        remove_file(self.get_disks()[entry_idx].clone())?;
        self.array.remove(entry_idx);
        Ok(self)
    }

    fn append(&mut self, values: &[T]) -> bincode::Result<&mut Self> {
        let next_target = self.next_target();
        self.array.append(values, next_target)?;
        Ok(self)
    }

    fn append_memory(&mut self, values: Box<[T]>) -> bincode::Result<&mut Self> {
        let next_target = self.next_target();
        self.array.append_memory(values, next_target)?;
        Ok(self)
    }

    fn append_array(&mut self, rhs: Self) -> Result<&mut Self, Self::BackingError> {
        rhs.array
            .get_disks()
            .iter()
            .map(|disk| copy(disk, self.directory_root.join(disk.file_name().unwrap())))
            .collect::<Result<Vec<_>, _>>()?;
        if self.directory_root != rhs.directory_root {
            remove_dir_all(rhs.directory_root)?;
        }
        self.array.append_array(rhs.array);
        Ok(self)
    }

    fn delete(self) -> Result<(), Self::BackingError> {
        remove_dir_all(self.directory_root)
    }
}

impl<T> Deref for DirectoryBackedArray<T> {
    type Target = BackedArray<T, PathBuf>;

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
            **disk = new_root.join(disk.file_name().unwrap());
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
                .map(|disk| copy(disk, new_root.join(disk.file_name().unwrap())))
                .collect::<Result<Vec<_>, _>>()?;
        }
        Ok(self.update_root(new_root))
    }
}

impl<T: Serialize> DirectoryBackedArray<T> {
    fn next_target(&self) -> PathBuf {
        self.directory_root.join(Uuid::new_v4().to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        env::temp_dir,
        fs::{remove_dir_all, File},
    };

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
