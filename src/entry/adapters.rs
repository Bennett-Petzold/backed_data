use std::{
    cmp::min,
    future::Future,
    io::{Cursor, ErrorKind, Read, Write},
    marker::PhantomData,
    pin::Pin,
    sync::{
        mpsc::{channel, Receiver, Sender},
        Arc, Condvar, Mutex,
    },
    task::{Context, Poll, Waker},
};

use futures::{AsyncBufRead, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use serde::{Deserialize, Serialize};

use crate::utils::blocking::BlockingFn;

use super::{
    disks::{AsyncReadDisk, AsyncWriteDisk, ReadDisk, WriteDisk},
    formats::{AsyncDecoder, AsyncEncoder, Decoder, Encoder},
};

/// Adapts a synchronous disk to be asynchronous.
///
/// This serializes as `inner`.
///
/// The function handle is used to spawn `inner`'s read/write disk on another
/// thread. If the function handle runs on the current thread instead, this
/// will block indefinitely when used.
#[derive(Debug, Clone)]
pub struct SyncAsAsync<T, RF, WF> {
    inner: T,
    read_handle: RF,
    write_handle: WF,
    read_buffer_size: usize,
    write_buffer_min_size: usize,
}

/// Default to 8 KiB, same as [`std::io::BufReader`].
const DEFAULT_READ_BUFFER_SIZE: usize = 8 * 1024;

/// Default to 8 KiB, same as [`std::io::BufReader`].
const DEFAULT_WRITE_BUFFER_SIZE: usize = 8 * 1024;

impl<T, RF, WF> SyncAsAsync<T, RF, WF> {
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
        read_handle: RF,
        write_handle: WF,
        read_buffer_size: Option<usize>,
        write_buffer_min_size: Option<usize>,
    ) -> Self {
        Self {
            inner,
            read_handle,
            write_handle,
            read_buffer_size: read_buffer_size.unwrap_or(DEFAULT_READ_BUFFER_SIZE),
            write_buffer_min_size: write_buffer_min_size.unwrap_or(DEFAULT_WRITE_BUFFER_SIZE),
        }
    }
}

impl<T: Serialize, RF, WF> Serialize for SyncAsAsync<T, RF, WF> {
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
    cmd: Arc<(Condvar, std::sync::Mutex<Option<ReadCmd>>)>,
    initialized: Arc<std::sync::Mutex<bool>>,
    waker: Arc<std::sync::Mutex<Option<Waker>>>,
    #[allow(clippy::type_complexity)]
    read: Arc<std::sync::Mutex<Option<std::io::Result<Vec<u8>>>>>,
    buffered: Vec<u8>,
}

#[derive(Debug)]
enum ReadCmd {
    Read,
    Kill,
}

/// [`BlockingFn`] that spawns a background reader.
#[derive(Debug)]
pub struct SyncAsAsyncReadBg<R> {
    reader: R,
    buffer_size: usize,
    cmd: Arc<(Condvar, std::sync::Mutex<Option<ReadCmd>>)>,
    initialized: Arc<std::sync::Mutex<bool>>,
    waker: Arc<std::sync::Mutex<Option<Waker>>>,
    #[allow(clippy::type_complexity)]
    read: Arc<std::sync::Mutex<Option<std::io::Result<Vec<u8>>>>>,
}

