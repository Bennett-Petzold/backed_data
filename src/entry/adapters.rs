/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/*!
Defines adapters that allow synchronous [`disks`][`super::disks`] and/or
[format encoders][`super::formats`] to be used asynchronously.
*/

use std::{
    cmp::min,
    future::{ready, Future, Ready},
    io::{Cursor, ErrorKind, Read, Write},
    marker::PhantomData,
    pin::{pin, Pin},
    sync::{
        mpsc::{channel, sync_channel, Receiver, Sender, SyncSender, TryRecvError, TrySendError},
        Arc, Condvar, Mutex,
    },
    task::{Context, Poll, Waker},
    time::Duration,
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
    cmd_tx: SyncSender<ReadCmd>,
    waker: Arc<std::sync::Mutex<Option<Waker>>>,
    #[allow(clippy::type_complexity)]
    read: Arc<std::sync::Mutex<Option<std::io::Result<Vec<u8>>>>>,
    buffered: Vec<u8>,
}

#[derive(Debug)]
enum ReadCmd {
    Read,
}

/// [`BlockingFn`] that spawns a background reader.
#[derive(Debug)]
pub struct SyncAsAsyncReadBg<R> {
    reader: R,
    buffer_size: usize,
    cmd_rx: Receiver<ReadCmd>,
    waker: Arc<std::sync::Mutex<Option<Waker>>>,
    #[allow(clippy::type_complexity)]
    read: Arc<std::sync::Mutex<Option<std::io::Result<Vec<u8>>>>>,
}

impl<R: Read> BlockingFn for SyncAsAsyncReadBg<R> {
    type Output = ();
    fn call(mut self) -> Self::Output {
        // Only valid command to be sent is a Read
        while self.cmd_rx.recv().is_ok() {
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
            if let Some(w) = self.waker.lock().unwrap().take() {
                w.wake()
            }

            drop(read_send);
        }
    }
}

impl SyncAsAsyncRead {
    fn new<R, F>(disk: &R, handle: &F, buffer_size: usize) -> std::io::Result<Self>
    where
        R: ReadDisk,
        F: Fn(SyncAsAsyncReadBg<R::ReadDisk>),
    {
        let (cmd_tx, cmd_rx) = sync_channel(0);
        let waker = Arc::new(std::sync::Mutex::default());
        let read = Arc::new(std::sync::Mutex::default());

        (handle)(SyncAsAsyncReadBg {
            reader: disk.read_disk()?,
            cmd_rx,
            waker: waker.clone(),
            read: read.clone(),
            buffer_size,
        });

        let buffered: Box<[_]> = Box::new([]);
        let buffered = buffered.into_vec();

        Ok(Self {
            pending: false,
            cmd_tx,
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
                        *self.waker.lock().unwrap() = Some(cx.waker().clone());
                        return Poll::Pending;
                    }
                };
                drop(read);
            }

            self.buffered = temp_buffer;
            self.pending = false;
        } else if self.buffered.is_empty() {
            self.pending = true;

            *self.waker.lock().unwrap() = Some(cx.waker().clone());
            self.cmd_tx.send(ReadCmd::Read).unwrap();

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
    R: ReadDisk + Send + Sync,
    RF: Fn(SyncAsAsyncReadBg<R::ReadDisk>) + Send + Sync,
    WF: Unpin + Sync,
{
    type ReadDisk = SyncAsAsyncRead;
    type ReadFut = Ready<std::io::Result<Self::ReadDisk>>;

    fn async_read_disk(&self) -> Self::ReadFut {
        ready(SyncAsAsyncRead::new(
            &self.inner,
            &self.read_handle,
            self.read_buffer_size,
        ))
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
                                    let (mut retry_toggle, _) = retry_wait
                                        .wait_timeout(
                                            self.retry_toggle.lock().unwrap(),
                                            Duration::from_secs(1),
                                        )
                                        .unwrap();

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

                    // Send ok status for complete write
                    let _ = self.status_tx.send(Ok(()));
                }
                AsyncWriteCommand::Flush => {
                    let _ = self.status_tx.send(
                        self.writer
                            .flush()
                            .map_err(|error| WriteError { error, retry: None }),
                    );
                    let flush_waker = self.flush_waker.lock().unwrap();
                    if let Some(flush_waker) = &*flush_waker {
                        flush_waker.wake_by_ref();
                    }
                }
            }
        }
    }
}

