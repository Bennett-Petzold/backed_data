use std::{
    fs::File,
    io::{BufReader, BufWriter, Read, Write},
    ops::{Deref, DerefMut},
    path::PathBuf,
};

use bincode::Options;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use super::{BackedEntry, BackedEntryUnload};

pub trait ReadDisk: Serialize + for<'de> Deserialize<'de> {
    type ReadDisk: Read;
    fn read_disk(&mut self) -> std::io::Result<Self::ReadDisk>;
}

pub trait WriteDisk: Serialize + for<'de> Deserialize<'de> {
    type WriteDisk: Write;
    fn write_disk(&mut self) -> std::io::Result<Self::WriteDisk>;
}

impl ReadDisk for PathBuf {
    type ReadDisk = BufReader<File>;

    fn read_disk(&mut self) -> std::io::Result<Self::ReadDisk> {
        Ok(BufReader::new(File::open(self.clone())?))
    }
}

impl WriteDisk for PathBuf {
    type WriteDisk = BufWriter<File>;

    fn write_disk(&mut self) -> std::io::Result<Self::WriteDisk> {
        Ok(BufWriter::new(
            File::options()
                .write(true)
                .create(true)
                .truncate(true)
                .open(self.clone())?,
        ))
    }
}

impl<T: Serialize, Disk: WriteDisk> BackedEntry<T, Disk> {
    /// Updates underlying storage with the current entry
    fn update(&mut self) -> bincode::Result<()> {
        if let Some(val) = self.value.as_ref() {
            let mut disk = self.disk.write_disk()?;
            bincode::options()
                .with_limit(u32::MAX as u64)
                .allow_trailing_bytes()
                .serialize_into(&mut disk, &val)?;
            disk.flush()?; // Make sure buffer is emptied
        }
        Ok(())
    }

    /// Writes the new value to memory and disk.
    ///
    /// See [`Self::write_unload`] to skip the memory write.
    pub fn write(&mut self, new_value: T) -> bincode::Result<()> {
        let mut disk = self.disk.write_disk()?;
        self.value = Some(new_value);
        bincode::options()
            .with_limit(u32::MAX as u64)
            .allow_trailing_bytes()
            .serialize_into(&mut disk, self.value.as_ref().unwrap())?;
        disk.flush()?; // Make sure buffer is emptied
        Ok(())
    }
}

impl<T: Serialize, Disk: for<'de> Deserialize<'de>> BackedEntry<T, Disk> where
    BackedEntry<T, Disk>: BackedEntryUnload
{
}

impl<T: DeserializeOwned, Disk: ReadDisk> BackedEntry<T, Disk> {
    /// Returns the entry, loading from disk if not in memory.
    ///
    /// Will remain in memory until an explicit call to unload.
    pub fn load(&mut self) -> bincode::Result<&T> {
        if self.value.is_none() {
            let disk = self.disk.read_disk()?;
            self.value = Some(
                bincode::options()
                    .with_limit(u32::MAX as u64)
                    .allow_trailing_bytes()
                    .deserialize_from(disk)?,
            );
        }
        Ok(self.value.as_ref().unwrap())
    }
}

impl<T, Disk: for<'de> Deserialize<'de>> BackedEntry<T, Disk> {
    pub fn is_loaded(&self) -> bool {
        self.value.is_some()
    }
}

impl<T, Disk: for<'de> Deserialize<'de>> BackedEntryUnload for BackedEntry<T, Disk> {
    fn unload(&mut self) {
        self.value = None;
    }
}

impl<T: Serialize, Disk: WriteDisk> BackedEntry<T, Disk> {
    /// Write the value to disk only, unloading current memory.
    ///
    /// See [`Self::write`] to keep the value in memory.
    pub fn write_unload<U: Into<T>>(&mut self, new_value: U) -> bincode::Result<()> {
        self.unload();
        let mut disk = self.disk.write_disk()?;
        bincode::options()
            .with_limit(u32::MAX as u64)
            .allow_trailing_bytes()
            .serialize_into(&mut disk, &new_value.into())?;
        disk.flush()?; // Make sure buffer is emptied
        Ok(())
    }
}

/// Gives mutable handle to a backed entry.
///
/// Modifying by [`BackedEntry::write`] writes the entire value to the
/// underlying storage on every modification. This allows for multiple values
/// values to be written before syncing with disk.
///
/// Call [`BackedEntryMut::flush`] to sync with underlying storage before
/// dropping. Otherwise, a panicking drop implementation runs.
pub struct BackedEntryMut<'a, T: Serialize, Disk: WriteDisk> {
    entry: &'a mut BackedEntry<T, Disk>,
    modified: bool,
}

impl<'a, T: Serialize, Disk: WriteDisk> Deref for BackedEntryMut<'a, T, Disk> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.entry.value.as_ref().unwrap()
    }
}

