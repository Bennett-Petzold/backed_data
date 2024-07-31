use std::{
    io::Write,
    ops::{Deref, DerefMut},
};

use itertools::Either;
use serde::{Deserialize, Serialize};

use crate::utils::Once;

use super::{
    disks::{ReadDisk, WriteDisk},
    formats::{Decoder, Encoder},
    BackedEntry, BackedEntryTrait,
};

impl<T: Once<Inner: Serialize>, Disk: WriteDisk, Coder: Encoder<Disk::WriteDisk>>
    BackedEntry<T, Disk, Coder>
{
    /// Updates underlying storage with the current entry
    pub fn update(&mut self) -> Result<(), Coder::Error> {
        if let Some(val) = self.value.get() {
            let mut disk = self.disk.write_disk()?;
            self.coder.encode(val, &mut disk)?;
            disk.flush()?; // Make sure buffer is emptied
        }
        Ok(())
    }

    /// Writes the new value to memory and disk.
    ///
    /// See [`Self::write_unload`] to skip the memory write.
    pub fn write(&mut self, new_value: T::Inner) -> Result<(), Coder::Error> {
        let mut disk = self.disk.write_disk()?;
        self.coder.encode(&new_value, &mut disk)?;
        disk.flush()?; // Make sure buffer is emptied

        // Drop previous value and write in new.
        // value.set() only works when uninitialized.
        self.value = T::new();
        let _ = self.value.set(new_value);
        Ok(())
    }
}

impl<T: Once<Inner: for<'de> Deserialize<'de>>, Disk: ReadDisk, Coder: Decoder<Disk::ReadDisk>>
    BackedEntry<T, Disk, Coder>
{
    /// Returns the entry, loading from disk if not in memory.
    ///
    /// Will remain in memory until an explicit call to unload.
    pub fn load(&self) -> Result<&T::Inner, Coder::Error> {
        let value = match self.value.get() {
            Some(x) => x,
            None => {
                let mut disk = self.disk.read_disk()?;
                let val = self.coder.decode(&mut disk)?;
                self.value.get_or_init(|| val)
            }
        };
        Ok(value)
    }
}

impl<T: Once, Disk, Coder> BackedEntry<T, Disk, Coder> {
    pub fn is_loaded(&self) -> bool {
        self.value.get().is_some()
    }
}

impl<T: Once, Disk, Coder> BackedEntry<T, Disk, Coder> {
    pub fn unload(&mut self) {
        self.value = T::new();
    }
}

impl<T: Once<Inner: Serialize>, Disk: WriteDisk, Coder: Encoder<Disk::WriteDisk>>
    BackedEntry<T, Disk, Coder>
{
    /// Write the value to disk only, unloading current memory.
    ///
    /// See [`Self::write`] to keep the value in memory.
    pub fn write_unload<U: Into<T::Inner>>(&mut self, new_value: U) -> Result<(), Coder::Error> {
        self.unload();
        let mut disk = self.disk.write_disk()?;
        self.coder.encode(&new_value.into(), &mut disk)?;
        disk.flush()?; // Make sure buffer is emptied
        Ok(())
    }
}

/// [`BackedEntryTrait`] that can be written to.
pub trait BackedEntryWrite:
    BackedEntryTrait<
    T: Once<Inner: Serialize>,
    Disk: WriteDisk,
    Coder: Encoder<
        <<Self as BackedEntryTrait>::Disk as WriteDisk>::WriteDisk,
        Error = Self::WriteError,
    >,
>
{
    type WriteError;
    fn get_inner_mut(&mut self) -> &mut BackedEntry<Self::T, Self::Disk, Self::Coder>;
}

