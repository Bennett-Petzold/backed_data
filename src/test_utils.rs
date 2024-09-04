/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/*!
Defines tools used for ONLY testing.

This should not be used by any library consumer code. Some of this module is
deliberately undefined/unsafe for testing purposes.
*/

use core::panic;
use std::{
    cell::UnsafeCell,
    future::{ready, Ready},
    io::{Cursor, Seek, Write},
    marker::PhantomData,
    ops::{Deref, DerefMut},
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use futures::AsyncWrite;
use serde::{Deserialize, Serialize};

#[cfg(feature = "async")]
use crate::entry::disks::{AsyncReadDisk, AsyncWriteDisk};

use crate::{
    entry::disks::{ReadDisk, WriteDisk},
    utils::AsyncCompatCursor,
};

/// Unsafe storage disk that allows for memory peeking during modification.
///
/// FOR TESTING PURPOSES ONLY.
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
///
/// FOR TESTING PURPOSES ONLY.
#[macro_export]
macro_rules! cursor_vec {
    ($x: ident, $peek: ident) => {
        let $x = $crate::test_utils::StaticCursorVec::new(std::io::Cursor::default());
        #[allow(unused_variables)]
        #[cfg(all(test, not(miri)))]
        let $peek = unsafe { &*$x.inner.get() };
    };
    ($x: ident) => {
        let $x = $crate::test_utils::StaticCursorVec::new(std::io::Cursor::default());
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
    type ReadDisk<'r> = Cursor<Vec<u8>> where Self: 'r;

    fn read_disk(&self) -> std::io::Result<Self::ReadDisk<'_>> {
        let mut this = self.inner.lock().unwrap().clone();
        this.rewind()?;
        Ok(this)
    }
}

impl<'a> WriteDisk for CursorVec<'a> {
    type WriteDisk<'w> = &'w mut Cursor<Vec<u8>> where Self: 'w;

    fn write_disk(&mut self) -> std::io::Result<Self::WriteDisk<'_>> {
        let mut this = self.inner.lock().unwrap();
        this.get_mut().clear();
        this.rewind()?;
        let this: *mut _ = this.deref_mut();
        unsafe { Ok(&mut *this) }
    }
}

#[cfg(feature = "async")]
impl<'a> AsyncReadDisk for CursorVec<'a> {
    type ReadDisk<'r> = &'r [u8] where Self: 'r;
    type ReadFut<'f> = Ready<std::io::Result<Self::ReadDisk<'f>>> where Self: 'f;

    fn async_read_disk(&self) -> Self::ReadFut<'_> {
        let rewind_status = self.inner.lock().unwrap().rewind();
        match rewind_status {
            Ok(()) => {
                let this: *const _ = &**self.inner.lock().unwrap().get_ref();
                ready(Ok(unsafe { &*this }))
            }
            Err(e) => ready(Err(e)),
        }
    }
}

#[cfg(feature = "async")]
impl<'a> AsyncWriteDisk for CursorVec<'a> {
    type WriteDisk<'w> = &'a mut Vec<u8> where Self: 'w;
    type WriteFut<'f> = Ready<std::io::Result<Self::WriteDisk<'f>>> where Self: 'f;

    fn async_write_disk(&mut self) -> Self::WriteFut<'_> {
        let mut this = self.inner.lock().unwrap();
        this.get_mut().clear();
        match this.rewind() {
            Ok(()) => {
                let this: *mut _ = this.get_mut();
                ready(unsafe { Ok(&mut *this) })
            }
            Err(e) => ready(Err(e)),
        }
    }
}

impl<'a> ReadDisk for &'a mut CursorVec<'a> {
    type ReadDisk<'r> = Cursor<Vec<u8>> where Self: 'r;

    fn read_disk(&self) -> std::io::Result<Self::ReadDisk<'_>> {
        let mut this = self.inner.lock().unwrap().clone();
        this.rewind()?;
        Ok(this)
    }
}

#[cfg(feature = "async")]
impl<'a> AsyncReadDisk for &mut CursorVec<'a> {
    type ReadDisk<'r> = &'r [u8] where Self: 'r;
    type ReadFut<'f> = Ready<std::io::Result<Self::ReadDisk<'f>>> where Self: 'f;

