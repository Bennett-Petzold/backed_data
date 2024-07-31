use core::panic;
use std::{
    io::{Cursor, Seek},
    ops::{Deref, DerefMut},
};

use serde::{Deserialize, Serialize};

use crate::entry::{
    async_impl::{AsyncReadDisk, AsyncWriteDisk},
    sync_impl::{ReadDisk, WriteDisk},
    DiskOverwritable,
};

#[derive(Debug, Serialize)]
pub struct CursorVec<'a> {
    #[serde(skip)]
    pub inner: &'a mut Cursor<Vec<u8>>,
}

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

impl DiskOverwritable for CursorVec<'_> {}
impl DiskOverwritable for &mut CursorVec<'_> {}

impl<'a> ReadDisk for CursorVec<'a> {
    type ReadDisk = &'a mut Cursor<Vec<u8>>;

    fn read_disk(&mut self) -> std::io::Result<Self::ReadDisk> {
        self.inner.rewind()?;
        let self_inner: *mut Cursor<Vec<u8>> = self.inner;
        unsafe { Ok(&mut *self_inner) }
    }
}

impl<'a> WriteDisk for CursorVec<'a> {
    type WriteDisk = &'a mut Cursor<Vec<u8>>;

    fn write_disk(&mut self) -> std::io::Result<Self::WriteDisk> {
        self.inner.get_mut().clear();
        self.inner.rewind()?;
        let self_inner: *mut Cursor<Vec<u8>> = self.inner;
        unsafe { Ok(&mut *self_inner) }
    }
}

impl<'a> AsyncReadDisk for CursorVec<'a> {
    type ReadDisk = &'a mut Cursor<Vec<u8>>;

    async fn read_disk(&mut self) -> std::io::Result<Self::ReadDisk> {
        self.inner.rewind()?;
        let self_inner: *mut Cursor<Vec<u8>> = self.inner;
        unsafe { Ok(&mut *self_inner) }
    }
}

impl<'a> AsyncWriteDisk for CursorVec<'a> {
    type WriteDisk = &'a mut Cursor<Vec<u8>>;

    async fn write_disk(&mut self) -> std::io::Result<Self::WriteDisk> {
        self.inner.get_mut().clear();
        self.inner.rewind()?;
        let self_inner: *mut Cursor<Vec<u8>> = self.inner;
        unsafe { Ok(&mut *self_inner) }
    }
}

impl<'a> ReadDisk for &mut CursorVec<'a> {
    type ReadDisk = &'a mut Cursor<Vec<u8>>;

    fn read_disk(&mut self) -> std::io::Result<Self::ReadDisk> {
        self.inner.rewind()?;
        let self_inner: *mut Cursor<Vec<u8>> = self.inner;
        unsafe { Ok(&mut *self_inner) }
    }
}

impl<'a> WriteDisk for &mut CursorVec<'a> {
    type WriteDisk = &'a mut Cursor<Vec<u8>>;

    fn write_disk(&mut self) -> std::io::Result<Self::WriteDisk> {
        self.inner.get_mut().clear();
        self.inner.rewind()?;
        let self_inner: *mut Cursor<Vec<u8>> = self.inner;
        unsafe { Ok(&mut *self_inner) }
    }
}

impl<'a> AsyncReadDisk for &mut CursorVec<'a> {
    type ReadDisk = &'a mut Cursor<Vec<u8>>;

    async fn read_disk(&mut self) -> std::io::Result<Self::ReadDisk> {
        self.inner.rewind()?;
        let self_inner: *mut Cursor<Vec<u8>> = self.inner;
        unsafe { Ok(&mut *self_inner) }
    }
}

impl<'a> AsyncWriteDisk for &mut CursorVec<'a> {
    type WriteDisk = &'a mut Cursor<Vec<u8>>;

    async fn write_disk(&mut self) -> std::io::Result<Self::WriteDisk> {
        self.inner.get_mut().clear();
        self.inner.rewind()?;
        let self_inner: *mut Cursor<Vec<u8>> = self.inner;
        unsafe { Ok(&mut *self_inner) }
    }
}

impl Unpin for CursorVec<'_> {}

impl Deref for CursorVec<'_> {
    type Target = Cursor<Vec<u8>>;
    fn deref(&self) -> &Self::Target {
        self.inner.deref()
    }
}

impl DerefMut for CursorVec<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner.deref_mut()
    }
}
