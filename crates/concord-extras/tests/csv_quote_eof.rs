//! Input that ends inside a quoted field (an opening quote never closed) is a terminal `Format`
//! error on the finite and the incremental routes: never a record, never a clean end of input
//! (wave 4, Open-Meteo friction F4). `csv-core` ends such a field silently; the shared framer
//! follows the parser's quote state for the configured dialect and refuses it. A closed quoted
//! field at the end of input without a final terminator stays valid.

#![cfg(feature = "csv")]

mod csv_support;

use concord_core::Decode;
use concord_core::bytes::Bytes;
use concord_extras::csv::{
    BomPolicy, Comma, Csv, CsvContext, CsvOptions, CsvProfile, CsvProfileOptions, CsvReader,
    HasHeader, HeaderMode, NoHeader, RawTable, RecordTerminator, StandardCsv,
};
use concord_extras::stream::{Chunks, Completion, ReadErrorKind};
use csv_support::{
    Outcome, byte_chunks, double_splits, finite, random_partitions, single_splits, streamed,
    streamed_raw,
};

const OPEN: &str = "the CSV input ends inside a quoted field";

/// Open-Meteo's hourly CSV shape, cut inside the last quoted value.
const OPEN_METEO: &[u8] = b"time,temperature_2m\n2026-01-01T00:00,\"12.7\n";

#[derive(Debug, PartialEq, serde::Deserialize)]
struct Hourly {
    time: String,
    temperature_2m: f64,
}

/// A backslash-escape dialect (no doubled quotes).
struct Backslash;
impl CsvProfile for Backslash {
    fn options() -> CsvProfileOptions {
        CsvProfileOptions::default()
            .double_quote(false)
            .escape(Some(b'\\'))
    }
}

/// A custom record terminator, with comment lines.
struct Tilde;
impl CsvProfile for Tilde {
    fn options() -> CsvProfileOptions {
        CsvProfileOptions::default()
            .read_terminator(RecordTerminator::Byte(b'~'))
            .comment(Some(b'#'))
    }
}

/// Comment lines with the standard terminators.
struct Comments;
impl CsvProfile for Comments {
    fn options() -> CsvProfileOptions {
        CsvProfileOptions::default().comment(Some(b'#'))
    }
}

/// Quoting off: a quote is data.
struct Unquoted;
impl CsvProfile for Unquoted {
    fn options() -> CsvProfileOptions {
        CsvProfileOptions::default().quoting(false)
    }
}

/// Every partition the CSV tests use: each split point, each two-split partition, one byte per
/// chunk and pseudo-random partitions.
fn partitions(body: &[u8]) -> Vec<Vec<Vec<u8>>> {
    let mut out = single_splits(body);
    out.extend(double_splits(body));
    out.push(byte_chunks(body));
    out.extend(random_partitions(body, 32, 0xc5f ^ body.len() as u64));
    out
}

/// The incremental outcome of `body` with the options of the static format `F` (typed
/// `Vec<String>` rows and raw records), asserted equal on every partition, and the finite outcome
/// of the same bytes through `F`, asserted equal (an error without partial rows).
async fn read<F>(body: &[u8]) -> Outcome
where
    F: Decode<Vec<Vec<String>>, Context = CsvContext>
        + Decode<RawTable, Context = CsvContext>
        + OptionsOf,
{
    let reader = CsvReader::new(F::options()).unwrap();
    let whole = streamed(&reader, Chunks::new([body.to_vec()])).await;
    let raw = streamed_raw(&reader, Chunks::new([body.to_vec()])).await;
    assert_eq!(raw, whole, "raw records");
    for chunks in partitions(body) {
        assert_eq!(
            streamed(&reader, Chunks::new(chunks.clone())).await,
            whole,
            "{chunks:?}"
        );
        assert_eq!(
            streamed_raw(&reader, Chunks::new(chunks.clone())).await,
            whole,
            "raw {chunks:?}"
        );
    }
    let expected = whole.clone().labels_only();
    let got = finite::<F>(body);
    if expected.error.is_some() {
        assert_eq!(got.error, expected.error);
        assert!(got.records.is_empty());
    } else {
        assert_eq!(got, expected);
    }
    whole
}

