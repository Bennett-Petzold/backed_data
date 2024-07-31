use std::{
    io::{Cursor, Read, Seek, Write},
    pin::Pin,
};

use tokio::io::{AsyncRead, AsyncSeek, AsyncWrite};

use crate::entry::DiskOverwritable;

#[derive(Debug)]
pub struct CursorVec(pub Cursor<Vec<u8>>);

impl DiskOverwritable for CursorVec {}
impl DiskOverwritable for &mut CursorVec {}

impl Write for CursorVec {
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.get_mut().flush()
    }
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.get_mut().write(buf)
    }
}

impl Read for CursorVec {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }
}

impl Seek for CursorVec {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.0.seek(pos)
    }

    fn rewind(&mut self) -> std::io::Result<()> {
        self.0.rewind()
    }
}

impl Unpin for CursorVec {}

impl AsyncWrite for CursorVec {
    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        Pin::new(&mut (self.get_mut()).0.get_mut()).poll_shutdown(cx)
    }
    fn poll_write_vectored(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        bufs: &[std::io::IoSlice<'_>],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        Pin::new(&mut (self.get_mut()).0.get_mut()).poll_write_vectored(cx, bufs)
    }
    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        Pin::new(&mut (self.get_mut()).0.get_mut()).poll_flush(cx)
    }
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        Pin::new(&mut (self.get_mut()).0.get_mut()).poll_write(cx, buf)
    }
    fn is_write_vectored(&self) -> bool {
        #[allow(unstable_name_collisions)]
        self.0.get_ref().is_write_vectored()
    }
}

impl AsyncRead for CursorVec {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        Pin::new(&mut (self.get_mut()).0).poll_read(cx, buf)
    }
}

impl AsyncSeek for CursorVec {
    fn poll_complete(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<u64>> {
        Pin::new(&mut (self.get_mut()).0).poll_complete(cx)
    }
    fn start_seek(
        self: std::pin::Pin<&mut Self>,
        position: std::io::SeekFrom,
    ) -> std::io::Result<()> {
        Pin::new(&mut (self.get_mut()).0).start_seek(position)
    }
}
