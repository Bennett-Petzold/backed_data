/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use std::future::Future;
use std::marker::PhantomData;
use std::pin::{pin, Pin};
use std::task::{Context, Poll};

use async_bincode::futures::{AsyncBincodeReader, AsyncBincodeWriter};
use futures::io::{AsyncRead, AsyncWrite};
use futures::{AsyncWriteExt, SinkExt, Stream, StreamExt};
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
        match Pin::new(&mut self.reader).poll_next(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(x) => {
                if let Some(val) = x {
                    Poll::Ready(val)
                } else {
                    Poll::Ready(Err(Box::new(bincode::ErrorKind::Custom(
                        "AsyncBincodeReader stream empty".to_string(),
                    ))))
                }
            }
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
    type DecodeFut = BincodeRead<Source, T>;
    fn decode(&self, source: Source) -> BincodeRead<Source, T> {
        BincodeRead {
            reader: AsyncBincodeReader::from(source),
        }
    }
}

/*
impl<T, Target> AsyncEncoder<Target> for AsyncBincodeCoder<T>
where
    T: Serialize + Send + Sync + Unpin,
    Target: AsyncWrite + Send + Sync + Unpin,
    for<'a> &'a mut Target: AsyncWrite,
{
    type Error = bincode::Error;
    type T = T;
    async fn encode(&self, data: &Self::T, mut target: Target) -> Result<(), Self::Error> {
        let mut bincode_writer = AsyncBincodeWriter::from(&mut target).for_async();
        bincode_writer.send(data).await?;
        target.flush().await?;
        target.close().await?;
        Ok(())
    }
}
*/