/// The runtime options of a static format.
trait OptionsOf {
    fn options() -> CsvOptions;
}
impl<H, D, P> OptionsOf for Csv<H, D, P>
where
    H: concord_extras::csv::HeaderPolicy,
    D: concord_extras::csv::DelimiterPolicy,
    P: CsvProfile,
{
    fn options() -> CsvOptions {
        Csv::<H, D, P>::options()
    }
}

#[test]
fn open_meteo_bytes_are_a_format_error_on_the_finite_route() {
    let context = || CsvContext::of::<HasHeader, Comma, StandardCsv>().unwrap();
    let typed =
        <Csv as Decode<Vec<Hourly>>>::decode_body(context(), Bytes::from(OPEN_METEO)).unwrap_err();
    assert_eq!(typed.label(), OPEN);
    assert_eq!(typed.get("record"), Some(1));
    let raw =
        <Csv as Decode<RawTable>>::decode_body(context(), Bytes::from(OPEN_METEO)).unwrap_err();
    assert_eq!(raw.label(), OPEN);
    assert_eq!(raw.get("record"), Some(1));
    // Diagnostics name the position, never the cell.
    assert!(!format!("{typed} {typed:?}").contains("12.7"));

    // The same value closed is a row, with or without a final terminator.
    for body in [
        &b"time,temperature_2m\n2026-01-01T00:00,\"12.7\""[..],
        b"time,temperature_2m\n2026-01-01T00:00,\"12.7\"\n",
    ] {
        let rows =
            <Csv as Decode<Vec<Hourly>>>::decode_body(context(), Bytes::copy_from_slice(body))
                .unwrap();
        assert_eq!(
            rows,
            [Hourly {
                time: "2026-01-01T00:00".into(),
                temperature_2m: 12.7
            }]
        );
    }
}

#[tokio::test]
async fn open_meteo_bytes_fail_the_incremental_reader_instead_of_ending_it() {
    let reader = CsvReader::new(CsvOptions::new(HeaderMode::Present, b',')).unwrap();
    for chunks in partitions(OPEN_METEO) {
        let mut rows = reader.read::<Hourly, _>(Chunks::new(chunks.clone()));
        let error = rows.next().await.unwrap().unwrap_err();
        assert_eq!(error.kind(), ReadErrorKind::Format, "{chunks:?}");
        assert!(error.is_terminal());
        let codec = error.codec().unwrap();
        assert_eq!(codec.label(), OPEN);
        assert_eq!(codec.get("record"), Some(1));
        assert_eq!(error.position().records, 0);
        assert_eq!(rows.completion(), Completion::Failed);
        assert!(rows.next().await.is_none());
        assert_eq!(rows.completion(), Completion::Failed);
        assert_eq!(
            rows.headers()
                .map(|h| h.iter().map(<[u8]>::to_vec).collect::<Vec<_>>()),
            Some(vec![b"time".to_vec(), b"temperature_2m".to_vec()])
        );
    }
    // A complete record before the truncated one is delivered; the end is still a failure.
    let body = b"time,temperature_2m\n2026-01-01T00:00,\"12.7\"\n2026-01-01T01:00,\"12";
    let mut rows = reader.read::<Hourly, _>(Chunks::new([body.to_vec()]));
    assert_eq!(rows.next().await.unwrap().unwrap().temperature_2m, 12.7);
    let error = rows.next().await.unwrap().unwrap_err();
    assert_eq!(
        error.codec().map(|c| (c.label(), c.get("record"))),
        Some((OPEN, Some(2)))
    );
    assert_eq!(error.position().records, 1);
    assert_eq!(rows.completion(), Completion::Failed);
    // Closed without a final terminator: a clean end.
    let body = b"time,temperature_2m\n2026-01-01T00:00,\"12.7\"";
    let mut rows = reader.read::<Hourly, _>(Chunks::new([body.to_vec()]));
    assert_eq!(rows.next().await.unwrap().unwrap().temperature_2m, 12.7);
    assert!(rows.next().await.is_none());
    assert_eq!(rows.completion(), Completion::EndOfInput);
}

#[tokio::test]
async fn the_open_meteo_outcome_is_the_same_on_every_partition_and_route() {
    assert_eq!(
        read::<Csv>(OPEN_METEO).await,
        Outcome::ok(Some(&["time", "temperature_2m"]), &[]).failing(ReadErrorKind::Format, OPEN)
    );
}

