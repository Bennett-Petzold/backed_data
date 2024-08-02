use std::{
    cmp::min,
    io::{Read, Write},
    pin::Pin,
    sync::{
        mpsc::{channel, Receiver, Sender},
        Arc, Condvar, Mutex,
    },
    task::{Context, Poll, Waker},
};

use futures::{AsyncBufRead, AsyncRead, AsyncWrite};
use serde::Serialize;

use crate::utils::blocking::BlockingFn;

use super::disks::{AsyncReadDisk, AsyncWriteDisk, ReadDisk, WriteDisk};

/// Adapts a synchronous disk to be asynchronous.
///
/// This serializes as `inner`.
///
/// The function handle is used to spawn `inner`'s read/write disk on another
/// thread. If the function handle runs on the current thread instead, this
/// will block indefinitely when used.
#[derive(Debug, Clone)]
pub struct SyncAsAsync<T, F> {
    inner: T,
    handle: F,
    read_buffer_size: usize,
    write_buffer_min_size: usize,
}

/// Default to 8 KiB, same as [`std::io::BufReader`].
const DEFAULT_READ_BUFFER_SIZE: usize = 8 * 1024;

/// Default to 8 KiB, same as [`std::io::BufReader`].
const DEFAULT_WRITE_BUFFER_SIZE: usize = 8 * 1024;

impl<T, F> SyncAsAsync<T, F> {
    /// Constructs a new async adapter for some [`ReadDisk`].
    ///
    /// # Parameters
    /// * `inner`: [`ReadDisk`] to wrap into [`AsyncReadDisk`].
    /// * `handle`: Handle for the background thread. MUST spawn onto another
    ///     thread, or any read/write calls will idle block indefinitely.
    /// * `read_buffer_size`: Size of heap allocation for `read` calls.
    ///     Sizing this below `read`'s max size will split reads into more
    ///     chunks, creating more heap allocations for thread transfers.
    ///     Sizing this above `read`'s max size will result in wasted
    ///     allocation space.
    /// * `write_buffer_min_size`: Min heap size before `write` calls are sent
    ///     to the background thread.
    pub fn new(
        inner: T,
        handle: F,
        read_buffer_size: Option<usize>,
        write_buffer_min_size: Option<usize>,
    ) -> Self {
        Self {
            inner,
            handle,
            read_buffer_size: read_buffer_size.unwrap_or(DEFAULT_READ_BUFFER_SIZE),
            write_buffer_min_size: write_buffer_min_size.unwrap_or(DEFAULT_WRITE_BUFFER_SIZE),
        }
    }
}

impl<T: Serialize, F> Serialize for SyncAsAsync<T, F> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.inner.serialize(serializer)
    }
}

/// Runs a sync [`Read`] on a background thread for nonblocking [`AsyncRead`].
#[derive(Debug)]
pub struct SyncAsAsyncRead {
    pending: bool,
    // Provide a waker and notify with condvar to trigger a new read.
    waker: Arc<(Condvar, std::sync::Mutex<Option<ReadWaker>>)>,
    #[allow(clippy::type_complexity)]
    read: Arc<std::sync::Mutex<Option<std::io::Result<Vec<u8>>>>>,
    buffered: Vec<u8>,
}

#[derive(Debug)]
enum ReadWaker {
    Waker(Waker),
    Kill,
}

/// [`BlockingFn`] that spawns a background reader.
#[derive(Debug)]
pub struct SyncAsAsyncReadBg<R> {
    reader: R,
    buffer_size: usize,
    waker: Arc<(Condvar, std::sync::Mutex<Option<ReadWaker>>)>,
    #[allow(clippy::type_complexity)]
    read: Arc<std::sync::Mutex<Option<std::io::Result<Vec<u8>>>>>,
}

impl<R: Read> BlockingFn for SyncAsAsyncReadBg<R> {
    type Output = ();
    fn call(mut self) -> Self::Output {
        loop {
            let waker = self
                .waker
                .0
                .wait(self.waker.1.lock().unwrap())
                .unwrap()
                .take();

            // If `waker` is `None`, this was a spurious wakeup.
            // See [`std::sync::Condvar::wait`].
            match waker {
                // Stop looping if the parent was dropped
                Some(ReadWaker::Kill) => {
                    return;
                }
                Some(ReadWaker::Waker(waker)) => {
                    let mut read_buffer = vec![0; self.buffer_size];

                    let res = self.reader.read(&mut read_buffer);
                    let mut read_send = self.read.lock().unwrap();

                    match res {
                        Ok(x) => {
                            *read_send = {
                                read_buffer.truncate(x);
                                Some(Ok(read_buffer))
                            }
                        }
                        Err(e) => *read_send = Some(Err(e)),
                    }
                    waker.wake();
                }
                None => (),
            }
        }
    }
}

