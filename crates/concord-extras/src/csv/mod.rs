//! `Csv<HasHeader, Delimiter>`: finite and incremental CSV (feature `csv`).
//!
//! # The format type
//!
//! [`Csv<H, D, P>`](Csv) names a CSV dialect by type: `H` says whether the first parsed record is
//! a header ([`HasHeader`], [`NoHeader`]), `D` is the field delimiter ([`Delimiter<BYTE>`] and its
//! aliases [`Comma`], [`Semicolon`], [`Tab`], [`Pipe`]), and the optional `P` is a
//! [`CsvProfile`] supplying every other setting (quoting, comments, mapping, writer columns,
//! limits, media policy). The defaults give `Csv = Csv<HasHeader, Comma, StandardCsv>`.
//!
//! ```
//! use concord_extras::csv::{Comma, Csv, CsvProfile, CsvProfileOptions, Delimiter, HasHeader,
//!                           NoHeader, Semicolon, Tab};
//!
//! /// A provider dialect: `#` comment lines, and a header that must be exactly `id;name`.
//! pub struct ProviderDialect;
//!
//! impl CsvProfile for ProviderDialect {
//!     fn options() -> CsvProfileOptions {
//!         CsvProfileOptions::default()
//!             .comment(Some(b'#'))
//!             .columns(["id", "name"])
//!             .max_record_bytes(16 * 1024 * 1024)
//!     }
//! }
//!
//! type StandardExport = Csv; // Csv<HasHeader, Comma, StandardCsv>
//! type HeaderlessExport = Csv<NoHeader>;
//! type EuropeanExport = Csv<HasHeader, Semicolon>;
//! type PositionalTsv = Csv<NoHeader, Tab>;
//! type UnitSeparated = Csv<NoHeader, Delimiter<0x1f>>;
//! type ProviderExport = Csv<HasHeader, Semicolon, ProviderDialect>;
//!
//! // The type is the whole configuration; `options()` shows it as a runtime value.
//! assert_eq!(EuropeanExport::options().delimiter(), b';');
//! assert!(ProviderExport::reader().is_ok());
//! # let _ = (StandardExport::options(), HeaderlessExport::options(), PositionalTsv::options(),
//! #          UnitSeparated::options(), Csv::<HasHeader, Comma>::options());
//! ```
//!
//! A statement names the type like any other format: `response Csv<HasHeader, Semicolon>;`,
//! `request ProviderExport;`. A profile has no header or delimiter setting, so an alias cannot
//! contradict its own `H` and `D`:
//!
//! ```compile_fail,E0599
//! use concord_extras::csv::CsvProfileOptions;
//! let _ = CsvProfileOptions::default().delimiter(b';'); // no such setting: `D` is the delimiter
//! ```
//!
//! The header and delimiter policies are sealed: their states are already covered (`Delimiter`
//! takes any byte), and only a profile is yours to implement.
//!
//! ```compile_fail,E0277
//! use concord_extras::csv::HeaderPolicy;
//! struct Sometimes;
//! impl HeaderPolicy for Sometimes {
//!     const HAS_HEADER: bool = true;
//! }
//! ```
//!
//! # Routes
//!
//! | Route | Payload | Contract |
//! |---|---|---|
//! | [`Decode<Vec<T>>`](concord_core::Decode) | `T: DeserializeOwned` | Concord collects the whole body, then the rows are parsed; the limits below bound the parser, not the collection |
//! | [`Decode<RawTable>`](concord_core::Decode) | [`RawTable`] | the same, with byte records (no UTF-8 requirement) |
//! | [`Encode<Vec<T>>`](concord_core::Encode), [`Encode<Rows<I>>`](concord_core::Encode) | `T: Serialize` rows | encoded before sending into a finite body; an iterator does not make the HTTP body streaming |
//! | [`Encode<StreamRows<S>>`](concord_core::Encode) | a one-shot `Stream` of `Result<T, E>` | a lazy body: preparation polls the source zero times; rows are serialized as the transport asks |
//! | [`Encode<RawTable>`](concord_core::Encode) | [`RawTable`] | byte records, header from the table |
//! | [`CsvReader::open`] / [`read`](CsvReader::read) | `T: DeserializeOwned` | incremental: status and head checked before the body is polled, records framed from arbitrary chunks, owned rows yielded one at a time |
//! | [`CsvReader::open_raw`] | [`RawRecord`] | incremental byte records |
//! | [`CsvWriter::upload`] | [`StreamRows`] | a lazy body with the `Content-Type` of the runtime header mode |
//!
//! The finite and incremental routes share one framing and mapping implementation
//! (`csv-core` framing with limits, then `csv`'s `StringRecord` deserialization), so equal
//! options give equal table semantics.
//!
//! # Table semantics
//!
//! - `HasHeader`: the first **parsed record** is the header (it may contain quoted newlines);
//!   `NoHeader`: it is data. Nothing is guessed from the contents.
//! - Read mapping ([`ReadMapping`]): `Default` maps named struct fields from the header with
//!   `HasHeader`, and positionally with `NoHeader` (structs by declaration order, tuples, `Vec`);
//!   `Positional` ignores the header names; `Columns(names)` names the positions of a headerless
//!   table. A supplied column list with `HasHeader` is refused as ambiguous: use the profile's
//!   [`columns`](CsvProfileOptions::columns) to **validate** the wire header instead.
//! - Named mapping refuses a header with an empty or repeated name; positional and raw reading
//!   keep them. Missing, defaulted and unknown struct fields follow the row's Serde derive; the
//!   header's column order is free.
//! - Every record has the width of the header (or of the supplied columns, or of the first record
//!   of a headerless table), unless [`flexible`](CsvProfileOptions::flexible).
//! - Header-required empty input (blank or comment lines only included) is a missing-header
//!   error; header-only input is zero rows; headerless empty input is zero rows.
//! - Typed rows are UTF-8 (an invalid sequence anywhere in the record is a
//!   [`Format`](crate::stream::ReadErrorKind::Format) error); raw records keep bytes. One leading
//!   UTF-8 byte order mark is stripped by default ([`BomPolicy`]).
//! - Input record terminators: `\r\n`, `\n` and `\r`; blank lines are skipped (`csv-core`'s
//!   grammar). A final record needs no terminator. `csv-core` is permissive rather than a strict
//!   RFC 4180 validator: a stray quote inside an unquoted field is data, and input that ends
//!   inside a quoted field yields that field.
//!
//! # Defaults
//!
//! | Setting | Default |
//! |---|---|
//! | Header / delimiter | `HasHeader`, comma (from the type) |
//! | Quote / escape | `"`, doubled quotes, no escape byte |
//! | Output terminator | `\r\n` on every platform ([`LineEnding`]) |
//! | Whitespace / comments | preserved; no comment byte |
//! | BOM | strip one leading UTF-8 BOM; never written |
//! | Limits | [`DEFAULT_MAX_RECORD_BYTES`] per data record, [`DEFAULT_MAX_HEADER_BYTES`] for the header, [`DEFAULT_MAX_FIELDS`] fields; no total byte or record cap |
//! | Media | one `text/csv` `Content-Type`, charset absent or UTF-8 ([`MediaPolicy`]) |
//!
//! The record and header limits count the unescaped field bytes of one record, the
//! representation this module owns while framing it: the buffer never grows past them, and a
//! larger record is a [`Limit`](crate::stream::ReadErrorKind::Limit) error, never a truncation.
//! They do not bound the transport's chunks, Concord's collected body on the finite route, your
//! accumulated rows or your own `Deserialize` code. The total limits report a `Limit` error, never
//! a clean end.
//!
//! # Writing
//!
//! `HasHeader` writes one header: the Serde field names of the rows (`serde(rename)` respected)
//! or explicit columns ([`Rows::with_columns`], [`StreamRows::with_columns`], or the profile's
//! [`columns`](CsvProfileOptions::columns); two different lists are refused). Rows without field
//! names (tuples, sequences, [`RawRecord`]) need explicit columns, which then label their
//! positions; named rows must emit exactly the columns, in order (columns never reorder fields).
//! An empty `HasHeader` table needs explicit columns. `NoHeader` never writes a header. Every
//! row's field names are captured and compared with the first row's, so a row that changes its
//! fields (a conditional `Serialize`) is refused even at the same width.
//!
//! A lazy body ([`Encode<StreamRows<S>>`](concord_core::Encode), [`CsvWriter::upload`]) fails
//! **after** transmission started when a later row cannot be written (serialization, changed
//! fields, width, size), when the source fails, or when a header-required source ends empty
//! without columns: the transfer fails with the bytes already sent; nothing is retried or rolled
//! back.
//!
//! # Head checks
//!
//! By default a response needs exactly one `Content-Type`, `text/csv`, whose charset is absent or
//! UTF-8 (`us-ascii`), and whose `header` parameter, if present, is `present` or `absent`, given
//! once, and agrees with `H`. These are `Content-Type` checks
//! ([`check_content_type`](concord_core::Decode::check_content_type)), so
//! [`IgnoreContentType<Csv<..>>`](crate::IgnoreContentType) skips them, and nothing else: the
//! configuration is validated in [`context`](concord_core::Decode::context), and the table rules
//! while the body is parsed. A profile relaxes the media rule with [`MediaPolicy`].

