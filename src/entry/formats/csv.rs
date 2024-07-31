use std::{
    io::{Read, Write},
    marker::PhantomData,
};

use csv::{QuoteStyle, ReaderBuilder, Terminator, Trim, WriterBuilder};
use serde::{Deserialize, Serialize};

use super::{Decoder, Encoder};

#[derive(Deserialize, Serialize)]
#[serde(remote = "csv::Trim")]
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
#[serde(remote = "csv::Terminator")]
#[non_exhaustive]
pub enum TerminatorSerial {
    CRLF,
    Any(u8),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TerminatorWrapper(#[serde(with = "TerminatorSerial")] Terminator);

#[derive(Deserialize, Serialize)]
#[serde(remote = "csv::QuoteStyle")]
#[non_exhaustive]
pub enum QuoteStyleSerial {
    Always,
    Necessary,
    NonNumeric,
    Never,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QuoteStyleWrapper(#[serde(with = "QuoteStyleSerial")] QuoteStyle);

/// Unified [`csv::ReaderBuilder`] and [`csv::WriterBuilder`] configuration.
///
/// # Fields
/// * `comment`..=`trim`: Options from the builders, applied if set.
/// * `T`: The output container type.
/// * `E`: The output element type.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CsvCoder<T: ?Sized, E: ?Sized> {
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

impl<T: ?Sized, E: ?Sized> CsvCoder<T, E> {
    fn reader_builder(&self) -> ReaderBuilder {
        let mut builder = ReaderBuilder::new();
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

    fn writer_builder(&self) -> WriterBuilder {
        let mut builder = WriterBuilder::new();
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

impl<T, E, Source> Decoder<Source> for CsvCoder<T, E>
where
    T: ?Sized + FromIterator<E> + for<'de> Deserialize<'de>,
    E: ?Sized + for<'de> Deserialize<'de>,
    Source: ?Sized + Read,
{
    type Error = csv::Error;
    type T = T;
    fn decode(&self, source: &mut Source) -> Result<Self::T, Self::Error> {
        self.reader_builder()
            .from_reader(source)
            .deserialize()
            .collect::<Result<T, _>>()
    }
}

impl<T, E, Target> Encoder<Target> for CsvCoder<T, E>
where
    T: ?Sized + Serialize + AsRef<[E]>,
    E: Serialize,
    Target: ?Sized + Write,
{
    type Error = csv::Error;
    type T = T;
    fn encode(&self, data: &Self::T, target: &mut Target) -> Result<(), Self::Error> {
        let mut writer = self.writer_builder().from_writer(target);
        for line in data.as_ref() {
            writer.serialize(line)?
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Seek};

    use crate::test_utils::csv_data::{IouZipcodes, FIRST_ENTRY, LAST_ENTRY};

    use super::*;

    #[test]
    fn load() {
        let mut buf = Cursor::new(include_bytes!(
            "../../../test_data/iou_zipcodes_2020_stub.csv"
        ));

        let coder = CsvCoder::<Vec<_>, _>::default();

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

        let coder = CsvCoder::<Vec<_>, _>::default();

        let vals: Vec<IouZipcodes> = coder.decode(&mut buf).unwrap();

        coder.encode(&vals, &mut write_buf).unwrap();
        write_buf.rewind().unwrap();

        let second_vals: Vec<_> = coder.decode(&mut write_buf).unwrap();

        assert_eq!(second_vals[0], *FIRST_ENTRY);

        assert_eq!(second_vals[second_vals.len() - 1], *LAST_ENTRY);
    }
}