impl<
        E: BackedEntryTrait<
            T: Once<Inner: Serialize>,
            Disk: WriteDisk,
            Coder: Encoder<<E::Disk as WriteDisk>::WriteDisk>,
        >,
    > BackedEntryWrite for E
{
    type WriteError = <E::Coder as Encoder<<E::Disk as WriteDisk>::WriteDisk>>::Error;
    fn get_inner_mut(&mut self) -> &mut BackedEntry<Self::T, Self::Disk, Self::Coder> {
        BackedEntryTrait::get_mut(self)
    }
}

/// [`BackedEntryTrait`] that can be read.
pub trait BackedEntryRead:
    BackedEntryTrait<
    T: Once<Inner: for<'de> Deserialize<'de>>,
    Disk: ReadDisk,
    Coder: Decoder<
        <<Self as BackedEntryTrait>::Disk as ReadDisk>::ReadDisk,
        Error = Self::ReadError,
    >,
>
{
    type ReadError;
    fn get_inner_ref(&self) -> &BackedEntry<Self::T, Self::Disk, Self::Coder>;
}

impl<
        E: BackedEntryTrait<
            T: Once<Inner: for<'de> Deserialize<'de>>,
            Disk: ReadDisk,
            Coder: Decoder<<E::Disk as ReadDisk>::ReadDisk>,
        >,
    > BackedEntryRead for E
{
    type ReadError = <E::Coder as Decoder<<E::Disk as ReadDisk>::ReadDisk>>::Error;
    fn get_inner_ref(&self) -> &BackedEntry<Self::T, Self::Disk, Self::Coder> {
        BackedEntryTrait::get_ref(self)
    }
}

/// Gives mutable handle to a backed entry.
///
/// Modifying by [`BackedEntry::write`] writes the entire value to the
/// underlying storage on every modification. This allows for multiple values
/// values to be written before syncing with disk.
///
/// Call [`Self::flush`] to sync changes with underlying storage before
/// dropping. Otherwise, a panicking drop implementation runs.
pub struct BackedEntryMut<'a, E: BackedEntryWrite> {
    entry: &'a mut E,
    modified: bool,
}

impl<E: BackedEntryWrite + BackedEntryRead> Deref for BackedEntryMut<'_, E> {
    type Target = <E::T as Once>::Inner;

    fn deref(&self) -> &Self::Target {
        self.entry.get_inner_ref().value.get().unwrap()
    }
}

impl<E: BackedEntryWrite + BackedEntryRead> DerefMut for BackedEntryMut<'_, E> {
    /// [`DerefMut::deref_mut`] that sets a modified flag.
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.modified = true;
        self.entry.get_inner_mut().value.get_mut().unwrap()
    }
}

impl<E: BackedEntryWrite> BackedEntryMut<'_, E> {
    /// Returns true if the memory version is desynced from the disk version
    #[allow(dead_code)]
    pub fn is_modified(&self) -> bool {
        self.modified
    }

    /// Saves modifications to disk, unsetting the modified flag if sucessful.
    pub fn flush(&mut self) -> Result<&mut Self, E::WriteError> {
        self.entry.get_inner_mut().update()?;
        self.modified = false;
        Ok(self)
    }
}

impl<E: BackedEntryWrite> Drop for BackedEntryMut<'_, E> {
    /// [`Drop::drop`] that attempts a write if modified, and panics if that
    /// write returns and error.
    fn drop(&mut self) {
        if self.modified && self.flush().is_err() {
            panic!("BackedEntryMut dropped while modified, and failed to flush.");
        }
    }
}

impl<'a, E: BackedEntryRead + BackedEntryWrite> BackedEntryMut<'a, E> {
    /// Returns [`BackedEntryMut`] to allow efficient in-memory modifications
    /// if variable-sized writes are safe for the underlying storage.
    ///
    /// Make sure to call [`BackedEntryMut::flush`] to sync with disk before
    /// dropping.
    pub fn mut_handle(backed: &'a mut E) -> Result<Self, E::ReadError> {
        backed.get_mut().load()?;
        Ok(BackedEntryMut {
            entry: backed,
            modified: false,
        })
    }
}

