//! Convenience `Disk` define structs

use std::io::{Read, Write};

use super::{ReadDisk, WriteDisk};

#[cfg(feature = "async")]
use {
    super::{AsyncReadDisk, AsyncWriteDisk},
    futures::io::{AsyncRead, AsyncWrite},
    std::future::Future,
};

/// Convenience struct to define a custom [`ReadDisk`], from a function.
///
/// Takes a [`Fn`] with no arguments, producing the reader.
///
/// # Example
/// ```
/// use backed_data::entry::disks::{ReadDisk, custom::CustomRead};
///
/// let disk = CustomRead(|| Ok(std::io::empty()));
/// let reader: std::io::Empty = disk.read_disk().unwrap();
/// ```
pub struct CustomRead<F>(pub F);

/// Convenience struct to define a custom [`WriteDisk`], from a function.
///
/// Takes a [`Fn`] with no arguments, producing the writer.
///
/// # Example
/// ```
/// use backed_data::entry::disks::{WriteDisk, custom::CustomWrite};
///
/// let disk = CustomWrite(|| Ok(std::io::sink()));
/// let writer: std::io::Sink = disk.write_disk().unwrap();
/// ```
pub struct CustomWrite<F>(pub F);

/// Combines [`CustomRead`] and [`CustomWrite`] to form one full I/O disk.
///
/// # Example
/// ```
/// use backed_data::entry::disks::{
///     ReadDisk,
///     WriteDisk,
///     custom::{
///         CustomSync,
///         CustomRead,
///         CustomWrite,
///     },
/// };
///
/// let disk = CustomSync(
///     CustomRead(|| Ok(std::io::empty())),
///     CustomWrite(|| Ok(std::io::sink()))
/// );
///
/// let reader: std::io::Empty = disk.read_disk().unwrap();
/// let writer: std::io::Sink = disk.write_disk().unwrap();
/// ```
pub struct CustomSync<R, W>(pub CustomRead<R>, pub CustomWrite<W>);

impl<F, R> ReadDisk for CustomRead<F>
where
    F: Fn() -> std::io::Result<R>,
    R: Read,
{
    type ReadDisk = R;

    fn read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        (self.0)()
    }
}

impl<F, W> WriteDisk for CustomWrite<F>
where
    F: Fn() -> std::io::Result<W>,
    W: Write,
{
    type WriteDisk = W;

    fn write_disk(&self) -> std::io::Result<Self::WriteDisk> {
        (self.0)()
    }
}

impl<R, W, O> ReadDisk for CustomSync<R, W>
where
    R: Fn() -> std::io::Result<O>,
    O: Read,
{
    type ReadDisk = O;

    fn read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        self.0.read_disk()
    }
}

impl<R, W, O> WriteDisk for CustomSync<R, W>
where
    W: Fn() -> std::io::Result<O>,
    O: Write,
{
    type WriteDisk = O;

    fn write_disk(&self) -> std::io::Result<Self::WriteDisk> {
        self.1.write_disk()
    }
}

// -------------------- Async -------------------- //

/// Convenience struct to define a custom [`AsyncReadDisk`], from a function.
///
/// Takes a [`Fn`] with no arguments, producing the future for the reader.
///
/// # Example
/// ```
/// use std::future::{Future, ready};
/// use backed_data::entry::disks::{AsyncReadDisk, custom::CustomAsyncRead};
///
/// let disk = CustomAsyncRead(|| ready(Ok(smol::io::empty())));
/// let reader: &dyn Future<Output = std::io::Result<smol::io::Empty>> =
///     &disk.async_read_disk();
/// ```
#[cfg(feature = "async")]
pub struct CustomAsyncRead<F>(pub F);

/// Convenience struct to define a custom [`AsyncWriteDisk`], from a function.
///
/// Takes a [`Fn`] with no arguments, producing the writer.
///
/// # Example
/// ```
/// use std::future::{Future, ready};
/// use backed_data::entry::disks::{AsyncWriteDisk, custom::CustomAsyncWrite};
///
/// let disk = CustomAsyncWrite(|| ready(Ok(smol::io::sink())));
/// let writer: &dyn Future<Output = std::io::Result<smol::io::Sink>> =
///     &disk.async_write_disk();
/// ```
#[cfg(feature = "async")]
pub struct CustomAsyncWrite<F>(pub F);

/// Combines [`CustomAsyncRead`] and [`CustomAsyncWrite`] to form one full I/O disk.
///
/// # Example
/// ```
/// use std::future::{Future, ready};
/// use backed_data::entry::disks::{
///     AsyncReadDisk,
///     AsyncWriteDisk,
///     custom::{
///         CustomAsyncRead,
///         CustomAsyncWrite,
///         CustomAsync,
///     },
/// };
///
/// let disk = CustomAsync(
///     CustomAsyncRead(|| ready(Ok(smol::io::empty()))),
///     CustomAsyncWrite(|| ready(Ok(smol::io::sink())))
/// );
///
/// let reader: &dyn Future<Output = std::io::Result<smol::io::Empty>> =
///     &disk.async_read_disk();
/// let writer: &dyn Future<Output = std::io::Result<smol::io::Sink>> =
///     &disk.async_write_disk();
/// ```
#[cfg(feature = "async")]
pub struct CustomAsync<R, W>(pub CustomAsyncRead<R>, pub CustomAsyncWrite<W>);

