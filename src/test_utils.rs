use core::panic;
use std::{
    io::{Cursor, Seek},
    marker::PhantomData,
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
    ($x: ident, $peek: ident) => {
        let mut inner = std::io::Cursor::default();
        let $x = std::cell::UnsafeCell::new($crate::test_utils::CursorVec::new(&mut inner));
        #[allow(unused_variables)]
        #[cfg(all(test, not(miri)))]
        let $peek = unsafe { &*$x.get() }.get_ref();
    };
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
    type ReadDisk = Cursor<Vec<u8>>;

    fn read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        let mut this = self.inner.lock().unwrap().clone();
        this.rewind()?;
        Ok(this)
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
    type ReadDisk = Cursor<Vec<u8>>;

    fn read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        let mut this = self.inner.lock().unwrap().clone();
        this.rewind()?;
        Ok(this)
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

#[derive(Serialize, Deserialize)]
pub struct OwnedCursorVec<'a> {
    #[serde(skip)]
    pub inner: Mutex<Cursor<Vec<u8>>>,
    _phantom: PhantomData<&'a ()>,
}

impl OwnedCursorVec<'_> {
    pub fn new(inner: Cursor<Vec<u8>>) -> Self {
        Self {
            inner: Mutex::new(inner),
            _phantom: PhantomData,
        }
    }
}

impl Default for OwnedCursorVec<'_> {
    fn default() -> Self {
        Self::new(Cursor::default())
    }
}

/// This is deliberately breaking mutability
impl ReadDisk for OwnedCursorVec<'_> {
    type ReadDisk = Cursor<Vec<u8>>;

    fn read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        let mut this = self.inner.lock().unwrap().clone();
        this.rewind()?;
        Ok(this)
    }
}

impl<'a> WriteDisk for OwnedCursorVec<'a> {
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
impl AsyncReadDisk for OwnedCursorVec<'_> {
    type ReadDisk = Cursor<Vec<u8>>;

    async fn async_read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        let mut this = self.inner.lock().unwrap().clone();
        this.rewind()?;
        Ok(this)
    }
}

#[cfg(feature = "async")]
impl<'a> AsyncWriteDisk for OwnedCursorVec<'a> {
    type WriteDisk = &'a mut Cursor<Vec<u8>>;

    async fn async_write_disk(&self) -> std::io::Result<Self::WriteDisk> {
        let mut this = self.inner.lock().unwrap();
        this.get_mut().clear();
        this.rewind()?;
        let this: *mut _ = this.deref_mut();
        unsafe { Ok(&mut *this) }
    }
}

pub mod csv_data {
    use std::sync::LazyLock;

    use serde::{Deserialize, Serialize};

    #[derive(Debug, Default, PartialEq, PartialOrd, Serialize, Deserialize)]
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