impl SyncAsAsyncRead {
    fn new<R, F>(disk: &R, handle: &F, buffer_size: usize) -> std::io::Result<Self>
    where
        R: ReadDisk,
        F: Fn(SyncAsAsyncReadBg<R::ReadDisk>),
    {
        let waker = Arc::new((Condvar::new(), std::sync::Mutex::default()));
        let read = Arc::new(std::sync::Mutex::default());

        (handle)(SyncAsAsyncReadBg {
            reader: disk.read_disk()?,
            waker: waker.clone(),
            read: read.clone(),
            buffer_size,
        });

        let buffered: Box<[_]> = Box::new([]);
        let buffered = buffered.into_vec();

        Ok(Self {
            pending: false,
            waker,
            read,
            buffered,
        })
    }
}

impl AsyncBufRead for SyncAsAsyncRead {
    fn poll_fill_buf(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<&[u8]>> {
        if self.pending {
            // A bit of borrow checker hacking, since we can't assign to
            // `self.buffered` while `self.read` is borrowed.
            #[allow(unused_assignments)]
            let mut temp_buffer = Vec::with_capacity(0);

            {
                let mut read = self.read.lock().unwrap();
                match read.take() {
                    Some(Ok(x)) => {
                        temp_buffer = x;
                    }
                    Some(Err(e)) => {
                        return Poll::Ready(Err(e));
                    }
                    None => {
                        // Replace with the newest waker.
                        // Since the read lock is being held, and the
                        // processing thread writes to the read lock before
                        // updating the waker, the `wake()` call will not race.
                        *self.waker.1.lock().unwrap() = Some(ReadWaker::Waker(cx.waker().clone()));
                        return Poll::Pending;
                    }
                };
            }

            self.buffered = temp_buffer;
            self.pending = false;
        } else if self.buffered.is_empty() {
            self.pending = true;
            *self.waker.1.lock().unwrap() = Some(ReadWaker::Waker(cx.waker().clone()));
            self.waker.0.notify_one();
            return Poll::Pending;
        };

        Poll::Ready(Ok(&self.into_ref().get_ref().buffered))
    }

    fn consume(mut self: Pin<&mut Self>, amt: usize) {
        self.buffered.drain(..amt);
    }
}

impl AsyncRead for SyncAsAsyncRead {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.as_mut().poll_fill_buf(cx) {
            // Underlying future already handled the waker.
            Poll::Pending => Poll::Pending,
            Poll::Ready(x) => {
                let x = x?;
                let read_len = min(buf.len(), x.len());

                buf[..read_len].copy_from_slice(&x[..read_len]);
                self.consume(read_len);
                Poll::Ready(Ok(read_len))
            }
        }
    }
}

impl Drop for SyncAsAsyncRead {
    /// Signals background thread to die on drop.
    fn drop(&mut self) {
        *self.waker.1.lock().unwrap() = Some(ReadWaker::Kill);
        self.waker.0.notify_all();
    }
}

impl<R, F> ReadDisk for SyncAsAsync<R, F>
where
    R: ReadDisk,
{
    type ReadDisk = R::ReadDisk;

    fn read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        self.inner.read_disk()
    }
}

impl<R, F> AsyncReadDisk for SyncAsAsync<R, F>
where
    R: ReadDisk + Unpin + Send + Sync,
    F: Fn(SyncAsAsyncReadBg<R::ReadDisk>) + Unpin + Send + Sync,
{
    type ReadDisk = SyncAsAsyncRead;

    async fn async_read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        SyncAsAsyncRead::new(&self.inner, &self.handle, self.read_buffer_size)
    }
}

#[derive(Debug)]
enum AsyncWriteCommand {
    Write(Box<[u8]>),
    Flush,
}

#[derive(Debug)]
struct WriteError {
    error: std::io::Error,
    retry: Option<Arc<Condvar>>,
}

#[derive(Debug)]
struct Retry {
    enabled: bool,
    alive: bool,
}

impl Default for Retry {
    fn default() -> Self {
        Self {
            enabled: false,
            alive: true,
        }
    }
}

#[derive(Debug)]
pub struct SyncAsAsyncWrite {
    cmd_tx: Sender<AsyncWriteCommand>,
    status_rx: Receiver<Result<(), WriteError>>,
    write_buffer: Vec<u8>,
    buffer_min_size: usize,
    num_cmds_pending: usize,
    retry: Option<Arc<Condvar>>,
    retry_toggle: Arc<Mutex<Retry>>,
    flush_waker: Arc<Mutex<Option<Waker>>>,
}

/// [`BlockingFn`] that spawns a background writer.
#[derive(Debug)]
pub struct SyncAsAsyncWriteBg<W> {
    writer: W,
    cmd_rx: Receiver<AsyncWriteCommand>,
    status_tx: Sender<Result<(), WriteError>>,
    retry_toggle: Arc<std::sync::Mutex<Retry>>,
    flush_waker: Arc<Mutex<Option<Waker>>>,
}