    fn async_read_disk(&self) -> Self::ReadFut<'_> {
        let rewind_status = self.inner.lock().unwrap().rewind();
        match rewind_status {
            Ok(()) => {
                let this: *const _ = &**self.inner.lock().unwrap().get_ref();
                ready(Ok(unsafe { &*this }))
            }
            Err(e) => ready(Err(e)),
        }
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

#[derive(Serialize, Deserialize)]
pub struct OwnedCursorVec {
    #[serde(skip)]
    pub inner: Mutex<Cursor<Vec<u8>>>,
}

impl Clone for OwnedCursorVec {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.lock().unwrap().clone().into(),
        }
    }
}

impl OwnedCursorVec {
    pub fn new(inner: Cursor<Vec<u8>>) -> Self {
        Self {
            inner: Mutex::new(inner),
        }
    }
}

impl Default for OwnedCursorVec {
    fn default() -> Self {
        Self::new(Cursor::default())
    }
}

/// This is deliberately breaking mutability
impl ReadDisk for OwnedCursorVec {
    type ReadDisk<'r> = Cursor<Vec<u8>> where Self: 'r;

    fn read_disk(&self) -> std::io::Result<Self::ReadDisk<'_>> {
        let mut this = self.inner.lock().unwrap().clone();
        this.rewind()?;
        Ok(this)
    }
}

impl WriteDisk for OwnedCursorVec {
    type WriteDisk<'w> = &'w mut Cursor<Vec<u8>> where Self: 'w;

    fn write_disk(&mut self) -> std::io::Result<Self::WriteDisk<'_>> {
        let mut this = self.inner.lock().unwrap();
        this.get_mut().clear();
        this.rewind()?;
        let this: *mut _ = this.deref_mut();
        unsafe { Ok(&mut *this) }
    }
}

#[cfg(feature = "async")]
impl AsyncReadDisk for OwnedCursorVec {
    type ReadDisk<'r> = &'r [u8] where Self: 'r;
    type ReadFut<'f> = Ready<std::io::Result<Self::ReadDisk<'f>>> where Self: 'f;

    fn async_read_disk(&self) -> Self::ReadFut<'_> {
        let rewind_status = self.inner.lock().unwrap().rewind();
        match rewind_status {
            Ok(()) => {
                let this: *const _ = &**self.inner.lock().unwrap().get_ref();
                ready(Ok(unsafe { &*this }))
            }
            Err(e) => ready(Err(e)),
        }
    }
}

#[cfg(feature = "async")]
impl AsyncWriteDisk for OwnedCursorVec {
    type WriteDisk<'w> = &'w mut Vec<u8> where Self: 'w;
    type WriteFut<'f> = Ready<std::io::Result<Self::WriteDisk<'f>>> where Self: 'f;

    fn async_write_disk(&mut self) -> Self::WriteFut<'_> {
        let mut this = self.inner.lock().unwrap();
        this.get_mut().clear();
        match this.rewind() {
            Ok(()) => {
                let this: *mut _ = this.get_mut();
                ready(unsafe { Ok(&mut *this) })
            }
            Err(e) => ready(Err(e)),
        }
    }
}

#[derive(Debug)]
pub struct CursorVecWrite(Arc<UnsafeCell<AsyncCompatCursor<Vec<u8>>>>);

unsafe impl Sync for CursorVecWrite {}
unsafe impl Send for CursorVecWrite {}

impl From<Arc<UnsafeCell<AsyncCompatCursor<Vec<u8>>>>> for CursorVecWrite {
    fn from(value: Arc<UnsafeCell<AsyncCompatCursor<Vec<u8>>>>) -> Self {
        Self(value)
    }
}

impl From<CursorVecWrite> for Arc<UnsafeCell<AsyncCompatCursor<Vec<u8>>>> {
    fn from(value: CursorVecWrite) -> Self {
        value.0
    }
}

impl Write for CursorVecWrite {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let this = unsafe { &mut *self.0.get() };
        this.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let this = unsafe { &mut *self.0.get() };
        this.flush()
    }
}