impl SyncAsAsyncWrite {
    fn new<W, F>(writer: &mut W, handle: &F, buffer_min_size: usize) -> std::io::Result<Self>
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
            let send_buffer = std::mem::take(&mut self.write_buffer);
            self.cmd_tx
                .send(AsyncWriteCommand::Write(send_buffer.into()))
                .unwrap();
            self.num_cmds_pending += 1;
        }

        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        // The underlying thread had an I/O failure, but the new write
        // indicates the caller wants to try again. Reschedule the failed call.
        if let Some(retry_wake) = self.retry.take() {
            self.retry_toggle.lock().unwrap().enabled = true;
            retry_wake.notify_one();
        }

        // May need to add in a final write of buffered data
        if !self.write_buffer.is_empty() {
            let send_buffer = std::mem::take(&mut self.write_buffer);
            self.cmd_tx
                .send(AsyncWriteCommand::Write(send_buffer.into()))
                .unwrap();
            self.num_cmds_pending += 1;
        }

        if self.flush_waker.lock().unwrap().is_none() {
            self.cmd_tx.send(AsyncWriteCommand::Flush).unwrap();
            self.num_cmds_pending += 1;
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

    fn write_disk(&mut self) -> std::io::Result<Self::WriteDisk> {
        self.inner.write_disk()
    }
}

impl<W, RF, WF> AsyncWriteDisk for SyncAsAsync<W, RF, WF>
where
    W: WriteDisk + Send + Sync,
    RF: Unpin + Sync + Send,
    WF: Fn(SyncAsAsyncWriteBg<W::WriteDisk>) + Send + Sync,
{
    type WriteDisk = SyncAsAsyncWrite;
    type WriteFut = Ready<std::io::Result<Self::WriteDisk>>;

    fn async_write_disk(&mut self) -> Self::WriteFut {
        ready(SyncAsAsyncWrite::new(
            &mut self.inner,
            &self.write_handle,
            self.write_buffer_min_size,
        ))
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
pub struct SyncCoderAsyncDisk<C, D, RF, EF, DR, ER> {
    coder: C,
    #[serde(skip)]
    read_handle: Option<RF>,
    #[serde(skip)]
    encode_handle: Option<EF>,
    #[serde(skip)]
    _phantom: (PhantomData<D>, PhantomData<DR>, PhantomData<ER>),
}

impl<C, D, RF, EF, DR, ER> SyncCoderAsyncDisk<C, D, RF, EF, DR, ER> {
    pub fn new(coder: C, read_handle: RF, encode_handle: EF) -> Self {
        Self {
            coder,
            read_handle: Some(read_handle),
            encode_handle: Some(encode_handle),
            _phantom: (PhantomData, PhantomData, PhantomData),
        }
    }

    pub fn set_handles(&mut self, read_handle: RF, encode_handle: EF) -> &mut Self {
        self.read_handle = Some(read_handle);
        self.encode_handle = Some(encode_handle);
        self
    }
}

/// Adapter that links a sync read adapater with some async provider.
///
/// The async provider is expected to be on an external thread via
/// [`AsyncToSyncReadProvider`]. The async provider is also expected to handle
/// errors on its end.
#[derive(Debug)]
pub struct AsyncToSyncRead {
    holdover: Vec<u8>,
    input: Arc<(Condvar, Mutex<(Option<Vec<u8>>, Option<Waker>)>)>,
}

impl Read for AsyncToSyncRead {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // Read in new bytes from async provider
        if self.holdover.is_empty() {
            let mut input_handle = self.input.1.lock().unwrap();

            // Ping waker if other thread is waiting to provide a new value
            // A new value won't be read in until the handle is released.
            // The provider end's wait for a mutex drop past this point
            // is minimal.
            if let Some(waker) = input_handle.1.take() {
                waker.wake();
            }

            // Take immediately available value, if present
            if let Some(val) = input_handle.0.take() {
                self.holdover = val;
            } else {
                // The other thread now MUST use condvar to wakeup when a value is available.
                // Dropping with no value available will cause a hang, on drop a zero-length read
                // should be mimicked with a zero-length vector.
                loop {
                    // This is where the handle is temporarily released for provider to grab.
                    let mut input_handle_cond = self.input.0.wait(input_handle).unwrap();
                    input_handle = input_handle_cond;
                    if let Some(val) = input_handle.0.take() {
                        self.holdover = val;
                    }
                }
            };
        }

        // Copy all the bytes out from holdover there's space for.
        let copy_len = min(buf.len(), self.holdover.len());
        buf[copy_len..].copy_from_slice(&self.holdover[copy_len..]);
        self.holdover.truncate(self.holdover.len() - copy_len);

        Ok(copy_len)
    }
}

#[derive(Debug)]
enum AsyncToSyncReadProviderState {
    /// Between or prior to reads
    Inactive,
    Reading,
}

/// Converts an [`AsyncRead`] type to provide for a sync thread.
///
/// Returns `Ok(())` when a zero length read occurs, or the other end closes.
/// Errors are nonfatal.
#[derive(Debug)]
pub struct AsyncToSyncReadProvider<D> {
    inner: D,
    output: Arc<(Condvar, Mutex<(Option<Vec<u8>>, Option<Waker>)>)>,
    state: AsyncToSyncReadProviderState,
}

impl<D: AsyncBufRead + Unpin> Future for AsyncToSyncReadProvider<D> {
    type Output = std::io::Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        match this.state {
            AsyncToSyncReadProviderState::Inactive => {
                let mut output_handle = this.output.1.lock().unwrap();

                // Register a waker if the other end still hasn't read out
                if output_handle.0.is_some() {
                    output_handle.1 = Some(cx.waker().clone());
                    Poll::Pending
                } else {
                    this.state = AsyncToSyncReadProviderState::Reading;

                    // Release mutex handle prior to entering read
                    drop(output_handle);

                    // Run the read op immediately
                    Pin::new(this).poll(cx)
                }
            }
            AsyncToSyncReadProviderState::Reading => {
                let mut inner = Pin::new(&mut this.inner);
                match inner.as_mut().poll_fill_buf(cx) {
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(x) => match x {
                        Ok(res) => {
                            let res_len = res.len();
                            let mut output_handle = this.output.1.lock().unwrap();

                            // Copy over the buffer, note the take, and notify.
                            output_handle.0 = Some(res.into());
                            inner.consume(res_len);
                            this.output.0.notify_one();

                            // End on zero length read
                            if res_len == 0 {
                                Poll::Ready(Ok(()))
                            } else {
                                drop(output_handle);

                                this.state = AsyncToSyncReadProviderState::Inactive;
                                Pin::new(this).poll(cx)
                            }
                        }
                        Err(e) => Poll::Ready(Err(e)),
                    },
                }
            }
        }
    }
}

