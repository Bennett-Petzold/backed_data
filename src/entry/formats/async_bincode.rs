/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use std::future::Future;
use std::marker::{PhantomData, PhantomPinned};
use std::pin::{pin, Pin};
use std::task::{ready, Context, Poll};

use async_bincode::futures::{AsyncBincodeReader, AsyncBincodeWriter};
use async_bincode::AsyncDestination;
use futures::io::{AsyncRead, AsyncWrite};
use futures::sink::Send;
use futures::{sink, AsyncWriteExt, Sink, SinkExt, Stream, StreamExt};
use pin_project::pin_project;
use serde::{Deserialize, Serialize};

use super::{AsyncDecoder, AsyncEncoder};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AsyncBincodeCoder<T: ?Sized> {
    _phantom_data: PhantomData<T>,
}

impl<T: ?Sized> Default for AsyncBincodeCoder<T> {
    fn default() -> Self {
        Self {
            _phantom_data: PhantomData,
        }
    }
}

#[derive(Debug)]
pub struct BincodeRead<R, T> {
    reader: AsyncBincodeReader<R, T>,
}

impl<R, T> Future for BincodeRead<R, T>
where
    for<'a> T: Deserialize<'a>,
    R: AsyncRead + Unpin,
{
    type Output = Result<T, bincode::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let x = ready!(Pin::new(&mut self.reader).poll_next(cx));
        if let Some(val) = x {
            Poll::Ready(val)
        } else {
            Poll::Ready(Err(Box::new(bincode::ErrorKind::Custom(
                "AsyncBincodeReader stream empty".to_string(),
            ))))
        }
    }
}

impl<T, Source> AsyncDecoder<Source> for AsyncBincodeCoder<T>
where
    T: for<'de> Deserialize<'de>,
    Source: AsyncRead + Unpin,
{
    type Error = bincode::Error;
    type T = T;
    type DecodeFut<'a> = BincodeRead<Source, T> where Self: 'a, Source: 'a;

    fn decode<'a>(&'a self, source: Source) -> Self::DecodeFut<'a>
    where
        Source: 'a,
    {
        BincodeRead {
            reader: AsyncBincodeReader::from(source),
        }
    }
}

#[derive(Debug)]
enum AsyncBincodeSendState {
    Prep,
    Flush,
    Close,
}

#[derive(Debug)]
pub struct AsyncBincodeSend<'a, Target, T> {
    writer: AsyncBincodeWriter<Target, &'a T, AsyncDestination>,
    data: &'a T,
    state: AsyncBincodeSendState,
}

impl<'a, Target, T> Future for AsyncBincodeSend<'a, Target, T>
where
    AsyncBincodeWriter<Target, &'a T, AsyncDestination>: Unpin + Sink<&'a T>,
{
    type Output = Result<
        (),
        <AsyncBincodeWriter<Target, &'a T, AsyncDestination> as futures::Sink<&'a T>>::Error,
    >;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.get_mut();
        match this.state {
            AsyncBincodeSendState::Prep => {
                let res = ready!(Pin::new(&mut this.writer).poll_ready(cx));
                if let Err(e) = res {
                    Poll::Ready(Err(e))
                } else {
                    match Pin::new(&mut this.writer).start_send(this.data) {
                        Ok(()) => {
                            this.state = AsyncBincodeSendState::Flush;
                            Pin::new(this).poll(cx)
                        }
                        Err(e) => Poll::Ready(Err(e)),
                    }
                }
            }
            AsyncBincodeSendState::Flush => {
                let res = ready!(Pin::new(&mut this.writer).poll_flush(cx));
                if let Err(e) = res {
                    Poll::Ready(Err(e))
                } else {
                    this.state = AsyncBincodeSendState::Close;
                    Pin::new(this).poll(cx)
                }
            }
            AsyncBincodeSendState::Close => Pin::new(&mut this.writer).poll_close(cx),
        }
    }
}

impl<'a, Target, T> AsyncBincodeSend<'a, Target, T> {
    pub fn new(writer: AsyncBincodeWriter<Target, &'a T, AsyncDestination>, data: &'a T) -> Self {
        Self {
            writer,
            data,
            state: AsyncBincodeSendState::Prep,
        }
    }
}

impl<T, Target> AsyncEncoder<Target> for AsyncBincodeCoder<T>
where
    T: Serialize + Unpin,
    Target: AsyncWrite + Unpin,
{
    type Error = bincode::Error;
    type T = T;
    type EncodeFut<'a> =
        AsyncBincodeSend<'a, Target, T>
        where Self: 'a, Target: 'a;

    fn encode<'a>(&'a self, data: &'a Self::T, target: Target) -> Self::EncodeFut<'a>
    where
        Target: 'a,
    {
        let writer = AsyncBincodeWriter::from(target).for_async();
        AsyncBincodeSend::new(writer, data)
    }
}