impl AsyncWrite for CursorVecWrite {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = unsafe { &mut *self.0.get() };
        Pin::new(this).poll_write(cx, buf)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = unsafe { &mut *self.0.get() };
        Pin::new(this).poll_close(cx)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = unsafe { &mut *self.0.get() };
        Pin::new(this).poll_flush(cx)
    }
}

#[derive(Debug)]
pub struct StaticCursorVec {
    pub inner: Arc<UnsafeCell<AsyncCompatCursor<Vec<u8>>>>,
}

unsafe impl Sync for StaticCursorVec {}
unsafe impl Send for StaticCursorVec {}

impl Clone for StaticCursorVec {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl StaticCursorVec {
    pub fn new(inner: Cursor<Vec<u8>>) -> Self {
        Self {
            inner: Arc::new(AsyncCompatCursor::from(inner).into()),
        }
    }
}

impl Default for StaticCursorVec {
    fn default() -> Self {
        Self::new(Cursor::default())
    }
}

impl ReadDisk for StaticCursorVec {
    type ReadDisk<'r> = AsyncCompatCursor<Vec<u8>> where Self: 'r;

    fn read_disk(&self) -> std::io::Result<Self::ReadDisk<'_>> {
        let mut this = unsafe { &*self.inner.get() }.clone();
        this.rewind()?;
        Ok(this)
    }
}

impl WriteDisk for StaticCursorVec {
    type WriteDisk<'w> = CursorVecWrite where Self: 'w;

    fn write_disk(&mut self) -> std::io::Result<Self::WriteDisk<'_>> {
        unsafe { &mut *self.inner.get() }.clear();
        Ok(self.inner.clone().into())
    }
}

#[cfg(feature = "async")]
impl AsyncReadDisk for StaticCursorVec {
    type ReadDisk<'r> = AsyncCompatCursor<Vec<u8>> where Self: 'r;
    type ReadFut<'f> = Ready<std::io::Result<Self::ReadDisk<'f>>> where Self: 'f;

    fn async_read_disk(&self) -> Self::ReadFut<'_> {
        let mut this = unsafe { &*self.inner.get() }.clone();
        this.rewind().unwrap();
        ready(Ok(this.into()))
    }
}

#[cfg(feature = "async")]
impl AsyncWriteDisk for StaticCursorVec {
    type WriteDisk<'w> = CursorVecWrite where Self: 'w;
    type WriteFut<'f> = Ready<std::io::Result<Self::WriteDisk<'f>>> where Self: 'f;

    fn async_write_disk(&mut self) -> Self::WriteFut<'_> {
        unsafe { &mut *self.inner.get() }.clear();
        ready(Ok(self.inner.clone().into()))
    }
}

pub mod csv_data {
    use std::sync::LazyLock;

    use serde::{Deserialize, Serialize};

    #[derive(Debug, Default, PartialEq, PartialOrd, Clone, Serialize, Deserialize)]
    pub struct IouZipcodes {
        zip: u32,
        eiaid: u32,
        utility_name: String,
        state: String,
        service_type: String,
        ownership: String,
        comm_rate: f64,
        ind_rate: f64,
        res_rate: f64,
    }

    pub static FIRST_ENTRY: LazyLock<IouZipcodes> = LazyLock::new(|| IouZipcodes {
        zip: 85321,
        eiaid: 176,
        utility_name: "Ajo Improvement Co".to_string(),
        state: "AZ".to_string(),
        service_type: "Bundled".to_string(),
        ownership: "Investor Owned".to_string(),
        comm_rate: 0.08789049919484701,
        ind_rate: 0.0,
        res_rate: 0.09388714733542321,
    });

    pub static LAST_ENTRY: LazyLock<IouZipcodes> = LazyLock::new(|| IouZipcodes {
        zip: 96107,
        eiaid: 57483,
        utility_name: "Liberty Utilities".to_string(),
        state: "CA".to_string(),
        service_type: "Bundled".to_string(),
        ownership: "Investor Owned".to_string(),
        comm_rate: 0.14622411551676107,
        ind_rate: 0.0,
        res_rate: 0.14001899277463484,
    });
}
