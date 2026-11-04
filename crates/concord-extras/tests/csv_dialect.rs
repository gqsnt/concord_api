//! Dialects, profiles and configuration validation (CSV.md §2, §4, §6, §8 items 1, 4, 5, 8, 9):
//! the aliases and an SDK-defined profile compile and resolve to their policies, invalid
//! dialects fail before any body or source polling, delimiters/quotes/escapes/CR-LF/comments/BOM
//! /trim/terminators behave as documented, and static aliases equal runtime options.

#![cfg(feature = "csv")]

mod csv_support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use concord_core::{Decode, Encode, ResponseHead};
use concord_extras::IgnoreContentType;
use concord_extras::csv::{
    BomPolicy, Comma, Csv, CsvConfigError, CsvOptions, CsvProfile, CsvProfileOptions, CsvReader,
    CsvWriter, Delimiter, HasHeader, HeaderMode, NoHeader, Pipe, ReadMapping, RecordTerminator,
    Rows, Semicolon, StreamRows, Tab, Trim,
};
use concord_extras::stream::{Chunks, ReadErrorKind};
use csv_support::{Counting, Outcome, streamed, streamed_raw};

/// An SDK-defined third profile (item 1): an external crate implements the open trait.
struct ProviderDialect;
impl CsvProfile for ProviderDialect {
    fn options() -> CsvProfileOptions {
        CsvProfileOptions::default()
            .comment(Some(b'#'))
            .trim(Trim::Fields)
            .max_record_bytes(16 * 1024 * 1024)
    }
}

type StandardExport = Csv;
type HeaderlessExport = Csv<NoHeader>;
type EuropeanExport = Csv<HasHeader, Semicolon>;
type PositionalTsv = Csv<NoHeader, Tab>;
type UnitSeparated = Csv<NoHeader, Delimiter<0x1f>>;
type ProviderExport = Csv<HasHeader, Semicolon, ProviderDialect>;

#[test]
fn aliases_resolve_to_their_header_and_delimiter_in_the_authored_order() {
    let cases: [(CsvOptions, HeaderMode, u8); 7] = [
        (StandardExport::options(), HeaderMode::Present, b','),
        (HeaderlessExport::options(), HeaderMode::Absent, b','),
        (EuropeanExport::options(), HeaderMode::Present, b';'),
        (PositionalTsv::options(), HeaderMode::Absent, b'\t'),
        (UnitSeparated::options(), HeaderMode::Absent, 0x1f),
        (ProviderExport::options(), HeaderMode::Present, b';'),
        (Csv::<NoHeader, Pipe>::options(), HeaderMode::Absent, b'|'),
    ];
    for (options, header, delimiter) in cases {
        assert_eq!(options.header_mode(), header);
        assert_eq!(options.delimiter(), delimiter);
    }
    // The profile supplies everything else, and nothing about H/D.
    assert_eq!(
        ProviderExport::options(),
        CsvOptions::with_profile(HeaderMode::Present, b';', ProviderDialect::options())
    );
    assert_eq!(
        Csv::<NoHeader, Comma, ProviderDialect>::options().header_mode(),
        HeaderMode::Absent
    );
    for options in [
        StandardExport::options(),
        UnitSeparated::options(),
        ProviderExport::options(),
    ] {
        assert!(CsvReader::new(options.clone()).is_ok());
        assert!(CsvWriter::new(options).is_ok());
    }
}

