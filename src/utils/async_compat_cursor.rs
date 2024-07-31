use std::io::{Cursor, IoSliceMut, Read, Seek, SeekFrom, Write};

#[cfg(feature = "async")]
use {
    futures::{io::IoSlice, AsyncBufRead, AsyncRead, AsyncSeek, AsyncWrite},
    std::{
        pin::Pin,
        task::{Context, Poll},
    },
};

/// [`std::io::Cursor`] wrapper that provides async methods.
///
/// [`futures`] does not define a cursor for both async and sync read/write,
/// so we do it ourselves. [`tokio`] provides the async read/write methods on
/// [`std::io::Cursor`] directly, [`futures`] wraps [`std::io::Cursor`] and
/// does not implement regular read/write on it.
///
/// This is almost a complete direct copy of [`futures::io::Cursor`]
/// for the async methods, with `inner` swapped for `0`.
#[derive(Debug)]
pub struct AsyncCompatCursor<T>(pub Cursor<T>);

impl<T> From<Cursor<T>> for AsyncCompatCursor<T> {
    fn from(value: Cursor<T>) -> Self {
        Self(value)
    }
}

impl<T: AsRef<[u8]>> Seek for AsyncCompatCursor<T> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.0.seek(pos)
    }
}

impl<T: AsRef<[u8]>> Read for AsyncCompatCursor<T> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }

    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> std::io::Result<usize> {
        self.0.read_vectored(bufs)
    }
}

impl Write for AsyncCompatCursor<&mut [u8]> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }

    fn write_vectored(&mut self, bufs: &[std::io::IoSlice<'_>]) -> std::io::Result<usize> {
        self.0.write_vectored(bufs)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

impl Write for AsyncCompatCursor<&mut Vec<u8>> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }

    fn write_vectored(&mut self, bufs: &[std::io::IoSlice<'_>]) -> std::io::Result<usize> {
        self.0.write_vectored(bufs)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}
impl<const N: usize> Write for AsyncCompatCursor<[u8; N]> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }

    fn write_vectored(&mut self, bufs: &[std::io::IoSlice<'_>]) -> std::io::Result<usize> {
        self.0.write_vectored(bufs)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}
impl Write for AsyncCompatCursor<Box<[u8]>> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }

    fn write_vectored(&mut self, bufs: &[std::io::IoSlice<'_>]) -> std::io::Result<usize> {
        self.0.write_vectored(bufs)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}
impl Write for AsyncCompatCursor<Vec<u8>> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }

    fn write_vectored(&mut self, bufs: &[std::io::IoSlice<'_>]) -> std::io::Result<usize> {
        self.0.write_vectored(bufs)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

#[cfg(feature = "async")]
impl<T> AsyncSeek for AsyncCompatCursor<T>
where
    T: AsRef<[u8]> + Unpin,
{
    fn poll_seek(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        pos: SeekFrom,
    ) -> Poll<std::io::Result<u64>> {
        Poll::Ready(std::io::Seek::seek(&mut self.0, pos))
    }
}

#[cfg(feature = "async")]
impl<T: AsRef<[u8]> + Unpin> AsyncRead for AsyncCompatCursor<T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        Poll::Ready(std::io::Read::read(&mut self.0, buf))
    }

    fn poll_read_vectored(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bufs: &mut [IoSliceMut<'_>],
    ) -> Poll<std::io::Result<usize>> {
        Poll::Ready(std::io::Read::read_vectored(&mut self.0, bufs))
    }
}

#[cfg(feature = "async")]
impl<T> AsyncBufRead for AsyncCompatCursor<T>
where
    T: AsRef<[u8]> + Unpin,
{
    fn poll_fill_buf(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<&[u8]>> {
        Poll::Ready(std::io::BufRead::fill_buf(&mut self.get_mut().0))
    }

    fn consume(mut self: Pin<&mut Self>, amt: usize) {
        std::io::BufRead::consume(&mut self.0, amt)
    }
}

#[cfg(feature = "async")]
macro_rules! delegate_async_write_to_stdio {
    () => {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<std::io::Result<usize>> {
            Poll::Ready(std::io::Write::write(&mut self.0, buf))
        }

        fn poll_write_vectored(
            mut self: Pin<&mut Self>,
            _: &mut Context<'_>,
            bufs: &[IoSlice<'_>],
        ) -> Poll<std::io::Result<usize>> {
            Poll::Ready(std::io::Write::write_vectored(&mut self.0, bufs))
        }

        fn poll_flush(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Poll::Ready(std::io::Write::flush(&mut self.0))
        }

        fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            self.poll_flush(cx)
        }
    };
}

#[cfg(feature = "async")]
impl AsyncWrite for AsyncCompatCursor<&mut [u8]> {
    delegate_async_write_to_stdio!();
}

#[cfg(feature = "async")]
impl AsyncWrite for AsyncCompatCursor<&mut Vec<u8>> {
    delegate_async_write_to_stdio!();
}

#[cfg(feature = "async")]
impl AsyncWrite for AsyncCompatCursor<Vec<u8>> {
    delegate_async_write_to_stdio!();
}

#[cfg(feature = "async")]
impl AsyncWrite for AsyncCompatCursor<Box<[u8]>> {
    delegate_async_write_to_stdio!();
}