impl<'a, T: Serialize, Disk: WriteDisk> DerefMut for BackedEntryMut<'a, T, Disk> {
    /// [`DerefMut::deref_mut`] that sets a modified flag.
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.modified = true;
        self.entry.value.as_mut().unwrap()
    }
}

impl<'a, T: Serialize, Disk: WriteDisk> BackedEntryMut<'a, T, Disk> {
    /// Returns true if the memory version is desynced from the disk version
    #[allow(dead_code)]
    pub fn is_modified(&self) -> bool {
        self.modified
    }

    /// Saves modifications to disk, unsetting the modified flag if sucessful.
    pub fn flush(&mut self) -> bincode::Result<&mut Self> {
        self.entry.update()?;
        self.modified = false;
        Ok(self)
    }
}

impl<'a, T: Serialize, Disk: WriteDisk> Drop for BackedEntryMut<'a, T, Disk> {
    /// [`Drop::drop`] that attempts a write if modified, and panics if that
    /// write returns and error.
    fn drop(&mut self) {
        if self.modified {
            self.flush().unwrap();
        }
    }
}

impl<T: Serialize + DeserializeOwned, Disk: WriteDisk + ReadDisk> BackedEntry<T, Disk> {
    /// Returns [`BackedEntryMut`] to allow efficient in-memory modifications
    /// if variable-sized writes are safe for the underlying storage.
    ///
    /// Make sure to call [`BackedEntryMut::flush`] to sync with disk before
    /// dropping.
    pub fn mut_handle(&mut self) -> bincode::Result<BackedEntryMut<T, Disk>> {
        self.load()?;
        Ok(BackedEntryMut {
            entry: self,
            modified: false,
        })
    }
}

impl<T, Disk: for<'de> Deserialize<'de>> BackedEntry<T, Disk> {
    pub fn replace_disk<OtherDisk>(self) -> BackedEntry<T, OtherDisk>
    where
        OtherDisk: for<'de> Deserialize<'de>,
        Disk: Into<OtherDisk>,
    {
        BackedEntry::<T, OtherDisk> {
            value: self.value,
            disk: self.disk.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, io::Cursor};

    use crate::{entry::BackedEntryArr, test_utils::CursorVec};

    use super::*;

    #[test]
    fn mutate() {
        const FIB: &[u8] = &[0, 1, 1, 5, 7];
        let mut back_vec = CursorVec {
            inner: &mut Cursor::new(Vec::with_capacity(10)),
        };
        let back_vec_ptr: *mut CursorVec = &mut back_vec;

        // Intentional unsafe access to later peek underlying storage
        let mut backed_entry = unsafe { BackedEntryArr::new(&mut *back_vec_ptr) };
        backed_entry.write_unload(FIB).unwrap();

        assert_eq!(backed_entry.load().unwrap().as_ref(), FIB);

        let backing_store = back_vec.inner.get_ref();
        assert_eq!(&backing_store[backing_store.len() - FIB.len()..], FIB);

        let mut handle = backed_entry.mut_handle().unwrap();
        handle[0] = 20;
        handle[2] = 30;

        let backing_store = back_vec.inner.get_ref();
        assert_eq!(backing_store[backing_store.len() - FIB.len()], FIB[0]);
        assert_eq!(handle[0], 20);
        assert_eq!(handle[2], 30);

        handle.flush().unwrap();
        let backing_store = back_vec.inner.get_ref();
        assert_eq!(backing_store[backing_store.len() - FIB.len()], 20);
        assert_eq!(backing_store[backing_store.len() - FIB.len() + 2], 30);
        assert_eq!(backing_store[backing_store.len() - FIB.len() + 1], FIB[1]);

        drop(handle);
        assert_eq!(backed_entry.load().unwrap().as_ref(), [20, 1, 30, 5, 7]);
    }

    #[test]
    fn mutate_option() {
        let mut input: HashMap<String, u128> = HashMap::new();
        input.insert("THIS IS A STRING".to_string(), 55);
        input.insert("THIS IS ALSO A STRING".to_string(), 23413);

        let mut back_vec = CursorVec {
            inner: &mut Cursor::new(Vec::with_capacity(10)),
        };

        // Intentional unsafe access to later peek underlying storage
        let mut backed_entry = BackedEntry::new(&mut back_vec);
        backed_entry.write_unload(input.clone()).unwrap();

        assert_eq!(&input, backed_entry.load().unwrap());
        let mut handle = backed_entry.mut_handle().unwrap();
        handle.insert("EXTRA STRING".to_string(), 234137);
        handle.flush().unwrap();

        drop(handle);
        assert_eq!(
            backed_entry.load().unwrap().get("EXTRA STRING").unwrap(),
            &234137
        );
    }
}