#[test]
fn the_static_type_is_the_whole_finite_format() {
    // Trait coherence: every route is a separate impl on the same type.
    fn decodes<F: Decode<Vec<(u8, String)>> + Decode<concord_extras::csv::RawTable>>() {}
    fn encodes<F>()
    where
        F: Encode<Vec<(u8, u8)>>
            + Encode<Rows<std::vec::IntoIter<(u8, u8)>>>
            + Encode<concord_extras::csv::RawTable>
            + Encode<
                StreamRows<
                    concord_extras::stream::FromIter<
                        std::vec::IntoIter<Result<u8, std::io::Error>>,
                    >,
                >,
            >,
    {
    }
    decodes::<StandardExport>();
    decodes::<UnitSeparated>();
    decodes::<ProviderExport>();
    decodes::<IgnoreContentType<EuropeanExport>>();
    encodes::<StandardExport>();
    encodes::<PositionalTsv>();
    encodes::<ProviderExport>();

    // Finite routes need no `Clone`, `Send`, `'static` or `Unpin` on rows.
    struct Borrowing<'a>(&'a str, std::marker::PhantomPinned, std::rc::Rc<()>);
    impl serde::Serialize for Borrowing<'_> {
        fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            s.serialize_str(self.0)
        }
    }
    let text = String::from("local");
    let rows = vec![Borrowing(
        &text,
        std::marker::PhantomPinned,
        std::rc::Rc::new(()),
    )];
    let body = <Csv<NoHeader> as Encode<Vec<Borrowing<'_>>>>::encode(rows).unwrap();
    assert_eq!(body.as_bytes(), Some(&b"local\r\n"[..]));
}

fn config_error(options: CsvOptions) -> &'static str {
    let reader = CsvReader::new(options.clone()).unwrap_err();
    let writer = CsvWriter::new(options).unwrap_err();
    assert_eq!(reader, writer);
    reader.label()
}

#[test]
fn invalid_dialects_are_configuration_errors() {
    let present = |d| CsvOptions::new(HeaderMode::Present, d);
    let cases: Vec<(CsvOptions, &str)> = vec![
        (
            present(b'\n'),
            "the CSV delimiter is a record terminator (\\r or \\n)",
        ),
        (
            present(b'\r'),
            "the CSV delimiter is a record terminator (\\r or \\n)",
        ),
        (present(b'"'), "the CSV delimiter equals the quote"),
        (
            present(b';').quote(b';'),
            "the CSV delimiter equals the quote",
        ),
        (
            present(b',').quote(b'\n'),
            "the CSV quote is a record terminator (\\r or \\n)",
        ),
        (
            present(0xA7),
            "a CSV delimiter, quote, escape, comment or terminator must be one ASCII byte (multi-byte separators are not supported)",
        ),
        (
            present(b',').escape(Some(b',')),
            "the CSV escape equals the delimiter",
        ),
        (
            present(b',').escape(Some(b'"')),
            "the CSV escape equals the quote (use double_quote)",
        ),
        (
            present(b',').escape(Some(b'\n')),
            "the CSV escape is a record terminator (\\r or \\n)",
        ),
        (
            present(b',').double_quote(false),
            "CSV double_quote(false) needs an escape byte",
        ),
        (
            present(b',').comment(Some(b',')),
            "the CSV comment byte equals the delimiter, the quote or the escape",
        ),
        (
            present(b',').comment(Some(b'"')),
            "the CSV comment byte equals the delimiter, the quote or the escape",
        ),
        (
            present(b',').comment(Some(b'\r')),
            "the CSV comment byte is a record terminator (\\r or \\n)",
        ),
        (
            present(b',').read_terminator(RecordTerminator::Byte(b',')),
            "the CSV record terminator equals the delimiter, the quote, the escape or the comment byte",
        ),
        (
            present(b',').max_record_bytes(0),
            "a CSV record, header or field limit is zero",
        ),
        (
            present(b',').max_fields(0),
            "a CSV record, header or field limit is zero",
        ),
        (
            present(b',').columns(["a", "a"]),
            "a CSV column list repeats a name",
        ),
        (present(b',').columns([""; 0]), "a CSV column list is empty"),
        (
            present(b',').max_fields(1).columns(["a", "b"]),
            "a CSV column list has more names than the field limit",
        ),
        (
            CsvOptions::new(HeaderMode::Absent, b',')
                .columns(["a", "b"])
                .read_mapping(ReadMapping::columns(["b", "a"])),
            "ReadMapping::Columns conflicts with the profile's columns",
        ),
    ];
    for (options, label) in cases {
        assert_eq!(config_error(options.clone()), label, "{options:?}");
    }
    // Other single bytes are valid delimiters, not only the four aliases.
    for byte in [b' ', b'^', b'`', 0x01, 0x1f, 0x7f, b'#'] {
        assert!(
            CsvReader::new(CsvOptions::new(HeaderMode::Absent, byte)).is_ok(),
            "{byte}"
        );
    }
    let shown = CsvConfigError::clone(&CsvReader::new(present(b'\n')).unwrap_err());
    assert_eq!(
        shown.to_string(),
        "invalid CSV configuration: the CSV delimiter is a record terminator (\\r or \\n)"
    );
}

