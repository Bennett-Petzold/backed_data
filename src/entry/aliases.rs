use std::{cell::OnceCell, sync::OnceLock};

#[cfg(feature = "async")]
use crate::utils::sync::AsyncOnceLock;

use super::{BackedEntryInner, BackedEntrySync};

/// Thread unsafe [`BackedEntrySync`] with inner value `T`.
pub type BackedEntryCell<T, Disk, Coder> = BackedEntrySync<OnceCell<T>, Disk, Coder>;
/// Thread safe [`BackedEntrySync`] with inner value `T`.
pub type BackedEntryLock<T, Disk, Coder> = BackedEntrySync<OnceLock<T>, Disk, Coder>;

/// Thread-unsafe typedef of [`BackedEntrySync`] for non-pointer types.
///
/// # Example
///
/// ```rust
/// #[cfg(feature = "bincode")] {
///     use std::fs::{File, remove_file};
///     use backed_data::entry::{
///         BackedEntry,
///         BackedEntryRead,
///         BackedEntryWrite,
///         BackedEntryBox,
///         formats::BincodeCoder,
///         disks::Plainfile,
///     };
///
///     let FILENAME = std::env::temp_dir().join("example_box");
///     let file = Plainfile::new(FILENAME.clone());
///
///     // Write string to file
///     let mut writer: BackedEntryBox<str, _, BincodeCoder<Box<str>>> =
///         BackedEntryBox::new(file.clone(), BincodeCoder::default());
///     writer.write_ref(&"HELLO I AM A STRING".into()).unwrap();
///     drop(writer);
///
///     // Read string from file
///     let mut sparse: BackedEntryBox<str, _, BincodeCoder<_>> =
///         BackedEntryBox::new(file.clone(), BincodeCoder::default());
///     assert_eq!(sparse.load().unwrap().as_ref(), "HELLO I AM A STRING");
///
///     // Cleanup
///     remove_file(FILENAME).unwrap();
/// }
/// ```
pub type BackedEntryBox<T, Disk, Coder> = BackedEntryCell<Box<T>, Disk, Coder>;

/// Thread-safe typedef of [`BackedEntrySync`] for non-pointer types.
///
/// # Example
///
/// ```rust
/// #[cfg(feature = "bincode")] {
///     use std::fs::{File, remove_file};
///     use backed_data::entry::{
///         BackedEntry,
///         BackedEntryRead,
///         BackedEntryWrite,
///         BackedEntryBoxLock,
///         formats::BincodeCoder,
///         disks::Plainfile,
///     };
///
///     let FILENAME = std::env::temp_dir().join("example_box_safe");
///     let file = Plainfile::new(FILENAME.clone());
///
///     // Write string to file
///     let mut writer: BackedEntryBoxLock<str, _, BincodeCoder<Box<str>>> =
///         BackedEntryBoxLock::new(file.clone(), BincodeCoder::default());
///     writer.write_ref(&"HELLO I AM A STRING".into()).unwrap();
///     drop(writer);
///
///     // Read string from file
///     let mut sparse: BackedEntryBoxLock<str, _, BincodeCoder<_>> =
///         BackedEntryBoxLock::new(file.clone(), BincodeCoder::default());
///     assert_eq!(sparse.load().unwrap().as_ref(), "HELLO I AM A STRING");
///
///     // Cleanup
///     remove_file(FILENAME).unwrap();
/// }
/// ```
pub type BackedEntryBoxLock<T, Disk, Coder> = BackedEntryLock<Box<T>, Disk, Coder>;

/// Thread-unsafe typedef of [`BackedEntrySync`] for arrays.
///
/// # Example
///
/// ```rust
/// #[cfg(feature = "bincode")] {
///     use std::fs::{File, remove_file};
///     use backed_data::entry::{
///         BackedEntry,
///         BackedEntryRead,
///         BackedEntryWrite,
///         BackedEntryArr,
///         formats::BincodeCoder,
///         disks::Plainfile,
///     };
///    
///     let FILENAME = std::env::temp_dir().join("example_array");
///     let file = Plainfile::new(FILENAME.clone());
///    
///     // Write array to file
///     let mut writer: BackedEntryArr<u8, _, BincodeCoder<_>> = BackedEntryArr::new(file.clone(),
///         BincodeCoder::default());
///     writer.write_ref(&[1, 2, 3].into()).unwrap();
///     drop(writer);
///    
///     // Read array from file
///     let mut sparse: BackedEntryArr<u8, _, _> = BackedEntryArr::new(file.clone(),
///         BincodeCoder::default());
///     assert_eq!(sparse.load().unwrap().as_ref(), [1, 2, 3]);
///    
///     // Cleanup
///     remove_file(FILENAME).unwrap();
/// }
/// ```
pub type BackedEntryArr<T, Disk, Coder> = BackedEntryBox<[T], Disk, Coder>;

/// Thread-safe typedef of [`BackedEntrySync`] for arrays.
///
/// # Example
///
/// ```rust
/// #[cfg(feature = "bincode")] {
///     use std::fs::{File, remove_file};
///     use backed_data::entry::{
///         BackedEntry,
///         BackedEntryRead,
///         BackedEntryWrite,
///         BackedEntryArrLock,
///         formats::BincodeCoder,
///         disks::Plainfile,
///     };
///    
///     let FILENAME = std::env::temp_dir().join("example_array_safe");
///     let file = Plainfile::new(FILENAME.clone());
///    
///     // Write array to file
///     let mut writer: BackedEntryArrLock<u8, _, BincodeCoder<_>> = BackedEntryArrLock::new(file.clone(),
///         BincodeCoder::default());
///     writer.write_ref(&[1, 2, 3].into()).unwrap();
///     drop(writer);
///    
///     // Read array from file
///     let mut sparse: BackedEntryArrLock<u8, _, _> = BackedEntryArrLock::new(file.clone(),
///         BincodeCoder::default());
///     assert_eq!(sparse.load().unwrap().as_ref(), [1, 2, 3]);
///    
///     // Cleanup
///     remove_file(FILENAME).unwrap();
/// }
/// ```
pub type BackedEntryArrLock<T, Disk, Coder> = BackedEntryBoxLock<[T], Disk, Coder>;