impl<W: Write> BlockingFn for SyncAsAsyncWriteBg<W> {
    type Output = ();
    fn call(mut self) -> Self::Output {
        while let Ok(cmd) = self.cmd_rx.recv() {
            match cmd {
                AsyncWriteCommand::Write(data) => {
                    let mut pos = 0;

                    while pos < data.len() {
                        match self.writer.write(&data[pos..]) {
                            Ok(advance) => {
                                pos += advance;
                            }
                            Err(error) => {
                                let retry_wait = Arc::new(Condvar::new());
                                self.status_tx
                                    .send(Err(WriteError {
                                        error,
                                        retry: Some(retry_wait.clone()),
                                    }))
                                    .unwrap();

                                // Wait until sent a signal to retry from the
                                // async controller. If the controller dropped,
                                // also quit this receiving thread. Otherwise
                                // only exit the wait on a non-suprious wakeup.
                                loop {
                                    let mut retry_toggle =
                                        retry_wait.wait(self.retry_toggle.lock().unwrap()).unwrap();

                                    if !retry_toggle.alive {
                                        return;
                                    }

                                    if retry_toggle.enabled {
                                        retry_toggle.enabled = false;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                AsyncWriteCommand::Flush => {
                    self.status_tx
                        .send(
                            self.writer
                                .flush()
                                .map_err(|error| WriteError { error, retry: None }),
                        )
                        .unwrap();
                    if let Some(flush_waker) = self.flush_waker.lock().unwrap().take() {
                        flush_waker.wake();
                    }
                }
            }
        }
    }
}

impl SyncAsAsyncWrite {
    fn new<W, F>(writer: &W, handle: &F, buffer_min_size: usize) -> std::io::Result<Self>
    where
        W: WriteDisk,
        F: Fn(SyncAsAsyncWriteBg<W::WriteDisk>),
    {
        let (cmd_tx, cmd_rx) = channel();
        let (status_tx, status_rx) = channel();
        let retry_toggle = Arc::new(Mutex::default());
        let flush_waker = Arc::new(Mutex::default());

        (handle)(SyncAsAsyncWriteBg {
            writer: writer.write_disk()?,
            cmd_rx,
            status_tx,
            retry_toggle: retry_toggle.clone(),
            flush_waker: flush_waker.clone(),
        });

        Ok(Self {
            cmd_tx,
            status_rx,
            write_buffer: Vec::with_capacity(0),
            buffer_min_size,
            num_cmds_pending: 0,
            retry: None,
            retry_toggle,
            flush_waker,
        })
    }
}

impl AsyncWrite for SyncAsAsyncWrite {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>, // This never waits.
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        // The underlying thread had an I/O failure, but the new write
        // indicates the caller wants to try again. Reschedule the failed call.
        if let Some(retry_wake) = self.retry.take() {
            self.retry_toggle.lock().unwrap().enabled = true;
            retry_wake.notify_one();
        }

        // Count off all the finished writes. Since the thread pauses after an
        // error to wait for a retry, two errors will not pipeline in statuses.
        while let Ok(status) = self.status_rx.try_recv() {
            if let Err(e) = status {
                self.retry = e.retry;
                return Poll::Ready(Err(e.error));
            } else {
                self.num_cmds_pending -= 1;
            }
        }

        self.write_buffer.extend_from_slice(buf);

        // This check avoids constant allocation to send single bytes, assuming
        // the writing client does not batch in large requests.
        if self.write_buffer.len() >= self.buffer_min_size {
            let send_buffer = std::mem::replace(&mut self.write_buffer, Vec::with_capacity(0));
            self.cmd_tx
                .send(AsyncWriteCommand::Write(send_buffer.into()))
                .unwrap();
            self.num_cmds_pending += 1;
        }

        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        if self.flush_waker.lock().unwrap().is_none() {
            self.cmd_tx.send(AsyncWriteCommand::Flush).unwrap();
        }

        while let Ok(status) = self.status_rx.try_recv() {
            if let Err(e) = status {
                self.retry = e.retry;
                return Poll::Ready(Err(e.error));
            } else {
                self.num_cmds_pending -= 1;
            }
        }

        if self.num_cmds_pending > 0 {
            *self.flush_waker.lock().unwrap() = Some(cx.waker().clone());
            Poll::Pending
        } else {
            Poll::Ready(Ok(()))
        }
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        self.poll_flush(cx)
    }
}

impl Drop for SyncAsAsyncWrite {
    fn drop(&mut self) {
        // In case the processing thread is waiting for a retry signal, set a
        // notification that the thread died and send the final wakeup.
        self.retry_toggle.lock().unwrap().alive = false;
        if let Some(wake) = self.retry.take() {
            wake.notify_all();
        }
    }
}

impl<W, F> WriteDisk for SyncAsAsync<W, F>
where
    W: WriteDisk,
{
    type WriteDisk = W::WriteDisk;

    fn write_disk(&self) -> std::io::Result<Self::WriteDisk> {
        self.inner.write_disk()
    }
}

impl<W, F> AsyncWriteDisk for SyncAsAsync<W, F>
where
    W: WriteDisk + Unpin + Send + Sync,
    F: Fn(SyncAsAsyncWriteBg<W::WriteDisk>) + Unpin + Send + Sync,
{
    type WriteDisk = SyncAsAsyncWrite;

    async fn async_write_disk(&self) -> std::io::Result<Self::WriteDisk> {
        SyncAsAsyncWrite::new(&self.inner, &self.handle, self.write_buffer_min_size)
    }
}
