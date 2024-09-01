/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use std::ops::{Deref, DerefMut};

use either::Either;
use serde::{Deserialize, Serialize};

use crate::utils::Once;

use super::{
    disks::{ReadDisk, WriteDisk},
    formats::{Decoder, Encoder},
    BackedEntry, BackedEntryInner, BackedEntryMutHandle, BackedEntryRead, BackedEntryWrite,
    MutHandle,
};

/// Synchronous implementation for [`BackedEntry`].
///
/// # Generics
///
/// * `T`: the type to store.
/// * `Disk`: an implementor of [`ReadDisk`](`super::disks::ReadDisk`) and/or [`WriteDisk`](`super::disks::WriteDisk`).
/// * `Coder`: an implementor of [`Encoder`](`super::formats::Encoder`) and/or [`Decoder`](`super::formats::Decoder`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BackedEntrySync<T: Once, Disk, Coder>(BackedEntryInner<T, Disk, Coder>);

// Copy over all the shared impls
impl<T: Once, Disk, Coder> BackedEntry for BackedEntrySync<T, Disk, Coder> {
    type OnceWrapper = T;
    type Disk = Disk;
    type Coder = Coder;
    fn get_storage(&self) -> (&Self::Disk, &Self::Coder) {
        self.0.get_storage()
    }
    fn get_storage_mut(&mut self) -> (&mut Self::Disk, &mut Self::Coder) {
        self.0.get_storage_mut()
    }
    fn is_loaded(&self) -> bool {
        self.0.is_loaded()
    }
    fn unload(&mut self) {
        self.0.unload()
    }
    fn take(self) -> Option<<Self::OnceWrapper as Once>::Inner> {
        self.0.take()
    }
    fn new(disk: Self::Disk, coder: Self::Coder) -> Self
    where
        Self: Sized,
    {
        Self(BackedEntry::new(disk, coder))
    }
}

impl<T, Disk, Coder> BackedEntryRead for BackedEntrySync<T, Disk, Coder>
where
    T: Once<Inner: for<'de> Deserialize<'de>>,
    Disk: ReadDisk,
    Coder: Decoder<Disk::ReadDisk, T = T::Inner>,
{
    type LoadResult<'a> = Result<&'a T::Inner, Coder::Error> where Self: 'a;
    fn load(&self) -> Self::LoadResult<'_> {
        let this = &self.0;

        let value = match this.value.get() {
            Some(x) => x,
            None => {
                let mut disk = this.disk.read_disk()?;
                let val = this.coder.decode(&mut disk)?;
                this.value.get_or_init(|| val)
            }
        };
        Ok(value)
    }
}

impl<T, Disk, Coder> BackedEntryWrite for BackedEntrySync<T, Disk, Coder>
where
    T: Once<Inner: Serialize>,
    Disk: WriteDisk,
    Coder: Encoder<Disk::WriteDisk, T = T::Inner>,
{
    type UpdateResult<'a> = Result<(), Coder::Error> where Self: 'a;
    fn update(&mut self) -> Self::UpdateResult<'_> {
        let this = &mut self.0;

        if let Some(val) = this.value.get() {
            let disk = this.disk.write_disk()?;
            this.coder.encode(val, disk)?;
        }
        Ok(())
    }

    fn write<U>(&mut self, new_value: U) -> Self::UpdateResult<'_>
    where
        U: Into<<Self::OnceWrapper as Once>::Inner>,
    {
        let this = &mut self.0;
        let new_value = new_value.into();

        let disk = this.disk.write_disk()?;
        this.coder.encode(&new_value, disk)?;

        // Drop previous value and write in new.
        // value.set() only works when uninitialized.
        this.value = T::new();
        let _ = this.value.set(new_value);
        Ok(())
    }

    fn write_ref(
        &mut self,
        new_value: &<Self::OnceWrapper as Once>::Inner,
    ) -> Self::UpdateResult<'_> {
        let this = &mut self.0;

        this.unload();
        let disk = this.disk.write_disk()?;
        this.coder.encode(new_value, disk)?;
        Ok(())
    }
}

/// [`MutHandle`] for [`BackedEntrySync`].
pub struct BackedEntryMut<
    'a,
    T: Once<Inner: Serialize>,
    Disk: WriteDisk,
    Coder: Encoder<Disk::WriteDisk, T = T::Inner>,
> {
    entry: &'a mut BackedEntrySync<T, Disk, Coder>,
    modified: bool,
}

impl<T, Disk, Coder> Deref for BackedEntryMut<'_, T, Disk, Coder>
where
    T: Once<Inner: Serialize>,
    Disk: WriteDisk,
    Coder: Encoder<Disk::WriteDisk, T = T::Inner>,
{
    type Target = T::Inner;

    fn deref(&self) -> &Self::Target {
        self.entry.0.value.get().unwrap()
    }
}

impl<T, Disk, Coder> DerefMut for BackedEntryMut<'_, T, Disk, Coder>
where
    T: Once<Inner: Serialize>,
    Disk: WriteDisk,
    Coder: Encoder<Disk::WriteDisk, T = T::Inner>,
{
    /// [`DerefMut::deref_mut`] that sets a modified flag.
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.modified = true;
        self.entry.0.value.get_mut().unwrap()
    }
}