/// Set empty read equivalent and notify the sync end, if waiting
///
/// Without this drop impl, the sync end might hang forever waiting for new data.
impl<T> Drop for AsyncToSyncReadProvider<T> {
    fn drop(&mut self) {
        let mut output_handle = self.output.1.lock().unwrap();
        output_handle.0 = Some(vec![]);
        self.output.0.notify_all();
    }
}

impl AsyncToSyncRead {
    pub fn new<T>(async_bufreader: T) -> (Self, AsyncToSyncReadProvider<T>) {
        let comm = Arc::new((Condvar::new(), Mutex::default()));
        (
            Self {
                holdover: vec![],
                input: comm.clone(),
            },
            AsyncToSyncReadProvider {
                inner: async_bufreader,
                output: comm,
                state: AsyncToSyncReadProviderState::Inactive,
            },
        )
    }
}

/// [`BlockingFn`] that runs a synchronous decoder.
#[derive(Debug)]
pub struct DecodeBg<'a, D, R> {
    decoder: &'a D,
    bytes: R,
}

impl<D, R> BlockingFn for DecodeBg<'_, D, R>
where
    D: Decoder<R>,
    R: Read,
{
    type Output = Result<D::T, D::Error>;
    fn call(mut self) -> Self::Output {
        self.decoder.decode(&mut self.bytes)
    }
}

#[derive(Debug)]
enum DecodeBgFutState<S> {
    NoHandle,
    AsyncRead(AsyncToSyncReadProvider<S>),
    Decode,
}