mod codec;
mod frame;
mod names;
mod options;
mod read;
mod record;
mod write;

use std::fmt;
use std::marker::PhantomData;

pub use codec::CsvContext;
pub use options::{
    BomPolicy, CsvConfigError, CsvOptions, CsvProfileOptions, DEFAULT_MAX_FIELDS,
    DEFAULT_MAX_HEADER_BYTES, DEFAULT_MAX_RECORD_BYTES, HeaderMode, LineEnding, MediaPolicy,
    QuoteStyle, ReadMapping, RecordTerminator, Trim,
};
pub use read::{CsvReader, CsvRecords};
pub use record::{RawRecord, RawTable};
pub use write::{CsvWriter, Rows, StreamRows};

/// The CSV format: header policy `H`, delimiter `D`, profile `P`.
///
/// `Csv` alone is `Csv<HasHeader, Comma, StandardCsv>`. See the [module documentation](self) for
/// the routes, the table semantics and the defaults.
pub struct Csv<H = HasHeader, D = Comma, P = StandardCsv>(Policies<H, D, P>);

/// `fn() -> ..`: no ownership of, and no auto-trait dependence on, the marker types.
type Policies<H, D, P> = PhantomData<fn() -> (H, D, P)>;

impl<H: HeaderPolicy, D: DelimiterPolicy, P: CsvProfile> Csv<H, D, P> {
    /// The `Content-Type` the static encoder sends: `text/csv; header=present` or
    /// `text/csv; header=absent`, from `H`.
    pub const CONTENT_TYPE: &'static str = content_type(H::HAS_HEADER);

