/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use std::future::Future;
use std::marker::{PhantomData, PhantomPinned};
use std::mem::transmute;
use std::pin::Pin;
use std::task::{ready, Context, Poll};

use csv_async::{
    AsyncReaderBuilder, AsyncSerializer, AsyncWriterBuilder, DeserializeRecordsIntoStream,
    DeserializeRecordsStream, QuoteStyle, Terminator, Trim,
};
use futures::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use futures::Stream;
use pin_project::pin_project;
use serde::{Deserialize, Serialize};

use super::{AsyncDecoder, AsyncEncoder};

#[cfg(feature = "csv")]
use super::CsvCoder;

#[derive(Deserialize, Serialize)]
#[serde(remote = "csv_async::Trim")]
#[non_exhaustive]
pub enum TrimSerial {
    None,
    Headers,
    Fields,
    All,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TrimWrapper(#[serde(with = "TrimSerial")] pub Trim);

#[derive(Deserialize, Serialize)]
#[serde(remote = "csv_async::Terminator")]
#[non_exhaustive]
pub enum TerminatorSerial {
    CRLF,
    Any(u8),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TerminatorWrapper(#[serde(with = "TerminatorSerial")] pub Terminator);

#[derive(Deserialize, Serialize)]
#[serde(remote = "csv_async::QuoteStyle")]
#[non_exhaustive]
pub enum QuoteStyleSerial {
    Always,
    Necessary,
    NonNumeric,
    Never,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QuoteStyleWrapper(#[serde(with = "QuoteStyleSerial")] pub QuoteStyle);

/// Unified [`csv_async::AsyncReaderBuilder`] and
/// [`csv_async::AsyncWriterBuilder`] configuration.
///
/// # Fields
/// * `comment`..=`trim`: Options from the builders, applied if set.
/// * `T`: The output container type.
/// * `E`: The output element type.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct AsyncCsvCoder<T: ?Sized, D: ?Sized> {
    pub comment: Option<u8>,
    pub delimiter: Option<u8>,
    pub double_quote: Option<bool>,
    pub escape: Option<u8>,
    pub flexible: Option<bool>,
    pub has_headers: Option<bool>,
    pub quote: Option<u8>,
    pub quoting: Option<bool>,
    pub quote_style: Option<QuoteStyleWrapper>,
    pub terminator: Option<TerminatorWrapper>,
    pub trim: Option<TrimWrapper>,
    _phantom_container: PhantomData<T>,
    _phantom_elements: PhantomData<D>,
}

impl<T: ?Sized, D: ?Sized> AsyncCsvCoder<T, D> {
    fn reader_builder(&self) -> AsyncReaderBuilder {
        let mut builder = AsyncReaderBuilder::new();
        builder.comment(self.comment);
        if let Some(delimiter) = self.delimiter {
            builder.delimiter(delimiter);
        };
        if let Some(double_quote) = self.double_quote {
            builder.double_quote(double_quote);
        };
        builder.escape(self.escape);
        if let Some(flexible) = self.flexible {
            builder.flexible(flexible);
        };
        if let Some(has_headers) = self.has_headers {
            builder.has_headers(has_headers);
        };
        if let Some(quote) = self.quote {
            builder.quote(quote);
        };
        if let Some(quoting) = self.quoting {
            builder.quoting(quoting);
        };
        if let Some(terminator) = &self.terminator {
            builder.terminator(terminator.0);
        };
        if let Some(trim) = &self.trim {
            builder.trim(trim.0);
        };

        builder
    }

    fn writer_builder(&self) -> AsyncWriterBuilder {
        let mut builder = AsyncWriterBuilder::new();
        builder.comment(self.comment);
        if let Some(delimiter) = self.delimiter {
            builder.delimiter(delimiter);
        };
        if let Some(double_quote) = self.double_quote {
            builder.double_quote(double_quote);
        };
        if let Some(escape) = self.escape {
            builder.escape(escape);
        };
        if let Some(flexible) = self.flexible {
            builder.flexible(flexible);
        };
        if let Some(has_headers) = self.has_headers {
            builder.has_headers(has_headers);
        };
        if let Some(quote) = self.quote {
            builder.quote(quote);
        };
        if let Some(quote_style) = &self.quote_style {
            builder.quote_style(quote_style.0);
        };
        if let Some(terminator) = &self.terminator {
            builder.terminator(terminator.0);
        };

        builder
    }
}

#[pin_project]
pub struct CsvDecodeFut<'a, R: AsyncRead + Unpin + Send, D, T> {
    #[pin]
    deserializer: DeserializeRecordsIntoStream<'a, R, D>,
    items: Vec<D>,
    _phantom: PhantomData<T>,
}

impl<'a, R, D, T> Future for CsvDecodeFut<'a, R, D, T>
where
    R: AsyncRead + Unpin + Send + 'a,
    D: for<'de> Deserialize<'de> + 'a,
    T: ?Sized + From<Vec<D>>,
{
    type Output = Result<T, csv_async::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.as_mut().project();
        loop {
            let next = ready!(this.deserializer.as_mut().poll_next(cx));
            if let Some(item_res) = next {
                match item_res {
                    Ok(item) => {
                        this.items.push(item);
                    }
                    Err(e) => {
                        return Poll::Ready(Err(e));
                    }
                }
            } else {
                let owned_items = std::mem::take(this.items);
                {
                    return Poll::Ready(Ok(owned_items.into()));
                }
            }
        }
    }
}

impl<'a, R, D, T> CsvDecodeFut<'a, R, D, T>
where
    R: AsyncRead + Unpin + Send,
{
    pub fn new(deserializer: DeserializeRecordsIntoStream<'a, R, D>) -> Self {
        Self {
            deserializer,
            items: Vec::new(),
            _phantom: PhantomData,
        }
    }
}

impl<T, D, Source> AsyncDecoder<Source> for AsyncCsvCoder<T, D>
where
    T: ?Sized + From<Vec<D>> + for<'de> Deserialize<'de>,
    D: for<'de> Deserialize<'de>,
    Source: AsyncRead + Send + Sync + Unpin,
{
    type Error = csv_async::Error;
    type T = T;
    type DecodeFut<'a> = CsvDecodeFut<'a, Source, D, T> where Self: 'a, Source: 'a;

    fn decode<'a>(&'a self, source: Source) -> Self::DecodeFut<'a>
    where
        Source: 'a,
    {
        let deserializer = self
            .reader_builder()
            .create_deserializer(source)
            .into_deserialize();
        CsvDecodeFut::new(deserializer)
    }
}

//#[derive(Debug)]
pub struct AsyncEncodeFut<'a, W: AsyncWrite + Unpin, E: Serialize> {
    serializer: AsyncSerializer<W>,
    serialize_fut: Option<Pin<Box<dyn Future<Output = csv_async::Result<()>> + 'a>>>,
    data_view: &'a [E],
    pos: Option<usize>,
    _phantom: PhantomData<E>,
    /// `serialize_fut` uses a mutable pointer to `serializer`
    _pin: PhantomPinned,
}

impl<'a, W, E> Future for AsyncEncodeFut<'a, W, E>
where
    W: AsyncWrite + Unpin + 'a,
    E: Serialize + 'a,
{
    type Output = csv_async::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // No moves will happen, but the struct can't be `Unpin` because of
        // reference shenanigans.
        let this = unsafe { self.as_mut().get_unchecked_mut() };

        loop {
            // `!Unpin` keeps serializer in place, we need to construct futures
            // with a mutable reference to it.
            let serializer_ptr: *mut _ = &mut this.serializer;
            let serializer = unsafe { &mut *serializer_ptr };

            if let Some(ref mut serialize_fut) = &mut this.serialize_fut {
                if let Err(e) = ready!(serialize_fut.as_mut().poll(cx)) {
                    return Poll::Ready(Err(e));
                } else {
                    match this.pos {
                        // Move to the next item
                        Some(x) if x < this.data_view.len() => {
                            let next_pos = x + 1;
                            this.pos = Some(next_pos);

                            let item = this.data_view.get(x).unwrap();
                            this.serialize_fut =
                                Some(Box::pin(async move { serializer.serialize(item).await }));
                        }
                        // Past last item, flush
                        Some(x) => {
                            this.serialize_fut = Some(Box::pin(async {
                                serializer.flush().await.map_err(|e| e.into())
                            }));
                            this.pos = None;
                        }
                        // Finished all writes and flush, just close
                        None => {
                            return Poll::Ready(Ok(()));
                        }
                    }
                }
            // Initialization logic, which relies on !Unpin.
            } else if let Some(item) = this.data_view.first() {
                this.serialize_fut =
                    Some(Box::pin(async move { serializer.serialize(item).await }));
            } else {
                this.serialize_fut = Some(Box::pin(async {
                    serializer.flush().await.map_err(|e| e.into())
                }));
            }
        }
    }
}

