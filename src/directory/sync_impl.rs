use std::{
    fs::{copy, create_dir_all, remove_dir, remove_file, rename},
    ops::{Deref, DerefMut, Range},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    array::{
        container::{
            open_mut, open_ref, BackedEntryContainer, BackedEntryContainerNested,
            BackedEntryContainerNestedAll, BackedEntryContainerNestedWrite, ResizingContainer,
        },
        sync_impl::BackedArray,
    },
    entry::{disks::Plainfile, BackedEntryArr},
};

/// [`BackedArray`] that uses a directory of plain files
#[derive(Debug, Serialize, Deserialize)]
pub struct DirectoryBackedArray<K, E> {
    array: BackedArray<K, E>,
    directory_root: PathBuf,
}

pub type StdDirBackedArray<T, Coder> =
    DirectoryBackedArray<Vec<Range<usize>>, Vec<BackedEntryArr<T, Plainfile, Coder>>>;

impl<K, E> DirectoryBackedArray<K, E>
where
    K: ResizingContainer<Data = Range<usize>>,
    E: BackedEntryContainerNestedAll + ResizingContainer,
    E::Disk: AsRef<Path>,
{
    pub fn remove(&mut self, entry_idx: usize) -> Result<&mut Self, std::io::Error> {
        if let Some(chunk) = self.entries().c_get(entry_idx) {
            remove_file(open_ref!(chunk).get_disk())?;
        }
        self.array.remove(entry_idx);
        Ok(self)
    }

    pub fn delete(mut self) -> Result<(), std::io::Error> {
        while !self.is_empty() {
            self.remove(0)?;
        }
        let _ = remove_dir(self.directory_root);
        Ok(())
    }
}

impl<K, E> DirectoryBackedArray<K, E>
where
    K: ResizingContainer<Data = Range<usize>>,
    E: BackedEntryContainerNestedWrite + ResizingContainer,
    E::Coder: Default,
    E::Disk: From<PathBuf>,
{
    pub fn append<U: Into<E::Unwrapped>>(&mut self, values: U) -> Result<&mut Self, E::WriteError> {
        let next_target = self.next_target();
        self.array
            .append(values, next_target, <E::Coder>::default())?;
        Ok(self)
    }

    pub fn append_memory<U: Into<E::Unwrapped>>(
        &mut self,
        values: U,
    ) -> Result<&mut Self, E::WriteError> {
        let next_target = self.next_target();
        self.array
            .append_memory(values, next_target, <E::Coder>::default())?;
        Ok(self)
    }
}

