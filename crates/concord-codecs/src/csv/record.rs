//! Owned records: [`RawRecord`] and [`RawTable`], and the mapping of a parsed record into a row.

use std::fmt;

use ::csv::{ByteRecord, StringRecord};
use concord_core::CodecError;
use serde::de::DeserializeOwned;
use serde::ser::{Serialize, SerializeSeq, Serializer};

use super::frame::{Table, TableError};
use crate::stream::ReadErrorKind;

/// One parsed record as owned byte fields, without any UTF-8 requirement: the raw route
/// ([`CsvReader::open_raw`](super::CsvReader::open_raw), [`RawTable`]) and a header
/// ([`CsvRecords::headers`](super::CsvRecords::headers)).
///
/// It also serializes as a sequence of byte fields, so raw records can be written back
/// ([`Rows`](super::Rows) of `RawRecord`, or a [`RawTable`]). `Debug` shows the number of fields
/// only: the values are data.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct RawRecord {
    record: ByteRecord,
}

impl RawRecord {
    /// A record of these fields.
    pub fn new<I, F>(fields: I) -> Self
    where
        I: IntoIterator<Item = F>,
        F: AsRef<[u8]>,
    {
        let mut record = ByteRecord::new();
        for field in fields {
            record.push_field(field.as_ref());
        }
        Self { record }
    }

    pub(crate) fn from_byte_record(record: ByteRecord) -> Self {
        Self { record }
    }

    /// The number of fields.
    pub fn len(&self) -> usize {
        self.record.len()
    }

    /// Whether the record has no field.
    pub fn is_empty(&self) -> bool {
        self.record.is_empty()
    }

    /// Field `i`, if there is one.
    pub fn get(&self, i: usize) -> Option<&[u8]> {
        self.record.get(i)
    }

    /// The fields, in order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &[u8]> + DoubleEndedIterator + '_ {
        self.record.iter()
    }

    /// The fields as strings.
    ///
    /// # Errors
    ///
    /// A [`CodecError`] naming the first field (1-based) that is not valid UTF-8.
    pub fn to_strings(&self) -> Result<Vec<String>, CodecError> {
        self.iter()
            .enumerate()
            .map(|(i, field)| {
                std::str::from_utf8(field).map(str::to_owned).map_err(|_| {
                    CodecError::new("a CSV field is not valid UTF-8").at("field", i as u64 + 1)
                })
            })
            .collect()
    }
}

impl fmt::Debug for RawRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RawRecord")
            .field("fields", &self.record.len())
            .finish_non_exhaustive()
    }
}

/// A sequence of byte fields: one positional row.
impl Serialize for RawRecord {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(self.len()))?;
        for field in self.iter() {
            seq.serialize_element(&Bytes(field))?;
        }
        seq.end()
    }
}

struct Bytes<'a>(&'a [u8]);

impl Serialize for Bytes<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(self.0)
    }
}

/// A whole table of byte records: `Decode<RawTable>` and `Encode<RawTable>` of
/// [`Csv`](super::Csv).
///
/// `headers` is the header record of a `HasHeader` table (`None` without one); reading keeps
/// empty and repeated names. Writing a `HasHeader` table requires it; a `NoHeader` table must not
/// have one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RawTable {
    /// The header record, for a table with one.
    pub headers: Option<RawRecord>,
    /// The data records.
    pub records: Vec<RawRecord>,
}

/// Turns a parsed data record into a row (typed or raw).
pub(crate) type MapFn<T> = fn(&Table, ByteRecord) -> Result<T, TableError>;

/// A typed row: UTF-8, then `csv`'s Serde deserializer with the table's names (if any).
pub(crate) fn typed<T: DeserializeOwned>(
    table: &Table,
    record: ByteRecord,
) -> Result<T, TableError> {
    let number = table.records();
    let record = StringRecord::from_byte_record(record).map_err(|e| {
        TableError::new(
            ReadErrorKind::Format,
            CodecError::new("a CSV field is not valid UTF-8")
                .at("record", number)
                .at("field", e.utf8_error().field() as u64 + 1),
        )
    })?;
    record.deserialize::<T>(table.names()).map_err(|e| {
        let field = match e.kind() {
            ::csv::ErrorKind::Deserialize { err, .. } => err.field(),
            _ => None,
        };
        let codec = CodecError::new("a CSV record does not decode into the requested row type")
            .at("record", number);
        let codec = match field {
            Some(field) => codec.at("field", field + 1),
            None => codec,
        };
        TableError::new(ReadErrorKind::Decode, codec.with_source(e))
    })
}

/// A raw row: the bytes as parsed.
pub(crate) fn raw(_: &Table, record: ByteRecord) -> Result<RawRecord, TableError> {
    Ok(RawRecord::from_byte_record(record))
}