#[derive(Debug)]
pub struct DecodeBgFut<C, S, DR> {
    state: DecodeBgFutState<S>,
    decode_handle: Option<DR>,
    _phantom: PhantomData<C>,
}

impl<C, DD, DR> DecodeBgFut<C, DD, DR> {
    pub fn new<D, RF, EF, ER>(
        inner: &SyncCoderAsyncDisk<C, D, RF, EF, DR, ER>,
        mut source: D::ReadDisk,
    ) -> Self
    where
        D: AsyncReadDisk<ReadDisk = DD>,
        RF: Fn(DecodeBg<C, AsyncToSyncRead>) -> DR,
    {
        if let Some(spawn_fn) = &inner.read_handle {
            let (reader, provider) = AsyncToSyncRead::new(source);
            let background = (spawn_fn)(DecodeBg {
                decoder: &inner.coder,
                bytes: reader,
            });
            Self {
                state: DecodeBgFutState::AsyncRead(provider),
                decode_handle: Some(background),
                _phantom: PhantomData,
            }
        } else {
            Self {
                state: DecodeBgFutState::NoHandle,
                decode_handle: None,
                _phantom: PhantomData,
            }
        }
    }
}

impl<C, D, DR> Future for DecodeBgFut<C, D, DR>
where
    C: Decoder<AsyncToSyncRead> + Unpin,
    D: AsyncBufRead + Unpin,
    DR: Future<Output = Result<C::T, C::Error>> + Unpin,
{
    type Output = Result<C::T, C::Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        match &mut this.state {
            // Misconfiguration trap
            DecodeBgFutState::NoHandle => Poll::Ready(Err(std::io::Error::new(
                ErrorKind::Other,
                "Need a handle configured to run!",
            )
            .into())),

            // Polling until async reads are done
            DecodeBgFutState::AsyncRead(provider) => match Pin::new(provider).poll(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(x) => {
                    if let Err(e) = x {
                        Poll::Ready(Err(e.into()))
                    } else {
                        this.state = DecodeBgFutState::Decode;
                        Pin::new(this).poll(cx)
                    }
                }
            },

            // Waiting for backing decode to finish
            DecodeBgFutState::Decode => {
                Pin::new(this.decode_handle.as_mut().unwrap_or_else(|| {
                    panic!("Should be in NoHandle state if the decode_handle is None")
                }))
                .poll(cx)
            }
        }
    }
}

impl<C, D, RF, EF, DR, ER> AsyncDecoder<D::ReadDisk> for SyncCoderAsyncDisk<C, D, RF, EF, DR, ER>
where
    C: Decoder<AsyncToSyncRead> + Unpin,
    D: AsyncReadDisk<ReadDisk: AsyncBufRead>,
    RF: Fn(DecodeBg<C, AsyncToSyncRead>) -> DR,
    DR: Future<Output = Result<C::T, C::Error>> + Unpin,
{
    type Error = C::Error;
    type T = C::T;
    type DecodeFut = DecodeBgFut<C, D::ReadDisk, DR>;

    fn decode(&self, source: D::ReadDisk) -> Self::DecodeFut {
        DecodeBgFut::new(self, source)
    }
}

/// Adapter that links a sync read adapater with some async provider.
///
/// The async provider is expected to be on an external thread via
/// [`AsyncToSyncWriteProvider`]. The async provider is also expected to handle
/// errors on its end.
#[derive(Debug)]
pub struct AsyncToSyncWrite {
    cmd_tx: Sender<AsyncWriteCommand>,
    cmd_wakeup: Arc<Mutex<Option<Waker>>>,
    write_buffer: Vec<u8>,
    flush_pending: bool,
}

impl Write for AsyncToSyncWrite {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let send_buffer = buf.into();

        // Use this mutex guard to prevent operation interleave
        let mut wake_handle = self.cmd_wakeup.lock().unwrap();

        self.cmd_tx
            .send(AsyncWriteCommand::Write(send_buffer))
            .unwrap();

        // Wake up the async end, if it cleared out the queue
        if let Some(waker) = wake_handle.take() {
            waker.wake();
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        // Use this mutex guard to prevent operation interleave
        let mut wake_handle = self.cmd_wakeup.lock().unwrap();

        if !self.flush_pending {
            self.cmd_tx.send(AsyncWriteCommand::Flush).unwrap();
            self.flush_pending = true;
        }

        Ok(())
    }
}

