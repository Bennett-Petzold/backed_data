use std::marker::PhantomData;

use csv_async::{AsyncReaderBuilder, AsyncWriterBuilder, QuoteStyle, Terminator, Trim};
use futures::io::{AsyncRead, AsyncWrite};
use futures::TryStreamExt;
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
pub struct AsyncCsvCoder<T: ?Sized, E: ?Sized> {
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
    _phantom_elements: PhantomData<E>,
}

impl<T: ?Sized, E: ?Sized> AsyncCsvCoder<T, E> {
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

impl<T, E, Source> AsyncDecoder<Source> for AsyncCsvCoder<T, E>
where
    T: ?Sized + From<Vec<E>> + Default + for<'de> Deserialize<'de> + Send + Sync,
    E: ?Sized + for<'de> Deserialize<'de> + Send + Sync,
    Source: ?Sized + AsyncRead + Send + Sync + Unpin,
{
    type Error = csv_async::Error;
    type T = T;
    async fn decode(&self, source: &mut Source) -> Result<Self::T, Self::Error> {
        self.reader_builder()
            .create_deserializer(source)
            .deserialize()
            .try_collect::<Vec<E>>()
            .await
            .map(|x| x.into())
    }
}

impl<T, E, Target> AsyncEncoder<Target> for AsyncCsvCoder<T, E>
where
    T: ?Sized + AsRef<[E]> + Default + Serialize + Send + Sync + Unpin,
    E: Serialize + Send + Sync + Unpin,
    Target: AsyncWrite + Send + Sync + Unpin,
{
    type Error = csv_async::Error;
    type T = T;
    async fn encode(&self, data: &Self::T, target: &mut Target) -> Result<(), Self::Error> {
        let mut writer = self.writer_builder().create_serializer(target);

        for line in data.as_ref() {
            writer.serialize(line).await?
        }
        Ok(())
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
