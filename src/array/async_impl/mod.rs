use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::entry::{
    async_impl::OnceCellWrap,
    disks::{AsyncReadDisk, AsyncWriteDisk},
    formats::{AsyncDecoder, AsyncEncoder},
    BackedEntryAsync,
};

use super::{
    container::{BackedEntryContainer, Container, ResizingContainer},
    BackedArray,
};

#[cfg(feature = "unsafe_array")]
pub mod generic;
pub mod slice;

/// A [`BackedEntryContainer`] inside a [`Container`].
///
/// For internal use, reduces size of generics boilerplate.
pub trait BackedEntryContainerNestedAsync:
    Container<
    Data: BackedEntryContainer<
        Container = Self::OnceWrapper,
        Disk = Self::Disk,
        Coder = Self::Coder,
    > + From<BackedEntryAsync<Self::Unwrapped, Self::Disk, Self::Coder>>
              + AsRef<BackedEntryAsync<Self::Unwrapped, Self::Disk, Self::Coder>>
              + AsMut<BackedEntryAsync<Self::Unwrapped, Self::Disk, Self::Coder>>,
>
{
    type InnerData;
    type OnceWrapper: OnceCellWrap<T = Self::Unwrapped>;
    type Unwrapped: Container<Data = Self::InnerData>;
    type Disk;
    type Coder;
}

/// Auto-implement trait to wrap intended generics.
impl<T> BackedEntryContainerNestedAsync for T
where
    T: Container<Data: BackedEntryContainer>,
    <<T as Container>::Data as BackedEntryContainer>::Container: OnceCellWrap<T: Container>,
    <T as Container>::Data: From<
        BackedEntryAsync<
            <<T::Data as BackedEntryContainer>::Container as OnceCellWrap>::T,
            <T::Data as BackedEntryContainer>::Disk,
            <T::Data as BackedEntryContainer>::Coder,
        >,
    >,
    <T as Container>::Data: AsRef<
        BackedEntryAsync<
            <<T::Data as BackedEntryContainer>::Container as OnceCellWrap>::T,
            <T::Data as BackedEntryContainer>::Disk,
            <T::Data as BackedEntryContainer>::Coder,
        >,
    >,
    <T as Container>::Data: AsMut<
        BackedEntryAsync<
            <<T::Data as BackedEntryContainer>::Container as OnceCellWrap>::T,
            <T::Data as BackedEntryContainer>::Disk,
            <T::Data as BackedEntryContainer>::Coder,
        >,
    >,
{
    type InnerData =
        <<<T::Data as BackedEntryContainer>::Container as OnceCellWrap>::T as Container>::Data;
    type OnceWrapper = <T::Data as BackedEntryContainer>::Container;
    type Unwrapped = <<T::Data as BackedEntryContainer>::Container as OnceCellWrap>::T;
    type Disk = <T::Data as BackedEntryContainer>::Disk;
    type Coder = <T::Data as BackedEntryContainer>::Coder;
}

/// [`BackedEntryContainerNested`] variant.
///
/// For internal use, reduces size of generics boilerplate.
pub trait BackedEntryContainerNestedAsyncRead:
    BackedEntryContainerNestedAsync<
    Unwrapped: for<'de> Deserialize<'de> + Send + Sync,
    Disk: AsyncReadDisk,
    Coder: AsyncDecoder<<Self::Disk as AsyncReadDisk>::ReadDisk, Error = Self::AsyncReadError>,
>
{
    type AsyncReadError;
}

impl<T> BackedEntryContainerNestedAsyncRead for T
where
    T: BackedEntryContainerNestedAsync<
        Unwrapped: for<'de> Deserialize<'de> + Send + Sync,
        Disk: AsyncReadDisk,
        Coder: AsyncDecoder<<Self::Disk as AsyncReadDisk>::ReadDisk>,
    >,
{
    type AsyncReadError =
        <Self::Coder as AsyncDecoder<<Self::Disk as AsyncReadDisk>::ReadDisk>>::Error;
}

/// [`BackedEntryContainerNested`] variant.
///
/// For internal use, reduces size of generics boilerplate.
pub trait BackedEntryContainerNestedAsyncWrite:
    BackedEntryContainerNestedAsync<
    Unwrapped: Serialize + Send + Sync,
    Disk: AsyncWriteDisk,
    Coder: AsyncEncoder<<Self::Disk as AsyncWriteDisk>::WriteDisk, Error = Self::AsyncWriteError>,
>
{
    type AsyncWriteError;
}

impl<T> BackedEntryContainerNestedAsyncWrite for T
where
    T: BackedEntryContainerNestedAsync<
        Unwrapped: Serialize + Send + Sync,
        Disk: AsyncWriteDisk,
        Coder: AsyncEncoder<<Self::Disk as AsyncWriteDisk>::WriteDisk>,
    >,
{
    type AsyncWriteError =
        <Self::Coder as AsyncEncoder<<Self::Disk as AsyncWriteDisk>::WriteDisk>>::Error;
}