#[cfg(feature = "async")]
impl<F, R, E> AsyncReadDisk for CustomAsyncRead<F>
where
    F: Fn() -> E + Unpin,
    E: Future<Output = std::io::Result<R>> + Send + Sync,
    R: AsyncRead + Unpin,
{
    type ReadDisk = R;

    fn async_read_disk(&self) -> impl Future<Output = std::io::Result<Self::ReadDisk>> {
        (self.0)()
    }
}

#[cfg(feature = "async")]
impl<F, W, E> AsyncWriteDisk for CustomAsyncWrite<F>
where
    F: Fn() -> E + Unpin,
    E: Future<Output = std::io::Result<W>> + Send + Sync,
    W: AsyncWrite + Unpin,
{
    type WriteDisk = W;

    fn async_write_disk(&self) -> impl Future<Output = std::io::Result<Self::WriteDisk>> {
        (self.0)()
    }
}

#[cfg(feature = "async")]
impl<R, W, E, O> AsyncReadDisk for CustomAsync<R, W>
where
    R: Fn() -> E + Unpin,
    W: Unpin,
    E: Future<Output = std::io::Result<O>> + Send + Sync,
    O: AsyncRead + Unpin,
{
    type ReadDisk = O;

    fn async_read_disk(&self) -> impl Future<Output = std::io::Result<Self::ReadDisk>> {
        self.0.async_read_disk()
    }
}

#[cfg(feature = "async")]
impl<R, W, E, O> AsyncWriteDisk for CustomAsync<R, W>
where
    R: Unpin,
    W: Fn() -> E + Unpin,
    E: Future<Output = std::io::Result<O>> + Send + Sync,
    O: AsyncWrite + Unpin,
{
    type WriteDisk = O;

    fn async_write_disk(&self) -> impl Future<Output = std::io::Result<Self::WriteDisk>> {
        self.1.async_write_disk()
    }
}

// -------------------- Combined -------------------- //

/// Combines [`CustomSync`] and [`CustomAsync`] to capture all possible I/O.
///
/// # Example
/// ```
/// use std::future::{Future, ready};
/// use backed_data::entry::disks::{
///     ReadDisk,
///     WriteDisk,
///     AsyncReadDisk,
///     AsyncWriteDisk,
///     custom::{
///         CustomSync,
///         CustomRead,
///         CustomWrite,
///         CustomAsyncRead,
///         CustomAsyncWrite,
///         CustomAsync,
///         CustomFull,
///     },
/// };
///
/// let disk = CustomFull(
///     CustomSync(
///         CustomRead(|| Ok(std::io::empty())),
///         CustomWrite(|| Ok(std::io::sink()))
///     ),
///     CustomAsync(
///         CustomAsyncRead(|| ready(Ok(smol::io::empty()))),
///         CustomAsyncWrite(|| ready(Ok(smol::io::sink())))
///     )
/// );
///
/// let sync_reader: std::io::Empty = disk.read_disk().unwrap();
/// let sync_writer: std::io::Sink = disk.write_disk().unwrap();
/// let async_reader: &dyn Future<Output = std::io::Result<smol::io::Empty>> =
///     &disk.async_read_disk();
/// let async_writer: &dyn Future<Output = std::io::Result<smol::io::Sink>> =
///     &disk.async_write_disk();
/// ```
#[cfg(feature = "async")]
pub struct CustomFull<R, W, AR, AW>(pub CustomSync<R, W>, pub CustomAsync<AR, AW>);

#[cfg(feature = "async")]
impl<R, W, AR, AW, O> ReadDisk for CustomFull<R, W, AR, AW>
where
    R: Fn() -> std::io::Result<O>,
    O: Read,
{
    type ReadDisk = O;

    fn read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        self.0.read_disk()
    }
}

#[cfg(feature = "async")]
impl<R, W, AR, AW, O> WriteDisk for CustomFull<R, W, AR, AW>
where
    W: Fn() -> std::io::Result<O>,
    O: Write,
{
    type WriteDisk = O;

    fn write_disk(&self) -> std::io::Result<Self::WriteDisk> {
        self.0.write_disk()
    }
}

#[cfg(feature = "async")]
impl<R, W, AR, AW, E, O> AsyncReadDisk for CustomFull<R, W, AR, AW>
where
    R: Unpin,
    W: Unpin,
    AR: Fn() -> E + Unpin,
    AW: Unpin,
    E: Future<Output = std::io::Result<O>> + Send + Sync,
    O: AsyncRead + Unpin,
{
    type ReadDisk = O;

    fn async_read_disk(&self) -> impl Future<Output = std::io::Result<Self::ReadDisk>> {
        self.1.async_read_disk()
    }
}

#[cfg(feature = "async")]
impl<R, W, AR, AW, E, O> AsyncWriteDisk for CustomFull<R, W, AR, AW>
where
    R: Unpin,
    W: Unpin,
    AR: Unpin,
    AW: Fn() -> E + Unpin,
    E: Future<Output = std::io::Result<O>> + Send + Sync,
    O: AsyncWrite + Unpin,
{
    type WriteDisk = O;

    fn async_write_disk(&self) -> impl Future<Output = std::io::Result<Self::WriteDisk>> {
        self.1.async_write_disk()
    }
}