#[derive(Debug)]
enum AsyncToSyncWriteProviderState {
    /// Invalid configuration
    NoHandle,
    /// Base case, checks for another write to execute
    Pending,
    /// Execute the last sent write.
    Write(Box<[u8]>),
    /// Execute a flush command.
    Flush,
    /// Use `close()` to finish out the connection.
    Shutdown,
    /// Check encoder state after shutdown
    EncoderRes,
}

/// Converts an [`AsyncWrite`] type to get data from a sync thread.
///
/// Returns `Ok(())` when encoding is finished. I/O errors are nonfatal,
/// encoding errors may be fatal.
#[derive(Debug)]
pub struct AsyncToSyncWriteProvider<D, E> {
    inner: D,
    encode_handle: Option<E>,
    state: AsyncToSyncWriteProviderState,
    cmd_rx: Receiver<AsyncWriteCommand>,
    cmd_wakeup: Arc<Mutex<Option<Waker>>>,
}

impl<D, E, Err> Future for AsyncToSyncWriteProvider<D, E>
where
    D: AsyncWrite + Unpin,
    E: Future<Output = Result<(), Err>> + Unpin,
    Err: From<std::io::Error>,
{
    type Output = Result<(), Err>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        match this.state {
            AsyncToSyncWriteProviderState::NoHandle => Poll::Ready(Err(std::io::Error::new(
                ErrorKind::Other,
                "Need a handle configured to run!",
            )
            .into())),

            AsyncToSyncWriteProviderState::Pending => {
                // Use this mutex guard to prevent operation interleave
                let mut wakeup_handle = this.cmd_wakeup.lock().unwrap();

                match this.cmd_rx.try_recv() {
                    Ok(cmd) => {
                        this.state = match cmd {
                            AsyncWriteCommand::Write(buf) => {
                                AsyncToSyncWriteProviderState::Write(buf)
                            }
                            AsyncWriteCommand::Flush => AsyncToSyncWriteProviderState::Flush,
                        };

                        drop(wakeup_handle);
                        Pin::new(this).poll(cx)
                    }
                    Err(TryRecvError::Empty) => {
                        *wakeup_handle = Some(cx.waker().clone());
                        Poll::Pending
                    }
                    Err(TryRecvError::Disconnected) => {
                        this.state = AsyncToSyncWriteProviderState::Shutdown;

                        drop(wakeup_handle);
                        Pin::new(this).poll(cx)
                    }
                }
            }

            AsyncToSyncWriteProviderState::Write(ref mut buf) => {
                match Pin::new(&mut this.inner).poll_write(cx, buf) {
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(res) => {
                        if let Err(e) = res {
                            Poll::Ready(Err(e.into()))
                        } else {
                            this.state = AsyncToSyncWriteProviderState::Pending;
                            Pin::new(this).poll(cx)
                        }
                    }
                }
            }

            AsyncToSyncWriteProviderState::Flush => {
                match Pin::new(&mut this.inner).poll_flush(cx) {
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(res) => {
                        if let Err(e) = res {
                            Poll::Ready(Err(e.into()))
                        } else {
                            this.state = AsyncToSyncWriteProviderState::Pending;
                            Pin::new(this).poll(cx)
                        }
                    }
                }
            }

            AsyncToSyncWriteProviderState::Shutdown => {
                match Pin::new(&mut this.inner).poll_close(cx) {
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(Ok(x)) => {
                        this.state = AsyncToSyncWriteProviderState::EncoderRes;
                        Pin::new(this).poll(cx)
                    }
                    Poll::Ready(Err(e)) => Poll::Ready(Err(e.into())),
                }
            }

            AsyncToSyncWriteProviderState::EncoderRes => {
                Pin::new(this.encode_handle.as_mut().unwrap_or_else(|| {
                    panic!("Should be in NoHandle state if the encode_handle is None")
                }))
                .poll(cx)
            }
        }
    }
}