impl<T, Disk, Coder> MutHandle<<T as Once>::Inner> for BackedEntryMut<'_, T, Disk, Coder>
where
    T: Once<Inner: Serialize>,
    Disk: WriteDisk,
    Coder: Encoder<Disk::WriteDisk, T = T::Inner>,
{
    fn is_modified(&self) -> bool {
        self.modified
    }

    type FlushResult<'a> = Result<&'a mut Self, Coder::Error> where Self: 'a;
    fn flush(&mut self) -> Self::FlushResult<'_> {
        // No need to update if unmodified
        if self.modified {
            self.entry.update()?;
            self.modified = false;
        }
        Ok(self)
    }
}

impl<T, Disk, Coder> Drop for BackedEntryMut<'_, T, Disk, Coder>
where
    T: Once<Inner: Serialize>,
    Disk: WriteDisk,
    Coder: Encoder<Disk::WriteDisk, T = T::Inner>,
{
    /// [`Drop::drop`] that attempts a write if modified, and panics if that
    /// write returns and error.
    fn drop(&mut self) {
        if self.modified && self.flush().is_err() {
            panic!("BackedEntryMut dropped while modified, and failed to flush.");
        }
    }
}

impl<T, Disk, Coder> BackedEntryMutHandle for BackedEntrySync<T, Disk, Coder>
where
    T: Once<Inner: Serialize + for<'de> Deserialize<'de>>,
    Disk: WriteDisk + ReadDisk,
    Coder: Encoder<Disk::WriteDisk, T = T::Inner> + Decoder<Disk::ReadDisk, T = T::Inner>,
{
    type MutHandleResult<'a> = Result<BackedEntryMut<'a, T, Disk, Coder>, <Coder as Decoder<Disk::ReadDisk>>::Error>
        where
            Self: 'a;

    fn mut_handle(&mut self) -> Self::MutHandleResult<'_> {
        self.load()?;
        Ok(BackedEntryMut {
            entry: self,
            modified: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, io::Cursor, sync::Arc, thread::scope};

    use crate::{
        entry::{BackedEntryArr, BackedEntryArrLock, BackedEntryCell},
        test_utils::cursor_vec,
        test_utils::CursorVec,
    };

    #[cfg(feature = "bincode")]
    use crate::entry::formats::BincodeCoder;

    use super::*;

    #[cfg(feature = "bincode")]
    #[test]
    fn mutate() {
        const FIB: &[u8] = &[0, 1, 1, 5, 7];
        cursor_vec!(back_vec, backing_store);

        // Intentional unsafe access to later peek underlying storage
        let mut backed_entry =
            unsafe { BackedEntryArr::new(&mut *back_vec.get(), BincodeCoder::default()) };
        backed_entry.write_ref(&FIB.into()).unwrap();

        assert_eq!(backed_entry.load().unwrap().as_ref(), FIB);

        #[cfg(not(miri))]
        assert_eq!(&backing_store[backing_store.len() - FIB.len()..], FIB);

        let mut handle = backed_entry.mut_handle().unwrap();
        handle[0] = 20;
        handle[2] = 30;

        #[cfg(not(miri))]
        assert_eq!(backing_store[backing_store.len() - FIB.len()], FIB[0]);

        assert_eq!(handle[0], 20);
        assert_eq!(handle[2], 30);

        handle.flush().unwrap();

        #[cfg(not(miri))]
        {
            assert_eq!(backing_store[backing_store.len() - FIB.len()], 20);
            assert_eq!(backing_store[backing_store.len() - FIB.len() + 2], 30);
            assert_eq!(backing_store[backing_store.len() - FIB.len() + 1], FIB[1]);
        }

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
        let mut backed_entry = BackedEntryCell::new(&mut back_vec, BincodeCoder::default());
        backed_entry.write_ref(&input).unwrap();

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

        cursor_vec!(back_vec, back_vec_inner);

        let mut backed_entry =
            BackedEntryArr::new(unsafe { &mut *back_vec.get() }, BincodeCoder::default());

        backed_entry.write_ref(&VALUE.into()).unwrap();
        assert!(!backed_entry.is_loaded());

        #[cfg(not(miri))]
        assert_eq!(&back_vec_inner[back_vec_inner.len() - VALUE.len()..], VALUE);

        assert_eq!(backed_entry.load().unwrap().as_ref(), VALUE);

        backed_entry.write(NEW_VALUE).unwrap();
        assert!(backed_entry.is_loaded());

        #[cfg(all(test, not(miri)))]
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
        const NEW_VALUES: &[u8] = &[17, 15, 13, 11, 10];

        let mut binding = Cursor::new(Vec::with_capacity(10));
        let mut back_vec = CursorVec {
            inner: (&mut binding).into(),
        };

        let mut backed_entry = BackedEntryArrLock::new(&mut back_vec, BincodeCoder::default());

        backed_entry.write_ref(&VALUES.into()).unwrap();
        assert!(!backed_entry.is_loaded());
        scope(|s| {
            let backed_share = Arc::new(&backed_entry);
            (0..VALUES.len()).for_each(|idx| {
                let backed_share = backed_share.clone();
                s.spawn(move || assert_eq!(backed_share.load().unwrap()[idx], VALUES[idx]));
            });
        });

        backed_entry.write(NEW_VALUES).unwrap();
        assert!(backed_entry.is_loaded());
        scope(|s| {
            let backed_share = Arc::new(&backed_entry);
            (0..NEW_VALUES.len()).for_each(|idx| {
                let backed_share = backed_share.clone();
                s.spawn(move || assert_eq!(backed_share.load().unwrap()[idx], NEW_VALUES[idx]));
            });
        });
    }
}