#[tokio::test]
async fn a_header_that_ends_inside_a_quoted_field() {
    for body in [&b"id,\"name"[..], b"\"id,name\n", b"\xef\xbb\xbf\"id"] {
        assert_eq!(
            read::<Csv>(body).await,
            Outcome::ok(None, &[]).failing(ReadErrorKind::Format, OPEN),
            "{body:?}"
        );
    }
    let reader = CsvReader::new(CsvOptions::new(HeaderMode::Present, b',')).unwrap();
    let mut rows = reader.read::<Vec<String>, _>(Chunks::new(["id,\"na", "me"]));
    let error = rows.next().await.unwrap().unwrap_err();
    // The header has no data record number, like every other header error.
    assert_eq!(
        error.codec().map(|c| (c.label(), c.get("record"))),
        Some((OPEN, None))
    );
    assert_eq!(rows.completion(), Completion::Failed);
    // Closed, without a final terminator: a header-only table.
    assert_eq!(
        read::<Csv>(b"id,\"name\"").await,
        Outcome::ok(Some(&["id", "name"]), &[])
    );
}

#[tokio::test]
async fn doubled_quotes_close_or_reopen_the_field() {
    let cases: [(&[u8], Outcome); 5] = [
        (b"\"a\"\"b\"", Outcome::ok(None, &[&["a\"b"]])),
        (b"\"a\"\"b\"\n", Outcome::ok(None, &[&["a\"b"]])),
        (b"\"a\"\"\"", Outcome::ok(None, &[&["a\""]])),
        (b"\"\"", Outcome::ok(None, &[&[""]])),
        (
            b"\"a\"\"",
            Outcome::ok(None, &[]).failing(ReadErrorKind::Format, OPEN),
        ),
    ];
    for (body, expected) in cases {
        assert_eq!(read::<Csv<NoHeader>>(body).await, expected, "{body:?}");
    }
    assert_eq!(
        read::<Csv<NoHeader>>(b"x\n\"a\"\"").await,
        Outcome::ok(None, &[&["x"]]).failing(ReadErrorKind::Format, OPEN)
    );
}

#[tokio::test]
async fn an_escape_byte_dialect() {
    let cases: [(&[u8], Outcome); 5] = [
        (b"\"a\\\"b\"", Outcome::ok(None, &[&["a\"b"]])),
        (b"\"a\\\\\"", Outcome::ok(None, &[&["a\\"]])),
        // The escaped quote does not close the field; an escape at the very end leaves it open.
        (
            b"\"a\\\"",
            Outcome::ok(None, &[]).failing(ReadErrorKind::Format, OPEN),
        ),
        (
            b"\"a\\",
            Outcome::ok(None, &[]).failing(ReadErrorKind::Format, OPEN),
        ),
        // No doubled quotes here: the second quote is data after the closed field.
        (b"\"a\"\"", Outcome::ok(None, &[&["a\""]])),
    ];
    for (body, expected) in cases {
        assert_eq!(
            read::<Csv<NoHeader, Comma, Backslash>>(body).await,
            expected,
            "{body:?}"
        );
    }
}

#[tokio::test]
async fn quoting_disabled_reads_a_lone_quote_as_data() {
    assert_eq!(
        read::<Csv<NoHeader, Comma, Unquoted>>(b"a,\"b\n\"").await,
        Outcome::ok(None, &[&["a", "\"b"]]).failing(
            ReadErrorKind::Format,
            "a CSV record does not have as many fields as the first record"
        )
    );
    assert_eq!(
        read::<Csv<NoHeader, Comma, Unquoted>>(b"a,\"b\n\",c").await,
        Outcome::ok(None, &[&["a", "\"b"], &["\"", "c"]])
    );
}

#[tokio::test]
async fn a_quote_inside_an_unquoted_field_is_data_not_an_open_quote() {
    assert_eq!(
        read::<Csv<NoHeader>>(b"ab\"c").await,
        Outcome::ok(None, &[&["ab\"c"]])
    );
    assert_eq!(
        read::<Csv<NoHeader>>(b"a\"\"b").await,
        Outcome::ok(None, &[&["a\"\"b"]])
    );
    // Lenient, as before: the quote is data, and the record ends normally.
    assert_eq!(
        read::<Csv<NoHeader>>(b"a\"b,\"c\"\nd\",e").await,
        Outcome::ok(None, &[&["a\"b", "c"], &["d\"", "e"]])
    );
}