impl<R: Read> BlockingFn for SyncAsAsyncReadBg<R> {
    type Output = ();
    fn call(mut self) -> Self::Output {
        loop {
            // Take the command and signal that it was received
            let cmd = self.cmd.0.wait(self.cmd.1.lock().unwrap()).unwrap().take();
            *self.initialized.lock().unwrap() = true;

            // If `cmd` is `None`, this was a spurious wakeup.
            // See [`std::sync::Condvar::wait`].
            match cmd {
                // Stop looping if the parent was dropped
                Some(ReadCmd::Kill) => {
                    return;
                }
                Some(ReadCmd::Read) => {
                    let mut read_buffer = vec![0; self.buffer_size];

                    let res = self.reader.read(&mut read_buffer);

                    // Mutex guards here held until end of block
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
                    self.waker.lock().unwrap().take().map(|w| w.wake());
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
        let cmd = Arc::new((Condvar::new(), std::sync::Mutex::default()));
        let waker = Arc::new(std::sync::Mutex::default());
        let initialized = Arc::new(std::sync::Mutex::default());
        let read = Arc::new(std::sync::Mutex::default());

        (handle)(SyncAsAsyncReadBg {
            reader: disk.read_disk()?,
            cmd: cmd.clone(),
            waker: waker.clone(),
            initialized: initialized.clone(),
            read: read.clone(),
            buffer_size,
        });

        let buffered: Box<[_]> = Box::new([]);
        let buffered = buffered.into_vec();

        Ok(Self {
            pending: false,
            cmd,
            waker,
            initialized,
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
        // Wait for initialization
        if !(*self.initialized.lock().unwrap()) {
            self.cmd.0.notify_all();
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }

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
                        *self.waker.lock().unwrap() = Some(cx.waker().clone());
                        return Poll::Pending;
                    }
                };
            }

            self.buffered = temp_buffer;
            self.pending = false;
        } else if self.buffered.is_empty() {
            self.pending = true;

            *self.waker.lock().unwrap() = Some(cx.waker().clone());
            *self.cmd.1.lock().unwrap() = Some(ReadCmd::Read);
            self.cmd.0.notify_one();

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
        // Wait for initialization
        while !(*self.initialized.lock().unwrap()) {
            self.cmd.0.notify_all();
        }

        *self.cmd.1.lock().unwrap() = Some(ReadCmd::Kill);
        self.cmd.0.notify_all();
    }
}

impl<R, RF, WF> ReadDisk for SyncAsAsync<R, RF, WF>
where
    R: ReadDisk,
{
    type ReadDisk = R::ReadDisk;

    fn read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        self.inner.read_disk()
    }
}

impl<R, RF, WF> AsyncReadDisk for SyncAsAsync<R, RF, WF>
where
    R: ReadDisk + Unpin + Send + Sync,
    RF: Fn(SyncAsAsyncReadBg<R::ReadDisk>) + Unpin + Send + Sync,
    WF: Unpin + Sync,
{
    type ReadDisk = SyncAsAsyncRead;

    async fn async_read_disk(&self) -> std::io::Result<Self::ReadDisk> {
        SyncAsAsyncRead::new(&self.inner, &self.read_handle, self.read_buffer_size)
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
                            // Just retry if the error is valid under async
                            Err(error)
                                if [ErrorKind::WouldBlock, ErrorKind::Interrupted]
                                    .contains(&error.kind()) => {}
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
                    let _ = self.status_tx.send(
                        self.writer
                            .flush()
                            .map_err(|error| WriteError { error, retry: None }),
                    );
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

impl<W, RF, WF> WriteDisk for SyncAsAsync<W, RF, WF>
where
    W: WriteDisk,
{
    type WriteDisk = W::WriteDisk;

    fn write_disk(&self) -> std::io::Result<Self::WriteDisk> {
        self.inner.write_disk()
    }
}

impl<W, RF, WF> AsyncWriteDisk for SyncAsAsync<W, RF, WF>
where
    W: WriteDisk + Unpin + Send + Sync,
    RF: Unpin + Sync,
    WF: Fn(SyncAsAsyncWriteBg<W::WriteDisk>) + Unpin + Send + Sync,
{
    type WriteDisk = SyncAsAsyncWrite;

    async fn async_write_disk(&self) -> std::io::Result<Self::WriteDisk> {
        SyncAsAsyncWrite::new(&self.inner, &self.write_handle, self.write_buffer_min_size)
    }
}

/// Adapts a synchronous coder and asynchronous disk to be asynchronous.
///
/// The function handle is used to spawn `coder`'s (de)serialization on another
/// thread.
///
/// When this struct is deserialized, the handle is uninitialized. The handle
/// needs to be initialized to use the [`AsyncEncoder`] and [`AsyncDecoder`]
/// implementations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncCoderAsyncDisk<C, D, F, DR, ER> {
    coder: C,
    #[serde(skip)]
    handle: Option<F>,
    #[serde(skip)]
    _phantom: (PhantomData<D>, PhantomData<DR>, PhantomData<ER>),
}

impl<C, D, F, DR, ER> SyncCoderAsyncDisk<C, D, F, DR, ER> {
    pub fn new(coder: C, handle: F) -> Self {
        Self {
            coder,
            handle: Some(handle),
            _phantom: (PhantomData, PhantomData, PhantomData),
        }
    }

    pub fn set_handle(&mut self, handle: F) -> &mut Self {
        self.handle = Some(handle);
        self
    }
}

/// [`BlockingFn`] that runs a synchronous decoder.
#[derive(Debug)]
pub struct DecodeBg<'a, D, B> {
    decoder: &'a D,
    bytes: B,
}

impl<D, B> BlockingFn for DecodeBg<'_, D, B>
where
    D: Decoder<B>,
    B: Read,
{
    type Output = Result<D::T, D::Error>;
    fn call(mut self) -> Self::Output {
        self.decoder.decode(&mut self.bytes)
    }
}

/// [`BlockingFn`] that runs a synchronous encoder into a write sink.
#[derive(Debug)]
pub struct EncodeBg<'a, E: Encoder<S>, S> {
    encoder: &'a E,
    sink: &'a mut S,
    data: &'a E::T,
}

impl<E, S> BlockingFn for EncodeBg<'_, E, S>
where
    E: Encoder<S, T: Sized>,
    S: Write,
{
    type Output = Result<(), E::Error>;
    fn call(mut self) -> Self::Output {
        self.encoder.encode(self.data, &mut self.sink)
    }
}