impl<
        T: Once<Inner: Serialize + for<'de> Deserialize<'de>>,
        Disk: WriteDisk + ReadDisk,
        Coder: Encoder<Disk::WriteDisk> + Decoder<Disk::ReadDisk>,
    > BackedEntry<T, Disk, Coder>
{
    /// Convenience wrapper for [`BackedEntryMut::mut_handle`]
    pub fn mut_handle(
        &mut self,
    ) -> Result<BackedEntryMut<Self>, <Coder as Decoder<Disk::ReadDisk>>::Error> {
        BackedEntryMut::mut_handle(self)
    }
}

impl<
        T: Once<Inner: for<'de> Deserialize<'de> + Serialize>,
        Disk: ReadDisk,
        Coder: Decoder<Disk::ReadDisk>,
    > BackedEntry<T, Disk, Coder>
{
    /// Converts [`self`] to another disk and encoding representation.
    ///
    /// This loads the value from the original disk in the original format (if necessary),
    /// and then encodes the value to the target disk. The original disk is not deleted.
    /// This may produce a disk read, and will always produce a disk write.
    ///
    /// If switching between compatible coders (e.g. simd_json -> serde_json)
    /// without switching the backing disk, use [`Self::encoder_into`] to skip
    /// the extraneous disk reads/writes.
    pub fn change_backing<OtherDisk, OtherCoder>(
        self,
        disk: OtherDisk,
        coder: OtherCoder,
    ) -> Result<BackedEntry<T, OtherDisk, OtherCoder>, Either<Coder::Error, OtherCoder::Error>>
    where
        OtherDisk: WriteDisk,
        OtherCoder: Encoder<OtherDisk::WriteDisk>,
    {
        self.load().map_err(Either::Left)?;
        let mut other = BackedEntry::<T, OtherDisk, OtherCoder> {
            value: self.value,
            disk,
            coder,
        };
        other.update().map_err(Either::Right)?;
        Ok(other)
    }

    /// Replaces [`self`]'s encoder without any disk operation.
    pub fn encoder_into<OtherCoder>(self) -> BackedEntry<T, Disk, OtherCoder>
    where
        OtherCoder: From<Coder>,
    {
        BackedEntry {
            value: self.value,
            disk: self.disk,
            coder: self.coder.into(),
        }
    }

    /// Replaces [`self`]'s encoder without any disk operation.
    pub fn encoder_try_into<OtherCoder>(
        self,
    ) -> Result<BackedEntry<T, Disk, OtherCoder>, OtherCoder::Error>
    where
        OtherCoder: TryFrom<Coder>,
    {
        Ok(BackedEntry {
            value: self.value,
            disk: self.disk,
            coder: self.coder.try_into()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::UnsafeCell, collections::HashMap, io::Cursor, sync::Arc, thread::scope};

    use crate::{
        entry::{formats::BincodeCoder, BackedEntryArr, BackedEntryArrLock, BackedEntryCell},
        test_utils::CursorVec,
    };

    use super::*;

    #[cfg(feature = "bincode")]
    #[test]
    fn mutate() {
        const FIB: &[u8] = &[0, 1, 1, 5, 7];
        let mut binding = Cursor::new(Vec::with_capacity(10));
        let back_vec = UnsafeCell::new(CursorVec {
            inner: (&mut binding).into(),
        });

        // Intentional unsafe access to later peek underlying storage
        let mut backed_entry =
            unsafe { BackedEntryArr::new(&mut *back_vec.get(), BincodeCoder {}) };
        backed_entry.write_unload(FIB).unwrap();

        assert_eq!(backed_entry.load().unwrap().as_ref(), FIB);

        let backing_store = unsafe { &*back_vec.get() }.get_ref();
        assert_eq!(&backing_store[backing_store.len() - FIB.len()..], FIB);

        let mut handle = backed_entry.mut_handle().unwrap();
        handle[0] = 20;
        handle[2] = 30;

        let backing_store = unsafe { &*back_vec.get() }.get_ref();
        assert_eq!(backing_store[backing_store.len() - FIB.len()], FIB[0]);
        assert_eq!(handle[0], 20);
        assert_eq!(handle[2], 30);

        handle.flush().unwrap();
        let backing_store = unsafe { &*back_vec.get() }.get_ref();
        assert_eq!(backing_store[backing_store.len() - FIB.len()], 20);
        assert_eq!(backing_store[backing_store.len() - FIB.len() + 2], 30);
        assert_eq!(backing_store[backing_store.len() - FIB.len() + 1], FIB[1]);

        drop(handle);
        assert_eq!(backed_entry.load().unwrap().as_ref(), [20, 1, 30, 5, 7]);
    }

    #[cfg(feature = "bincode")]
    #[test]
    fn mutate_option() {
        let mut input: HashMap<String, u128> = HashMap::new();
        input.insert("THIS IS A STRING".to_string(), 55);
        input.insert("THIS IS ALSO A STRING".to_string(), 23413);

        let mut binding = Cursor::new(Vec::with_capacity(10));
        let mut back_vec = CursorVec {
            inner: (&mut binding).into(),
        };

        // Intentional unsafe access to later peek underlying storage
        let mut backed_entry = BackedEntryCell::new(&mut back_vec, BincodeCoder {});
        backed_entry.write_unload(input.clone()).unwrap();

        assert_eq!(&input, backed_entry.load().unwrap());
        let mut handle = backed_entry.mut_handle().unwrap();
        handle
            .deref_mut()
            .insert("EXTRA STRING".to_string(), 234137);
        handle.flush().unwrap();

        drop(handle);
        assert_eq!(
            backed_entry.load().unwrap().get("EXTRA STRING").unwrap(),
            &234137
        );
    }

    #[cfg(feature = "bincode")]
    #[test]
    fn write() {
        const VALUE: &[u8] = &[5];
        const NEW_VALUE: &[u8] = &[7];

        let mut binding = Cursor::new(Vec::with_capacity(1));
        let back_vec = UnsafeCell::new(CursorVec {
            inner: (&mut binding).into(),
        });

        // Intentional unsafe access to later peek underlying storage
        let mut backed_entry =
            unsafe { BackedEntryArr::new(&mut *back_vec.get(), BincodeCoder {}) };

        backed_entry.write_unload(VALUE).unwrap();
        assert!(!backed_entry.is_loaded());
        let back_vec_inner = unsafe { (*back_vec.get()).get_mut() };
        assert_eq!(&back_vec_inner[back_vec_inner.len() - VALUE.len()..], VALUE);
        assert_eq!(backed_entry.load().unwrap().as_ref(), VALUE);

        backed_entry.write(NEW_VALUE.into()).unwrap();
        assert!(backed_entry.is_loaded());
        let back_vec_inner = unsafe { (*back_vec.get()).get_mut() };
        assert_eq!(
            &back_vec_inner[back_vec_inner.len() - NEW_VALUE.len()..],
            NEW_VALUE
        );
        assert_eq!(backed_entry.load().unwrap().as_ref(), NEW_VALUE);
    }

    #[cfg(feature = "bincode")]
    #[test]
    fn read_threaded() {
        const VALUES: &[u8] = &[0, 1, 3, 5, 7];
        const NEW_VALUES: &[u8] = &[7, 5, 3, 1, 0];

        let mut binding = Cursor::new(Vec::with_capacity(10));
        let mut back_vec = CursorVec {
            inner: (&mut binding).into(),
        };

        let mut backed_entry = BackedEntryArrLock::new(&mut back_vec, BincodeCoder {});

        backed_entry.write_unload(VALUES).unwrap();
        assert!(!backed_entry.is_loaded());
        scope(|s| {
            let backed_share = Arc::new(&backed_entry);
            (0..VALUES.len()).for_each(|idx| {
                let backed_share = backed_share.clone();
                s.spawn(move || assert_eq!(backed_share.load().unwrap()[idx], VALUES[idx]));
            });
        });

        backed_entry.write(NEW_VALUES.into()).unwrap();
        assert!(backed_entry.is_loaded());
        scope(|s| {
            let backed_share = Arc::new(&backed_entry);
            (0..NEW_VALUES.len()).for_each(|idx| {
                let backed_share = backed_share.clone();
                s.spawn(move || assert_eq!(backed_share.load().unwrap()[idx], NEW_VALUES[idx]));
            });
        });
    }

    #[cfg(all(feature = "bincode", feature = "serde_json"))]
    #[test]
    fn change_coder() {
        use crate::entry::formats::SerdeJsonCoder;

        const VALUES: &[u8] = &[0, 1, 3, 5, 7];
        const VALUES_JSON: &str = "[\n  0,\n  1,\n  3,\n  5,\n  7\n]";
        let mut binding = Cursor::new(Vec::with_capacity(10));
        let back_vec = UnsafeCell::new(CursorVec {
            inner: (&mut binding).into(),
        });

        // Intentional unsafe access to later peek underlying storage
        let mut backed_entry =
            unsafe { BackedEntryArr::new(&mut *back_vec.get(), BincodeCoder {}) };
        backed_entry.write(VALUES.into()).unwrap();

        // Check for valid bincode encoding
        {
            let backing_store = unsafe { &*back_vec.get() }.get_ref();
            assert_eq!(backing_store[backing_store.len() - VALUES.len()], VALUES[0]);
        }

        let mut backed_entry = backed_entry
            .change_backing(unsafe { &mut *back_vec.get() }, SerdeJsonCoder {})
            .unwrap();

        // Check for valid json encoding
        {
            let backing_store = unsafe { &*back_vec.get() }.get_ref();
            assert_eq!(std::str::from_utf8(backing_store).unwrap(), VALUES_JSON);
        }

        // Check that data is preserved with json backing
        assert_eq!(backed_entry.load().unwrap().as_ref(), VALUES);
        backed_entry.unload();
        assert_eq!(backed_entry.load().unwrap().as_ref(), VALUES);
    }

    #[cfg(all(feature = "serde_json", feature = "simd_json"))]
    #[test]
    fn encoder_into() {
        use crate::entry::formats::{SerdeJsonCoder, SimdJsonCoder};

        const VALUES: &[u8] = &[0, 1, 3, 5, 7];
        const VALUES_JSON: &str = "[\n  0,\n  1,\n  3,\n  5,\n  7\n]";
        let mut binding = Cursor::new(Vec::with_capacity(10));
        let back_vec = UnsafeCell::new(CursorVec {
            inner: (&mut binding).into(),
        });

        // Intentional unsafe access to later peek underlying storage
        let mut backed_entry =
            unsafe { BackedEntryArr::new(&mut *back_vec.get(), SerdeJsonCoder {}) };
        backed_entry.write(VALUES.into()).unwrap();

        // Check for valid serde json encoding
        {
            let backing_store = unsafe { &*back_vec.get() }.get_ref();
            assert_eq!(std::str::from_utf8(backing_store).unwrap(), VALUES_JSON);
        }

        let mut backed_entry = backed_entry.encoder_into::<SimdJsonCoder>();

        // Check for valid simd json encoding
        {
            let backing_store = unsafe { &*back_vec.get() }.get_ref();
            assert_eq!(std::str::from_utf8(backing_store).unwrap(), VALUES_JSON);
        }

        // Check that data is preserved in the alternative reader
        assert_eq!(backed_entry.load().unwrap().as_ref(), VALUES);
        backed_entry.unload();
        assert_eq!(backed_entry.load().unwrap().as_ref(), VALUES);
    }
}