/// `Delimiter<b'\n'>` is a well-formed type; using it fails before anything is polled.
#[tokio::test]
async fn an_invalid_static_dialect_fails_before_any_polling() {
    type Newline = Csv<HasHeader, Delimiter<b'\n'>>;
    assert!(Newline::reader().is_err());
    assert!(Newline::writer().is_err());

    // Encode of a one-shot source: a pre-send error, the source polled zero times.
    let polls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&polls);
    let source = concord_extras::stream::from_iter((0..3).map(move |i| {
        counted.fetch_add(1, Ordering::SeqCst);
        Ok::<_, std::io::Error>((i, i))
    }));
    let error = <Newline as Encode<StreamRows<_>>>::encode(StreamRows::new(source)).unwrap_err();
    assert_eq!(
        error.label(),
        "the CSV delimiter is a record terminator (\\r or \\n)"
    );
    assert_eq!(polls.load(Ordering::SeqCst), 0);
    assert!(<Newline as Encode<Vec<(u8, u8)>>>::encode(vec![(1, 2)]).is_err());

    // Decode: `context` refuses before the body is read; the deliberate media override does not
    // bypass it.
    let headers = concord_core::reqwest::header::HeaderMap::new();
    let head = ResponseHead::new(
        concord_core::reqwest::StatusCode::OK,
        concord_core::reqwest::Version::HTTP_11,
        &headers,
    );
    let label = <Newline as Decode<Vec<Vec<String>>>>::context(&head).unwrap_err();
    assert_eq!(
        label.label(),
        "the CSV delimiter is a record terminator (\\r or \\n)"
    );
    assert!(
        <IgnoreContentType<Newline> as Decode<Vec<Vec<String>>>>::check_content_type(&head).is_ok()
    );
    assert!(<IgnoreContentType<Newline> as Decode<Vec<Vec<String>>>>::context(&head).is_err());

    // The runtime reader: no reader, so no source is ever polled.
    let (source, polls) = Counting::new([b"a\nb\n".to_vec()]);
    assert!(CsvReader::new(CsvOptions::new(HeaderMode::Absent, b'\n')).is_err());
    drop(source);
    assert_eq!(polls.load(Ordering::SeqCst), 0);
}

async fn outcome(options: CsvOptions, body: &[u8]) -> Outcome {
    let reader = CsvReader::new(options).unwrap();
    let whole = streamed(&reader, Chunks::new([body.to_vec()])).await;
    for chunks in csv_support::single_splits(body) {
        assert_eq!(streamed(&reader, Chunks::new(chunks)).await, whole);
    }
    whole
}

#[tokio::test]
async fn delimiters_quoted_separators_doubled_quotes_and_embedded_terminators() {
    let absent = |d| CsvOptions::new(HeaderMode::Absent, d);
    assert_eq!(
        outcome(
            absent(b','),
            b"a,\"b,c\",\"d\"\"e\"\r\n\"x\r\ny\",\"\",z\r\n"
        )
        .await,
        Outcome::ok(None, &[&["a", "b,c", "d\"e"], &["x\r\ny", "", "z"]])
    );
    assert_eq!(
        outcome(absent(b';'), b"a;\"b;c\";d,e\n").await,
        Outcome::ok(None, &[&["a", "b;c", "d,e"]])
    );
    assert_eq!(
        outcome(absent(b'\t'), b"a\t\"b\tc\"\t d \n").await,
        Outcome::ok(None, &[&["a", "b\tc", " d "]])
    );
    assert_eq!(
        outcome(absent(0x1f), b"a\x1f\"b\x1fc\"\x1f,\n").await,
        Outcome::ok(None, &[&["a", "b\x1fc", ","]])
    );
    // CR, LF and CRLF terminators mixed; blank lines skipped; no final terminator.
    assert_eq!(
        outcome(absent(b','), b"a,1\rb,2\n\nc,3\r\n\r\nd,4").await,
        Outcome::ok(None, &[&["a", "1"], &["b", "2"], &["c", "3"], &["d", "4"]])
    );
}