impl<C, D, F, DR, ER> AsyncDecoder<D::ReadDisk> for SyncCoderAsyncDisk<C, D, F, DR, ER>
where
    C: Decoder<Cursor<Vec<u8>>, T: Sync + Send> + Sync + Send,
    D: AsyncReadDisk<ReadDisk: Sync + Send> + Sync + Send,
    F: Fn(DecodeBg<C, Cursor<Vec<u8>>>) -> DR + Sync + Send,
    DR: Future<Output = Result<C::T, C::Error>> + Sync + Send,
    ER: Sync + Send,
{
    type Error = C::Error;
    type T = C::T;

    async fn decode(&self, source: &mut D::ReadDisk) -> Result<Self::T, Self::Error> {
        let handle = self.handle.as_ref().ok_or(std::io::Error::new(
            ErrorKind::Other,
            "Need a handle configured to run!",
        ))?;
        let mut buffer = Vec::new();
        source.read_to_end(&mut buffer).await?;
        (handle)(DecodeBg {
            decoder: &self.coder,
            bytes: Cursor::new(buffer),
        })
        .await
    }
}

impl<C, D, F, DR, ER> AsyncEncoder<D::WriteDisk> for SyncCoderAsyncDisk<C, D, F, DR, ER>
where
    C: Encoder<Vec<u8>, T: Sync + Send + Sized> + Sync + Send + Unpin,
    D: AsyncWriteDisk<WriteDisk: Sync + Send> + Sync + Send + Unpin,
    F: Fn(EncodeBg<C, Vec<u8>>) -> ER + Sync + Send + Unpin,
    DR: Sync + Send + Unpin,
    ER: Future<Output = Result<Vec<u8>, C::Error>> + Sync + Send + Unpin,
{
    type Error = C::Error;
    type T = C::T;

    async fn encode(&self, data: &Self::T, target: &mut D::WriteDisk) -> Result<(), Self::Error> {
        let handle = self.handle.as_ref().ok_or(std::io::Error::new(
            ErrorKind::Other,
            "Need a handle configured to run!",
        ))?;
        let encoded = (handle)(EncodeBg {
            encoder: &self.coder,
            sink: &mut Vec::new(),
            data,
        })
        .await?;

        target.write_all(&encoded).await?;
        target.flush().await?;
        target.close().await?;

        Ok(())
    }
}

/// Adapts a synchronous coder (with synchronous disks) to be asynchronous.
///
/// The function handle is used to spawn `coder`'s (de)serialization on another
/// thread.
///
/// This struct is deserialized as the underlying sync coder.
#[derive(Debug, Clone)]
pub struct SyncCoderAsAsync<C, D, DF, EF, DR, ER> {
    coder: C,
    decode_handle: DF,
    encode_handle: EF,
    _phantom: (PhantomData<D>, PhantomData<DR>, PhantomData<ER>),
}

impl<C, D, DF, EF, DR, ER> SyncCoderAsAsync<C, D, DF, EF, DR, ER> {
    pub fn new(coder: C, decode_handle: DF, encode_handle: EF) -> Self {
        Self {
            coder,
            decode_handle,
            encode_handle,
            _phantom: (PhantomData, PhantomData, PhantomData),
        }
    }
}

impl<C: Serialize, D, DF, EF, DR, ER> Serialize for SyncCoderAsAsync<C, D, DF, EF, DR, ER> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.coder.serialize(serializer)
    }
}

impl<C, D, DF, EF, DR, ER> AsyncDecoder<D::ReadDisk> for SyncCoderAsAsync<C, D, DF, EF, DR, ER>
where
    C: Decoder<Cursor<Vec<u8>>, T: Sync + Send> + Sync + Send,
    D: ReadDisk<ReadDisk: Sync + Send> + Sync + Send,
    DF: Fn(DecodeBg<C, &mut D::ReadDisk>) -> DR + Sync + Send,
    EF: Sync + Send,
    DR: Future<Output = Result<C::T, C::Error>> + Sync + Send,
    ER: Sync + Send,
{
    type Error = C::Error;
    type T = C::T;

    async fn decode(&self, source: &mut D::ReadDisk) -> Result<Self::T, Self::Error> {
        (self.decode_handle)(DecodeBg {
            decoder: &self.coder,
            bytes: source,
        })
        .await
    }
}

impl<C, D, DF, EF, DR, ER> AsyncEncoder<D::WriteDisk> for SyncCoderAsAsync<C, D, DF, EF, DR, ER>
where
    C: Encoder<D::WriteDisk, T: Sync + Send + Sized> + Sync + Send + Unpin,
    D: WriteDisk<WriteDisk: Send> + Sync + Unpin,
    DF: Sync + Send + Unpin,
    EF: Fn(EncodeBg<C, D::WriteDisk>) -> ER + Sync + Send + Unpin,
    DR: Sync + Send + Unpin,
    ER: Future<Output = Result<(), C::Error>> + Sync + Send + Unpin,
{
    type Error = C::Error;
    type T = C::T;

    async fn encode(&self, data: &Self::T, target: &mut D::WriteDisk) -> Result<(), Self::Error> {
        (self.encode_handle)(EncodeBg {
            encoder: &self.coder,
            sink: target,
            data,
        })
        .await
    }
}