impl<WD, ER> AsyncToSyncWriteProvider<WD, ER> {
    pub fn new<C, D, RF, EF, DR>(
        inner: &SyncCoderAsyncDisk<C, D, RF, EF, DR, ER>,
        data: &C::T,
        target: D::WriteDisk,
    ) -> Self
    where
        C: Encoder<AsyncToSyncWrite>,
        D: AsyncWriteDisk<WriteDisk = WD>,
        EF: Fn(EncodeBg<C, AsyncToSyncWrite>) -> ER,
        ER: Future<Output = Result<(), C::Error>>,
        WD: Unpin,
    {
        let (cmd_tx, cmd_rx) = channel();
        let cmd_wakeup = Arc::new(Mutex::default());

        let sync_write = AsyncToSyncWrite {
            cmd_tx,
            cmd_wakeup: cmd_wakeup.clone(),
            write_buffer: Vec::new(),
            flush_pending: false,
        };

        let background = EncodeBg {
            encoder: &inner.coder,
            sink: sync_write,
            data,
        };

        let (encode_handle, state) = if let Some(encode_spawn) = &inner.encode_handle {
            (
                Some((encode_spawn)(background)),
                AsyncToSyncWriteProviderState::Pending,
            )
        } else {
            (None, AsyncToSyncWriteProviderState::NoHandle)
        };

        Self {
            inner: target,
            encode_handle,
            state,
            cmd_rx,
            cmd_wakeup,
        }
    }
}

/// [`BlockingFn`] that runs a synchronous encoder into a write sink.
#[derive(Debug)]
pub struct EncodeBg<'a, E: Encoder<S>, S> {
    encoder: &'a E,
    sink: S,
    data: &'a E::T,
}

impl<E, S> BlockingFn for EncodeBg<'_, E, S>
where
    E: Encoder<S, T: Sized>,
    S: Write,
{
    type Output = Result<(), E::Error>;
    fn call(self) -> Self::Output {
        self.encoder.encode(self.data, self.sink)
    }
}

impl<C, D, RF, EF, DR, ER> AsyncEncoder<D::WriteDisk> for SyncCoderAsyncDisk<C, D, RF, EF, DR, ER>
where
    C: Encoder<AsyncToSyncWrite>,
    D: AsyncWriteDisk<WriteDisk: Unpin>,
    EF: Fn(EncodeBg<C, AsyncToSyncWrite>) -> ER,
    ER: Future<Output = Result<(), C::Error>> + Unpin,
{
    type Error = C::Error;
    type T = C::T;
    type EncodeFut = AsyncToSyncWriteProvider<D::WriteDisk, ER>;

    fn encode(&self, data: &Self::T, target: D::WriteDisk) -> Self::EncodeFut {
        AsyncToSyncWriteProvider::new(self, data, target)
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
    coder: Arc<C>,
    decode_handle: DF,
    encode_handle: EF,
    _phantom: (PhantomData<D>, PhantomData<DR>, PhantomData<ER>),
}

impl<C, D, DF, EF, DR, ER> SyncCoderAsAsync<C, D, DF, EF, DR, ER> {
    pub fn new(coder: C, decode_handle: DF, encode_handle: EF) -> Self {
        Self {
            coder: Arc::new(coder),
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
    C: Decoder<D::ReadDisk>,
    D: ReadDisk,
    DF: Fn(DecodeBg<C, D::ReadDisk>) -> DR,
    DR: Future<Output = Result<C::T, C::Error>>,
{
    type Error = C::Error;
    type T = C::T;
    type DecodeFut = DR;

    fn decode(&self, source: D::ReadDisk) -> Self::DecodeFut {
        (self.decode_handle)(DecodeBg {
            decoder: &self.coder,
            bytes: source,
        })
    }
}

impl<C, D, DF, EF, DR, ER> AsyncEncoder<D::WriteDisk> for SyncCoderAsAsync<C, D, DF, EF, DR, ER>
where
    C: Encoder<D::WriteDisk, T: Sized + Clone>,
    D: WriteDisk,
    EF: Fn(EncodeBg<C, D::WriteDisk>) -> ER,
    ER: Future<Output = Result<(), C::Error>>,
{
    type Error = C::Error;
    type T = C::T;
    type EncodeFut = ER;

    fn encode(&self, data: &Self::T, target: D::WriteDisk) -> Self::EncodeFut {
        (self.encode_handle)(EncodeBg {
            encoder: &self.coder,
            sink: target,
            data,
        })
    }
}