#[tokio::test]
async fn an_alternate_escape_dialect() {
    let options = CsvOptions::new(HeaderMode::Absent, b',')
        .double_quote(false)
        .escape(Some(b'\\'));
    assert_eq!(
        outcome(options.clone(), b"\"say \\\"hi\\\"\",x\n").await,
        Outcome::ok(None, &[&["say \"hi\"", "x"]])
    );
    // A single-quote dialect.
    assert_eq!(
        outcome(
            CsvOptions::new(HeaderMode::Absent, b',').quote(b'\''),
            b"'a,b','it''s',\"q\"\n"
        )
        .await,
        Outcome::ok(None, &[&["a,b", "it's", "\"q\""]])
    );
    // Quoting off: quotes are data.
    assert_eq!(
        outcome(
            CsvOptions::new(HeaderMode::Absent, b',').quoting(false),
            b"\"a,b\"\n"
        )
        .await,
        Outcome::ok(None, &[&["\"a", "b\""]])
    );
}

#[tokio::test]
async fn comments_are_off_by_default_and_a_profile_can_enable_them() {
    let body = b"id,name\n#1,hash\n1,a#b\n";
    // Default: `#` is data.
    assert_eq!(
        outcome(CsvOptions::new(HeaderMode::Present, b','), body).await,
        Outcome::ok(Some(&["id", "name"]), &[&["#1", "hash"], &["1", "a#b"]])
    );
    // Provider comments: a record starting with `#` is skipped; `#` inside a field is data.
    assert_eq!(
        outcome(
            ProviderExport::options().comment(Some(b'#')),
            b"# note\nid;name\n#1;hash\n1;a#b\n# trailer"
        )
        .await,
        Outcome::ok(Some(&["id", "name"]), &[&["1", "a#b"]])
    );
    // A quoted field starting with the comment byte is data.
    assert_eq!(
        outcome(
            CsvOptions::new(HeaderMode::Absent, b',').comment(Some(b'#')),
            b"\"#x\",1\n"
        )
        .await,
        Outcome::ok(None, &[&["#x", "1"]])
    );
}

#[tokio::test]
async fn bom_policies() {
    let bom = b"\xef\xbb\xbfid,name\n1,a\n";
    let present = || CsvOptions::new(HeaderMode::Present, b',');
    // Strip (default): exactly one.
    assert_eq!(
        outcome(present(), bom).await,
        Outcome::ok(Some(&["id", "name"]), &[&["1", "a"]])
    );
    assert_eq!(
        outcome(present(), b"\xef\xbb\xbf\xef\xbb\xbfid,name\n1,a\n")
            .await
            .headers,
        Some(vec!["\u{feff}id".to_owned(), "name".to_owned()])
    );
    // A partial BOM is data.
    let reader = CsvReader::new(CsvOptions::new(HeaderMode::Absent, b',')).unwrap();
    for body in [&b"\xef\xbbx\n"[..], b"\xefx\n", b"\xef\xbb"] {
        let raw = streamed_raw(&reader, Chunks::new(csv_support::byte_chunks(body))).await;
        let whole = streamed_raw(&reader, Chunks::new([body.to_vec()])).await;
        assert_eq!(raw, whole);
        assert_eq!(raw.records.len(), 1);
    }
    // Reject.
    let rejected = outcome(present().bom(BomPolicy::Reject), bom).await;
    assert_eq!(
        rejected.error,
        Some((
            Some(ReadErrorKind::Format),
            "the CSV input starts with a byte order mark, which this profile refuses"
        ))
    );
    assert_eq!(
        outcome(present().bom(BomPolicy::Reject), b"id\n1\n")
            .await
            .records,
        [["1"]]
    );
    // Preserve: the BOM belongs to the first field.
    assert_eq!(
        outcome(present().bom(BomPolicy::Preserve), bom)
            .await
            .headers,
        Some(vec!["\u{feff}id".to_owned(), "name".to_owned()])
    );
}

