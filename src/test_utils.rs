use core::panic;
use std::{
    io::{Cursor, Seek},
    ops::{Deref, DerefMut},
    sync::Mutex,
};

use serde::{Deserialize, Serialize};

#[cfg(feature = "async")]
use crate::entry::disks::{AsyncReadDisk, AsyncWriteDisk};

use crate::entry::disks::{ReadDisk, WriteDisk};

#[derive(Debug, Serialize)]
pub struct CursorVec<'a> {
    #[serde(skip)]
    pub inner: Mutex<&'a mut Cursor<Vec<u8>>>,
}

impl<'a> CursorVec<'a> {
    pub fn new(inner: &'a mut Cursor<Vec<u8>>) -> Self {
        Self {
            inner: Mutex::new(inner),
        }
    }
}

/// Creates a default [`CursorVec`] for testing.
#[macro_export]
macro_rules! cursor_vec {
    ($x: ident) => {
        let mut inner = std::io::Cursor::default();
        let $x = $crate::test_utils::CursorVec::new(&mut inner);
    };
}
pub use cursor_vec;

impl<'de> Deserialize<'de> for CursorVec<'_> {
    fn deserialize<D>(_deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        panic!("Don't deserialize CursorVec!")
    }
}

impl<'de> Deserialize<'de> for &mut CursorVec<'_> {
    fn deserialize<D>(_deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        panic!("Don't deserialize CursorVec!")
    }
}

/// This is deliberately breaking mutability
impl<'a> ReadDisk for CursorVec<'a> {
    type ReadDisk = &'a mut Cursor<Vec<u8>>;

    fn read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        self.inner.lock().unwrap().rewind()?;
        let this: *mut _ = self.inner.lock().unwrap().deref_mut();
        Ok(unsafe { &mut *this })
    }
}

impl<'a> WriteDisk for CursorVec<'a> {
    type WriteDisk = &'a mut Cursor<Vec<u8>>;

    fn write_disk(&self) -> std::io::Result<Self::WriteDisk> {
        let mut this = self.inner.lock().unwrap();
        this.get_mut().clear();
        this.rewind()?;
        let this: *mut _ = this.deref_mut();
        unsafe { Ok(&mut *this) }
    }
}

#[cfg(feature = "async")]
impl<'a> AsyncReadDisk for CursorVec<'a> {
    type ReadDisk = &'a mut Cursor<Vec<u8>>;

    async fn async_read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        self.inner.lock().unwrap().rewind()?;
        let this: *mut _ = self.inner.lock().unwrap().deref_mut();
        Ok(unsafe { &mut *this })
    }
}

#[cfg(feature = "async")]
impl<'a> AsyncWriteDisk for CursorVec<'a> {
    type WriteDisk = &'a mut Cursor<Vec<u8>>;

    async fn async_write_disk(&self) -> std::io::Result<Self::WriteDisk> {
        let mut this = self.inner.lock().unwrap();
        this.get_mut().clear();
        this.rewind()?;
        let this: *mut _ = this.deref_mut();
        unsafe { Ok(&mut *this) }
    }
}

impl<'a> ReadDisk for &'a mut CursorVec<'a> {
    type ReadDisk = &'a mut Cursor<Vec<u8>>;

    fn read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        self.inner.lock().unwrap().rewind()?;
        let this: *mut _ = self.inner.lock().unwrap().deref_mut();
        Ok(unsafe { &mut *this })
    }
}

impl<'a> WriteDisk for &'a mut CursorVec<'a> {
    type WriteDisk = &'a mut Cursor<Vec<u8>>;

    fn write_disk(&self) -> std::io::Result<Self::WriteDisk> {
        let mut this = self.inner.lock().unwrap();
        this.get_mut().clear();
        this.rewind()?;
        let this: *mut _ = this.deref_mut();
        unsafe { Ok(&mut *this) }
    }
}

#[cfg(feature = "async")]
impl<'a> AsyncReadDisk for &mut CursorVec<'a> {
    type ReadDisk = &'a mut Cursor<Vec<u8>>;

    async fn async_read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        self.inner.lock().unwrap().rewind()?;
        let this: *mut _ = self.inner.lock().unwrap().deref_mut();
        Ok(unsafe { &mut *this })
    }
}

#[cfg(feature = "async")]
impl<'a> AsyncWriteDisk for &mut CursorVec<'a> {
    type WriteDisk = &'a mut Cursor<Vec<u8>>;

    async fn async_write_disk(&self) -> std::io::Result<Self::WriteDisk> {
        let mut this = self.inner.lock().unwrap();
        this.get_mut().clear();
        this.rewind()?;
        let this: *mut _ = this.deref_mut();
        unsafe { Ok(&mut *this) }
    }
}

impl Unpin for CursorVec<'_> {}

impl Deref for CursorVec<'_> {
    type Target = Cursor<Vec<u8>>;
    fn deref(&self) -> &Self::Target {
        let this: *const _ = self.inner.lock().unwrap().deref();
        unsafe { *this }
    }
}

impl DerefMut for CursorVec<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        let this: *mut _ = self.inner.lock().unwrap().deref_mut();
        unsafe { *this }
    }
}