/// [`BackedEntryContainerNested`] variant.
///
/// For internal use, reduces size of generics boilerplate.
pub trait BackedEntryContainerNestedAsyncAll:
    BackedEntryContainerNestedAsyncRead + BackedEntryContainerNestedAsyncWrite
{
}

impl<T> BackedEntryContainerNestedAsyncAll for T where
    T: BackedEntryContainerNestedAsyncRead + BackedEntryContainerNestedAsyncWrite
{
}

pub type AsyncVecBackedArray<T, Disk, Coder> =
    BackedArray<Vec<Range<usize>>, Vec<BackedEntryAsync<Box<[T]>, Disk, Coder>>>;

/// Write implementations
impl<
        K: ResizingContainer<Data = Range<usize>>,
        E: BackedEntryContainerNestedAsyncWrite + ResizingContainer,
    > BackedArray<K, E>
{
    /// Adds new values by writing them to the backing store.
    ///
    /// Does not keep the values in memory.
    ///
    /// # Example
    /// ```rust
    /// #[cfg(feature = "async_bincode")] {
    /// use backed_data::{
    ///     array::async_impl::AsyncVecBackedArray,
    ///     entry::{
    ///         disks::Plainfile,
    ///         formats::AsyncBincodeCoder,
    ///     }
    /// };
    /// use std::fs::{File, create_dir_all, remove_dir_all, OpenOptions};
    ///
    /// use tokio::runtime::Builder;
    ///
    /// let rt = Builder::new_current_thread().build().unwrap();
    ///
    /// let FILENAME_BASE = std::env::temp_dir().join("example_async_array_append");
    /// let values = ([0, 1, 1],
    ///     [2, 3, 5]);
    ///
    /// create_dir_all(FILENAME_BASE.clone()).unwrap();
    /// let file_0 = FILENAME_BASE.clone().join("_0");
    /// let file_1 = FILENAME_BASE.join("_1");
    /// let mut array: AsyncVecBackedArray<u32, Plainfile, _> = AsyncVecBackedArray::default();
    /// rt.block_on(array.a_append(values.0, file_0.into(), AsyncBincodeCoder {}));
    /// rt.block_on(array.a_append(values.1, file_1.into(), AsyncBincodeCoder {}));
    ///
    /// assert_eq!(*rt.block_on(array.a_get(4)).unwrap(), 3);
    /// remove_dir_all(FILENAME_BASE).unwrap();
    /// }
    /// ```
    pub async fn a_append<U: Into<E::Unwrapped>>(
        &mut self,
        values: U,
        backing_store: E::Disk,
        coder: E::Coder,
    ) -> Result<&mut Self, E::AsyncWriteError> {
        let values = values.into();

        // End of a range is exclusive
        let start_idx = self
            .keys
            .c_ref()
            .as_ref()
            .last()
            .map(|key_range| key_range.end)
            .unwrap_or(0);
        self.keys.c_push(start_idx..(start_idx + values.c_len()));

        let mut entry = BackedEntryAsync::new(backing_store, coder);
        entry.a_write_unload(values).await?;
        self.entries.c_push(entry.into());
        Ok(self)
    }

    /// [`Self::append`], but keeps values in memory.
    ///
    /// # Example
    /// ```rust
    /// #[cfg(feature = "async_bincode")] {
    ///     use backed_data::{
    ///         array::async_impl::AsyncVecBackedArray,
    ///         entry::{
    ///             disks::Plainfile,
    ///             formats::AsyncBincodeCoder,
    ///         },
    ///     };
    ///     use std::fs::{File, remove_file, OpenOptions};
    ///
    ///     use tokio::runtime::Builder;
    ///
    ///     let rt = Builder::new_current_thread().build().unwrap();
    ///
    ///     let FILENAME = std::env::temp_dir().join("example_async_array_append_memory");
    ///     let values = ([0, 1, 1],
    ///         [2, 3, 5]);
    ///
    ///     let mut array: AsyncVecBackedArray<u32, Plainfile, _> = AsyncVecBackedArray::default();
    ///     rt.block_on(array.a_append_memory(values.0, FILENAME.clone().into(), AsyncBincodeCoder {}));
    ///
    ///     // Overwrite file, making disk pointer for first array invalid
    ///     rt.block_on(array.a_append_memory(values.1, FILENAME.clone().into(), AsyncBincodeCoder {}));
    ///
    ///     assert_eq!(*rt.block_on(array.a_get(0)).unwrap(), 0);
    ///     remove_file(FILENAME).unwrap();
    /// }
    /// ```
    pub async fn a_append_memory<U: Into<E::Unwrapped>>(
        &mut self,
        values: U,
        backing_store: E::Disk,
        coder: E::Coder,
    ) -> Result<&mut Self, E::AsyncWriteError> {
        let values = values.into();

        // End of a range is exclusive
        let start_idx = self
            .keys
            .c_ref()
            .as_ref()
            .last()
            .map(|key_range| key_range.end)
            .unwrap_or(0);
        self.keys.c_push(start_idx..(start_idx + values.c_len()));
        let mut entry = BackedEntryAsync::new(backing_store, coder);
        entry.a_write(values).await?;
        self.entries.c_push(entry.into());
        Ok(self)
    }
}