#[tokio::test]
async fn trim_terminator_and_flexible_settings() {
    let body = b" id , name \n 1 , a \n";
    assert_eq!(
        outcome(
            CsvOptions::new(HeaderMode::Present, b',').trim(Trim::All),
            body
        )
        .await,
        Outcome::ok(Some(&["id", "name"]), &[&["1", "a"]])
    );
    assert_eq!(
        outcome(
            CsvOptions::new(HeaderMode::Present, b',').trim(Trim::Headers),
            body
        )
        .await,
        Outcome::ok(Some(&["id", "name"]), &[&[" 1 ", " a "]])
    );
    assert_eq!(
        outcome(
            CsvOptions::new(HeaderMode::Present, b',').trim(Trim::Fields),
            body
        )
        .await,
        Outcome::ok(Some(&[" id ", " name "]), &[&["1", "a"]])
    );
    // A custom terminator: `\n` is then data.
    assert_eq!(
        outcome(
            CsvOptions::new(HeaderMode::Absent, b',').read_terminator(RecordTerminator::Byte(b'~')),
            b"a,b\nc~d,e~"
        )
        .await,
        Outcome::ok(None, &[&["a", "b\nc"], &["d", "e"]])
    );
    // Flexible widths.
    assert_eq!(
        outcome(
            CsvOptions::new(HeaderMode::Absent, b',').flexible(true),
            b"a,b\nc\nd,e,f\n"
        )
        .await,
        Outcome::ok(None, &[&["a", "b"], &["c"], &["d", "e", "f"]])
    );
    assert_eq!(
        outcome(CsvOptions::new(HeaderMode::Absent, b','), b"a,b\nc\n").await,
        Outcome::ok(None, &[&["a", "b"]]).failing(
            ReadErrorKind::Format,
            "a CSV record does not have as many fields as the first record"
        )
    );
}

#[tokio::test]
async fn a_profile_and_the_equivalent_runtime_options_read_and_write_equally() {
    let body = b"# export\nid;name\n 1 ; a \n2;\"b;c\"\n";
    let fixed = ProviderExport::reader().unwrap();
    let runtime = CsvReader::new(
        CsvOptions::new(HeaderMode::Present, b';')
            .comment(Some(b'#'))
            .trim(Trim::Fields)
            .max_record_bytes(16 * 1024 * 1024),
    )
    .unwrap();
    assert_eq!(fixed.options(), runtime.options());
    let a = streamed(&fixed, Chunks::new([body.to_vec()])).await;
    let b = streamed(&runtime, Chunks::new([body.to_vec()])).await;
    assert_eq!(a, b);
    assert_eq!(
        a,
        Outcome::ok(Some(&["id", "name"]), &[&["1", "a"], &["2", "b;c"]])
    );

    let rows = vec![(1, "a;b"), (2, "c")];
    let w1 = ProviderExport::writer()
        .unwrap()
        .to_vec(Rows::new(rows.clone()).with_columns(["id", "name"]))
        .unwrap();
    let w2 = CsvWriter::new(ProviderExport::options())
        .unwrap()
        .to_vec(Rows::new(rows).with_columns(["id", "name"]))
        .unwrap();
    assert_eq!(w1, w2);
    assert_eq!(w1, b"id;name\r\n1;\"a;b\"\r\n2;c\r\n");
}