impl<K, E> DirectoryBackedArray<K, E>
where
    K: ResizingContainer<Data = Range<usize>>,
    E: BackedEntryContainerNestedWrite + ResizingContainer,
    E::Disk: AsRef<Path>,
{
    pub fn append_dir(&mut self, rhs: Self) -> Result<&mut Self, std::io::Error> {
        if self.directory_root != rhs.directory_root {
            rhs.entries()
                .ref_iter()
                .map(|chunk| {
                    let disk = open_ref!(chunk).get_disk().as_ref();
                    let new_loc = self.directory_root.join(disk.file_name().unwrap());
                    if disk != new_loc {
                        match rename(disk, &new_loc) {
                            Ok(_) => Ok(()),
                            Err(_) => {
                                copy(disk, &new_loc)?;
                                remove_file(disk)
                            }
                        }
                    } else {
                        Ok(())
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            let _ = remove_dir(rhs.directory_root);
        }
        self.array.merge(rhs.array);
        Ok(self)
    }
}

impl<K, E> Deref for DirectoryBackedArray<K, E> {
    type Target = BackedArray<K, E>;

    fn deref(&self) -> &Self::Target {
        &self.array
    }
}

impl<K, E> DerefMut for DirectoryBackedArray<K, E> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.array
    }
}

impl<K, E> DirectoryBackedArray<K, E>
where
    K: Default,
    E: Default,
{
    /// Creates a new directory at `directory_root`.
    ///
    /// * `directory_root`: Valid read/write directory on system
    pub fn new(directory_root: PathBuf) -> std::io::Result<Self> {
        create_dir_all(directory_root.clone())?;
        Ok(DirectoryBackedArray {
            array: BackedArray::<K, E>::default(),
            directory_root,
        })
    }
}

impl<K, E> DirectoryBackedArray<K, E>
where
    E: BackedEntryContainerNested,
    E::Disk: AsRef<Path> + From<PathBuf>,
{
    /// Updates the root of the directory backed array.
    ///
    /// Does not move any files or directories, just changes the stored root.
    pub fn update_root<P: AsRef<Path>>(&mut self, new_root: P) -> &mut Self {
        let new_root = new_root.as_ref();
        self.array.raw_chunks().for_each(|mut chunk| {
            let chunk = open_mut!(chunk);
            let disk = chunk.get_disk_mut();
            *disk = new_root.join(disk.as_ref().file_name().unwrap()).into();
        });
        self.directory_root = new_root.to_path_buf();
        self
    }

    /// Moves the directory to a new location wholesale.
    pub fn move_root(&mut self, new_root: PathBuf) -> std::io::Result<&mut Self> {
        if rename(self.directory_root.clone(), new_root.clone()).is_err() {
            create_dir_all(new_root.clone())?;
            self.array
                .entries()
                .ref_iter()
                .map(|chunk| {
                    let disk = open_ref!(chunk).get_disk().as_ref();
                    copy(disk, new_root.join(disk.file_name().unwrap()))?;
                    remove_file(disk)
                })
                .collect::<Result<Vec<_>, _>>()?;
        }
        Ok(self.update_root(new_root))
    }
}

impl<K, E> DirectoryBackedArray<K, E> {
    pub fn from_existing_array(array: BackedArray<K, E>, directory_root: PathBuf) -> Self {
        DirectoryBackedArray {
            array,
            directory_root,
        }
    }
}

impl<K, E> DirectoryBackedArray<K, E>
where
    E: BackedEntryContainerNested,
    E::Disk: From<PathBuf>,
{
    pub fn next_target(&self) -> E::Disk {
        self.directory_root.join(Uuid::new_v4().to_string()).into()
    }
}

#[cfg(test)]
#[cfg(feature = "bincode")]
mod tests {
    use std::{
        array,
        env::temp_dir,
        fs::{remove_dir_all, File},
    };

    use itertools::Itertools;

    use crate::entry::formats::{BincodeCoder, Decoder, Encoder};

    use super::*;

    fn values() -> (Box<[String]>, Box<[String]>) {
        (
            Box::<[String; 100]>::new(array::from_fn(|_| "TEST STRING".to_string())),
            Box::<[String; 100]>::new(array::from_fn(|_| "OTHER VALUE".to_string())),
        )
    }

    #[test]
    fn write() {
        let directory = temp_dir().join("directory_write");
        let _ = remove_dir_all(directory.clone());
        let mut arr = StdDirBackedArray::<String, BincodeCoder>::new(directory.clone()).unwrap();
        let (values, second_values) = values();

        arr.append_memory(values).unwrap();
        arr.append(second_values).unwrap();
        assert_eq!(arr.get(10).unwrap().as_ref(), &"TEST STRING");
        assert_eq!(arr.get(150).unwrap().as_ref(), &"OTHER VALUE");

        remove_dir_all(directory).unwrap();
    }

    #[test]
    fn write_and_read() {
        let directory = temp_dir().join("directory_write_and_read");
        let _ = remove_dir_all(directory.clone());
        let mut arr = StdDirBackedArray::<_, BincodeCoder>::new(directory.clone()).unwrap();
        let (values, second_values) = values();

        arr.append(values).unwrap();
        arr.append_memory(second_values).unwrap();
        BincodeCoder::default()
            .encode(
                &arr,
                &mut File::create(directory.join("meta.data")).unwrap(),
            )
            .unwrap();
        drop(arr);

        let arr: StdDirBackedArray<String, BincodeCoder> = BincodeCoder::default()
            .decode(&mut File::open(directory.join("meta.data")).unwrap())
            .unwrap();
        assert_eq!(arr.get(10).unwrap().as_ref(), &"TEST STRING");
        assert_eq!(arr.get(150).unwrap().as_ref(), &"OTHER VALUE");
        assert_eq!(arr.get(20).unwrap().as_ref(), &"TEST STRING");
        assert_eq!(arr.get(1).unwrap().as_ref(), &"TEST STRING");

        remove_dir_all(directory).unwrap();
    }
}