impl<'a, W, E> AsyncEncodeFut<'a, W, E>
where
    W: AsyncWrite + Unpin + 'a,
    E: Serialize + 'a,
{
    pub fn new<T>(serializer: AsyncSerializer<W>, data: &'a T) -> Self
    where
        T: ?Sized + AsRef<[E]>,
    {
        Self {
            serializer,
            serialize_fut: None,
            data_view: data.as_ref(),
            pos: Some(0),
            _phantom: PhantomData,
            _pin: PhantomPinned,
        }
    }
}

impl<T, E, Target> AsyncEncoder<Target> for AsyncCsvCoder<T, E>
where
    T: ?Sized + AsRef<[E]> + Default + Serialize + Send + Sync + Unpin,
    E: Serialize + Send + Sync + Unpin,
    Target: AsyncWrite + Send + Sync + Unpin,
    for<'a> &'a mut Target: AsyncWrite,
{
    type Error = csv_async::Error;
    type T = T;
    type EncodeFut<'a> = AsyncEncodeFut<'a, Target, E> where Self: 'a, Target: 'a;

    fn encode<'a>(&'a self, data: &'a Self::T, mut target: Target) -> Self::EncodeFut<'a>
    where
        Target: 'a,
    {
        let mut writer = self.writer_builder().create_serializer(target);
        AsyncEncodeFut::new(writer, data)
    }
}

#[cfg(feature = "csv")]
fn quote_style_conv(other: csv::QuoteStyle) -> QuoteStyle {
    match other {
        csv::QuoteStyle::Always => QuoteStyle::Always,
        csv::QuoteStyle::Necessary => QuoteStyle::Necessary,
        csv::QuoteStyle::NonNumeric => QuoteStyle::NonNumeric,
        csv::QuoteStyle::Never => QuoteStyle::Never,
        x => panic!("Unrecognized quote style in conversion: {:#?}", x),
    }
}

#[cfg(feature = "csv")]
fn term_conv(other: csv::Terminator) -> Terminator {
    match other {
        csv::Terminator::CRLF => Terminator::CRLF,
        csv::Terminator::Any(x) => Terminator::Any(x),
        x => panic!("Unrecognized terminator style in conversion: {:#?}", x),
    }
}

#[cfg(feature = "csv")]
fn trim_conv(other: csv::Trim) -> Trim {
    match other {
        csv::Trim::None => Trim::None,
        csv::Trim::Headers => Trim::Headers,
        csv::Trim::Fields => Trim::Fields,
        csv::Trim::All => Trim::All,
        x => panic!("Unrecognized trim style in conversion: {:#?}", x),
    }
}

#[cfg(feature = "csv")]
impl<T, U> From<CsvCoder<T, U>> for AsyncCsvCoder<T, U> {
    fn from(value: CsvCoder<T, U>) -> Self {
        Self {
            comment: value.comment,
            delimiter: value.delimiter,
            double_quote: value.double_quote,
            escape: value.escape,
            flexible: value.flexible,
            has_headers: value.has_headers,
            quote: value.quote,
            quoting: value.quoting,
            quote_style: value
                .quote_style
                .map(|q| QuoteStyleWrapper(quote_style_conv(q.0))),
            terminator: value.terminator.map(|t| TerminatorWrapper(term_conv(t.0))),
            trim: value.trim.map(|t| TrimWrapper(trim_conv(t.0))),
            _phantom_container: PhantomData,
            _phantom_elements: PhantomData,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Seek};

    use tokio::runtime::Builder;

    use crate::{
        test_utils::csv_data::{IouZipcodes, FIRST_ENTRY, LAST_ENTRY},
        utils::AsyncCompatCursor,
    };

    use super::*;
    #[test]
    fn load() {
        Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(async {
                let mut buf: AsyncCompatCursor<_> = Cursor::new(include_bytes!(
                    "../../../test_data/iou_zipcodes_2020_stub.csv"
                ))
                .into();

                let coder = AsyncCsvCoder::<Vec<_>, _>::default();

                let vals: Vec<IouZipcodes> = coder.decode(&mut buf).await.unwrap();

                assert_eq!(vals[0], *FIRST_ENTRY);

                assert_eq!(vals[vals.len() - 1], *LAST_ENTRY);
            })
    }

    #[test]
    fn write_and_load() {
        Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(async {
                let mut buf: AsyncCompatCursor<_> = Cursor::new(include_bytes!(
                    "../../../test_data/iou_zipcodes_2020_stub.csv"
                ))
                .into();
                let mut write_buf: AsyncCompatCursor<_> = Cursor::new(Vec::new()).into();

                let coder = AsyncCsvCoder::<Vec<_>, _>::default();

                let vals: Vec<IouZipcodes> = coder.decode(&mut buf).await.unwrap();

                coder.encode(&vals, &mut write_buf).await.unwrap();
                write_buf.rewind().unwrap();

                let second_vals: Vec<_> = coder.decode(&mut write_buf).await.unwrap();

                assert_eq!(second_vals[0], *FIRST_ENTRY);

                assert_eq!(second_vals[second_vals.len() - 1], *LAST_ENTRY);
            })
    }
}
