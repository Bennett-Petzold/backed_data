use std::marker::PhantomData;

use csv_async::{AsyncReaderBuilder, AsyncWriterBuilder, QuoteStyle, Terminator, Trim};
use futures::TryStreamExt;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use super::{AsyncDecoder, AsyncEncoder};

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
pub struct TrimWrapper(#[serde(with = "TrimSerial")] Trim);

#[derive(Deserialize, Serialize)]
#[serde(remote = "csv_async::Terminator")]
#[non_exhaustive]
pub enum TerminatorSerial {
    CRLF,
    Any(u8),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TerminatorWrapper(#[serde(with = "TerminatorSerial")] Terminator);

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
pub struct QuoteStyleWrapper(#[serde(with = "QuoteStyleSerial")] QuoteStyle);

/// Unified [`csv_async::ReaderBuilder`] and [`csv_async::WriterBuilder`] configuration.
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
            .create_deserializer(source.compat())
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
        let mut writer = self
            .writer_builder()
            .create_serializer(target.compat_write());

        for line in data.as_ref() {
            writer.serialize(line).await?
        }
        Ok(())
    }
}

/*
#[cfg(test)]
mod tests {
    use std::{
        io::{Cursor, Seek},
        sync::LazyLock,
    };

    use super::*;

    #[derive(Debug, Default, PartialEq, PartialOrd, Serialize, Deserialize)]
    struct IouZipcodes {
        zip: u32,
        eiaid: u32,
        utility_name: String,
        state: String,
        service_type: String,
        ownership: String,
        comm_rate: f64,
        ind_rate: f64,
        res_rate: f64,
    }

    static FIRST_ENTRY: LazyLock<IouZipcodes> = LazyLock::new(|| IouZipcodes {
        zip: 85321,
        eiaid: 176,
        utility_name: "Ajo Improvement Co".to_string(),
        state: "AZ".to_string(),
        service_type: "Bundled".to_string(),
        ownership: "Investor Owned".to_string(),
        comm_rate: 0.08789049919484701,
        ind_rate: 0.0,
        res_rate: 0.09388714733542321,
    });

    static LAST_ENTRY: LazyLock<IouZipcodes> = LazyLock::new(|| IouZipcodes {
        zip: 96107,
        eiaid: 57483,
        utility_name: "Liberty Utilities".to_string(),
        state: "CA".to_string(),
        service_type: "Bundled".to_string(),
        ownership: "Investor Owned".to_string(),
        comm_rate: 0.14622411551676107,
        ind_rate: 0.0,
        res_rate: 0.14001899277463484,
    });

    #[test]
    fn load() {
        let mut buf = Cursor::new(include_bytes!(
            "../../../test_data/iou_zipcodes_2020_stub.csv"
        ));

        let coder = AsyncCsvCoder::<Vec<_>, _>::default();

        let vals: Vec<IouZipcodes> = coder.decode(&mut buf).unwrap();

        assert_eq!(vals[0], *FIRST_ENTRY);

        assert_eq!(vals[vals.len() - 1], *LAST_ENTRY);
    }

    #[test]
    fn write_and_load() {
        let mut buf = Cursor::new(include_bytes!(
            "../../../test_data/iou_zipcodes_2020_stub.csv"
        ));
        let mut write_buf = Cursor::new(Vec::new());

        let coder = AsyncCsvCoder::<Vec<_>, _>::default();

        let vals: Vec<IouZipcodes> = coder.decode(&mut buf).unwrap();

        coder.encode(&vals, &mut write_buf).unwrap();
        write_buf.rewind().unwrap();

        let second_vals: Vec<_> = coder.decode(&mut write_buf).unwrap();

        assert_eq!(second_vals[0], *FIRST_ENTRY);

        assert_eq!(second_vals[second_vals.len() - 1], *LAST_ENTRY);
    }
}
*/
