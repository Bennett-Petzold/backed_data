use std::{
    ops::{Deref, DerefMut},
    path::PathBuf,
};

use async_trait::async_trait;

use itertools::Itertools;
use serde::{
    de::{DeserializeOwned, Error},
    Deserialize, Serialize,
};
use tokio::fs::{create_dir_all, remove_file};
use tokio::{
    fs::{copy, remove_dir_all, rename},
    task::JoinSet,
};
use uuid::Uuid;

use crate::{
    array::async_impl::BackedArray, array::sync_impl::BackedArray as SyncBackedArray,
    directory::sync_impl::DirectoryBackedArray as SyncDirBackedArray,
    meta::async_impl::BackedArrayWrapper,
};

/// [`BackedArray`] that uses a directory of plain files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryBackedArray<T> {
    array: BackedArray<T, PathBuf>,
    directory_root: PathBuf,
}

#[async_trait]
impl<T: Serialize + DeserializeOwned + Sync + Send> BackedArrayWrapper<T>
    for DirectoryBackedArray<T>
{
    type Storage = PathBuf;
    type BackingError = std::io::Error;

    async fn remove(&mut self, entry_idx: usize) -> Result<&mut Self, std::io::Error> {
        remove_file(self.get_disks()[entry_idx].clone()).await?;
        self.array.remove(entry_idx);
        Ok(self)
    }

    async fn append<U: Into<Box<[T]>> + Send>(&mut self, values: U) -> bincode::Result<&mut Self> {
        let next_target = self.next_target().await.map_err(bincode::Error::custom)?;
        self.array.append(values, next_target).await?;
        Ok(self)
    }

    async fn append_memory<U: Into<Box<[T]>> + Send>(
        &mut self,
        values: U,
    ) -> bincode::Result<&mut Self> {
        let next_target = self.next_target().await.map_err(bincode::Error::custom)?;
        self.array.append_memory(values, next_target).await?;
        Ok(self)
    }

    async fn append_array(&mut self, rhs: Self) -> Result<&mut Self, Self::BackingError> {
        let mut copy_futures = JoinSet::new();

        if self.directory_root != rhs.directory_root {
            let disks: Vec<PathBuf> = rhs.array.get_disks().into_iter().cloned().collect_vec();
            disks.into_iter().for_each(|path| {
                let new_root_clone = self.directory_root.clone();
                copy_futures.spawn(async move {
                    copy(path.clone(), new_root_clone.join(path.file_name().unwrap())).await
                });
            });

            remove_dir_all(rhs.directory_root).await?;
        }

        self.array.append_array(rhs.array);

        while let Some(future) = copy_futures.join_next().await {
            let _ = future?;
        }

        Ok(self)
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
    pub async fn new(directory_root: PathBuf) -> std::io::Result<Self> {
        create_dir_all(directory_root.clone()).await?;
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
    pub async fn move_root(&mut self, new_root: PathBuf) -> std::io::Result<&mut Self> {
        let mut copy_futures = JoinSet::new();
        if rename(self.directory_root.clone(), new_root.clone())
            .await
            .is_err()
        {
            create_dir_all(new_root.clone()).await?;

            let disks: Vec<PathBuf> = self.array.get_disks().into_iter().cloned().collect_vec();
            disks.into_iter().for_each(|path| {
                let new_root_clone = new_root.clone();
                copy_futures.spawn(async move {
                    copy(path.clone(), new_root_clone.join(path.file_name().unwrap())).await
                });
            });
        }

        self.update_root(new_root);
        while let Some(future) = copy_futures.join_next().await {
            let _ = future?;
        }
        Ok(self)
    }
}

impl<T: Serialize> DirectoryBackedArray<T> {
    async fn next_target(&self) -> std::io::Result<PathBuf> {
        Ok(self.directory_root.join(Uuid::new_v4().to_string()))
    }

    /// Convert to synchronous version
    pub async fn conv_to_sync(self) -> bincode::Result<SyncDirBackedArray<T>> {
        Ok(SyncDirBackedArray::from_existing_array(
            self.array.to_sync_array().await?,
            self.directory_root,
        ))
    }
}

impl<T: Serialize + Clone> DirectoryBackedArray<T> {
    /// Utility method for synchronous to asynchronous conversions.
    pub async fn from_existing_array(
        array: SyncBackedArray<T, PathBuf>,
        directory_root: PathBuf,
    ) -> bincode::Result<Self> {
        let mut new_array = BackedArray::new();
        new_array.append_sync_array(array).await?;

        Ok(DirectoryBackedArray {
            array: new_array,
            directory_root,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::env::temp_dir;

    use itertools::Itertools;
    use tokio::fs::{remove_dir_all, File};

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

    #[tokio::test]
    async fn write() {
        let directory = temp_dir().join("directory_write_async");
        let _ = remove_dir_all(directory.clone()).await;
        let mut arr = DirectoryBackedArray::new(directory.clone()).await.unwrap();
        let (values, second_values) = values();

        arr.append_memory(values).await.unwrap();
        arr.append(second_values).await.unwrap();
        assert_eq!(arr.get(100).await.unwrap(), &"TEST STRING");
        assert_eq!(arr.get(15_000).await.unwrap(), &"OTHER VALUE");

        remove_dir_all(directory).await.unwrap();
    }

    #[tokio::test]
    async fn write_and_read() {
        let directory = temp_dir().join("directory_write_and_read_async");
        let _ = remove_dir_all(directory.clone()).await;
        let mut arr = DirectoryBackedArray::new(directory.clone()).await.unwrap();
        let (values, second_values) = values();

        arr.append(values).await.unwrap();
        arr.append_memory(second_values).await.unwrap();
        arr.save_to_disk(&mut File::create(directory.join("directory")).await.unwrap())
            .await
            .unwrap();
        drop(arr);

        let mut arr: DirectoryBackedArray<String> =
            DirectoryBackedArray::load(&mut File::open(directory.join("directory")).await.unwrap())
                .await
                .unwrap();
        assert_eq!(arr.get(100).await.unwrap(), &"TEST STRING");
        assert_eq!(arr.get(15_000).await.unwrap(), &"OTHER VALUE");
        assert_eq!(arr.get(200).await.unwrap(), &"TEST STRING");
        assert_eq!(arr.get(1).await.unwrap(), &"TEST STRING");

        remove_dir_all(directory).await.unwrap();
    }

    #[tokio::test]
    async fn cross_write_and_read() {
        let directory = temp_dir().join("directory_write_and_read_async_cross");
        let _ = remove_dir_all(directory.clone()).await;
        let mut arr = DirectoryBackedArray::new(directory.clone()).await.unwrap();
        let (values, second_values) = values();

        arr.append(values).await.unwrap();
        arr.append_memory(second_values).await.unwrap();
        arr.save_to_disk(&mut File::create(directory.join("directory")).await.unwrap())
            .await
            .unwrap();
        drop(arr);

        let mut arr: DirectoryBackedArray<String> =
            DirectoryBackedArray::load(&mut File::open(directory.join("directory")).await.unwrap())
                .await
                .unwrap();
        assert_eq!(arr.get(100).await.unwrap(), &"TEST STRING");
        assert_eq!(arr.get(15_000).await.unwrap(), &"OTHER VALUE");
        assert_eq!(arr.get(200).await.unwrap(), &"TEST STRING");
        assert_eq!(arr.get(1).await.unwrap(), &"TEST STRING");

        remove_dir_all(directory).await.unwrap();
    }
}