#[tokio::test]
async fn terminators_inside_quotes_are_data_up_to_the_end_of_input() {
    // CRLF inside a closed and an open quoted field.
    assert_eq!(
        read::<Csv<NoHeader>>(b"\"a\r\nb\"\r\n\"c\r\n").await,
        Outcome::ok(None, &[&["a\r\nb"]]).failing(ReadErrorKind::Format, OPEN)
    );
    assert_eq!(
        read::<Csv<NoHeader>>(b"\"a\r\nb\"\r\n\"c\r\n\"").await,
        Outcome::ok(None, &[&["a\r\nb"], &["c\r\n"]])
    );
    assert_eq!(
        read::<Csv<NoHeader>>(b"\"a\rb\"\r\"c\r").await,
        Outcome::ok(None, &[&["a\rb"]]).failing(ReadErrorKind::Format, OPEN)
    );
    // A custom terminator inside quotes.
    assert_eq!(
        read::<Csv<NoHeader, Comma, Tilde>>(b"\"a~b\"~\"c~\n").await,
        Outcome::ok(None, &[&["a~b"]]).failing(ReadErrorKind::Format, OPEN)
    );
    assert_eq!(
        read::<Csv<NoHeader, Comma, Tilde>>(b"\"a~b\"~\"c~\n\"").await,
        Outcome::ok(None, &[&["a~b"], &["c~\n"]])
    );
}

#[tokio::test]
async fn quotes_in_comment_lines_are_comment_text() {
    // Quotes inside comment lines open nothing; a comment starts only at the start of a record.
    assert_eq!(
        read::<Csv<HasHeader, Comma, Comments>>(b"# \"note\nid,\"x\"\n# open \"\n1,2\n# \"").await,
        Outcome::ok(Some(&["id", "x"]), &[&["1", "2"]])
    );
    assert_eq!(
        read::<Csv<NoHeader, Comma, Comments>>(b"a,\"#\n# x").await,
        Outcome::ok(None, &[]).failing(ReadErrorKind::Format, OPEN)
    );
    assert_eq!(
        read::<Csv<NoHeader, Comma, Comments>>(b"\"#\"\n#\"").await,
        Outcome::ok(None, &[&["#"]])
    );
    // With a custom terminator, `csv-core`'s DFA ends a comment line only at the end of input.
    assert_eq!(
        read::<Csv<NoHeader, Comma, Tilde>>(b"a~# \"~\n\"b~").await,
        Outcome::ok(None, &[&["a"]])
    );
}

#[tokio::test]
async fn bom_policies_go_through_the_same_check() {
    let open = Outcome::ok(None, &[]).failing(ReadErrorKind::Format, OPEN);
    let reader = |bom| CsvReader::new(CsvOptions::new(HeaderMode::Absent, b',').bom(bom)).unwrap();
    for (bom, body, expected) in [
        (BomPolicy::Strip, &b"\xef\xbb\xbf\"a"[..], open.clone()),
        (
            BomPolicy::Strip,
            b"\xef\xbb\xbf\"a\"",
            Outcome::ok(None, &[&["a"]]),
        ),
        // Preserved, the BOM starts an unquoted field: the quote is data.
        (
            BomPolicy::Preserve,
            b"\xef\xbb\xbf\"a",
            Outcome::ok(None, &[&["\u{feff}\"a"]]),
        ),
        // A partial BOM is replayed as data through the parser.
        (
            BomPolicy::Strip,
            b"\xef\xbb",
            Outcome::ok(None, &[&["\u{fffd}"]]),
        ),
        // Only a leading BOM is one: later, it is data of an unquoted field.
        (
            BomPolicy::Strip,
            b"x\n\xef\xbb\xbf\"a",
            Outcome::ok(None, &[&["x"], &["\u{feff}\"a"]]),
        ),
        (
            BomPolicy::Strip,
            b"x\n\"\xef\xbb\xbf",
            Outcome::ok(None, &[&["x"]]).failing(ReadErrorKind::Format, OPEN),
        ),
    ] {
        let reader = reader(bom);
        for chunks in partitions(body) {
            let got = streamed_raw(&reader, Chunks::new(chunks.clone())).await;
            assert_eq!(got, expected.clone(), "{body:?} {chunks:?}");
        }
    }
    assert_eq!(read::<Csv<NoHeader>>(b"\xef\xbb\xbf\"a").await, open);
}