    /// The policies of this type as a runtime value: `H` and `D` with the profile's options. An
    /// explicit conversion, for code that then edits them (a job that changes a limit, say).
    pub fn options() -> CsvOptions {
        CsvOptions::with_profile(header_mode::<H>(), D::BYTE, P::options())
    }

    /// An incremental reader of this dialect.
    ///
    /// # Errors
    ///
    /// A [`CsvConfigError`] when the delimiter, the profile or both are not a valid dialect
    /// (a `\n` delimiter, a delimiter equal to the quote, ...). Nothing is read.
    pub fn reader() -> Result<CsvReader, CsvConfigError> {
        CsvReader::new(Self::options())
    }

    /// A writer of this dialect.
    ///
    /// # Errors
    ///
    /// A [`CsvConfigError`] for an invalid dialect, as [`reader`](Self::reader).
    pub fn writer() -> Result<CsvWriter, CsvConfigError> {
        CsvWriter::new(Self::options())
    }
}

impl<H, D, P> fmt::Debug for Csv<H, D, P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Csv")
    }
}

pub(crate) const fn content_type(has_header: bool) -> &'static str {
    if has_header {
        "text/csv; header=present"
    } else {
        "text/csv; header=absent"
    }
}

fn header_mode<H: HeaderPolicy>() -> HeaderMode {
    if H::HAS_HEADER {
        HeaderMode::Present
    } else {
        HeaderMode::Absent
    }
}

mod sealed {
    pub trait Sealed {}
}

/// Whether the first parsed record is a header: [`HasHeader`] or [`NoHeader`]. Sealed.
pub trait HeaderPolicy: sealed::Sealed + 'static {
    /// `true` for [`HasHeader`].
    const HAS_HEADER: bool;
}

/// The field delimiter byte: [`Delimiter<BYTE>`]. Sealed; `Delimiter` takes any byte.
pub trait DelimiterPolicy: sealed::Sealed + 'static {
    /// The delimiter.
    const BYTE: u8;
}

/// The first parsed record is the header.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct HasHeader;

/// Every parsed record is data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct NoHeader;

/// A single-byte field delimiter. Any ASCII byte other than `\r`, `\n` and the dialect's quote,
/// escape and comment bytes is valid; a byte that is not (`Delimiter<b'\n'>` is a well-formed
/// type) is a [`CsvConfigError`] before anything is read or sent. Multi-byte separators are not
/// supported: a byte above `0x7f` is refused (it would split UTF-8 text).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Delimiter<const BYTE: u8>;

/// `,`
pub type Comma = Delimiter<b','>;
/// `;`
pub type Semicolon = Delimiter<b';'>;
/// `\t`
pub type Tab = Delimiter<b'\t'>;
/// `|`
pub type Pipe = Delimiter<b'|'>;

impl sealed::Sealed for HasHeader {}
impl sealed::Sealed for NoHeader {}
impl<const BYTE: u8> sealed::Sealed for Delimiter<BYTE> {}

impl HeaderPolicy for HasHeader {
    const HAS_HEADER: bool = true;
}

impl HeaderPolicy for NoHeader {
    const HAS_HEADER: bool = false;
}

impl<const BYTE: u8> DelimiterPolicy for Delimiter<BYTE> {
    const BYTE: u8 = BYTE;
}

/// Every CSV setting other than the header policy and the delimiter: implement it on a type of
/// yours for a provider dialect (`Csv<HasHeader, Semicolon, ProviderDialect>`).
///
/// The options are resolved once per reader, writer or call, never from mutable global state.
pub trait CsvProfile: 'static {
    /// The options; the defaults unless overridden.
    fn options() -> CsvProfileOptions {
        CsvProfileOptions::default()
    }
}

/// The default profile: [`CsvProfileOptions::default`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct StandardCsv;

impl CsvProfile for StandardCsv {}
