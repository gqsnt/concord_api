//! Writer integrity (4.4 review findings): every row is serialized once and its field names are
//! checked in the serialization that is written (SOL-1); escape dialects write what their own
//! reader reads back, byte for byte (ASTRA-1); field and record limits stop a row's encoding
//! before an oversized record is buffered (ASTRA-2). The process-memory observation of ASTRA-2 is
//! `csv_write_memory.rs`; the buffer-capacity bounds are unit tests of the encoder.

#![cfg(feature = "csv")]

mod csv_support;

use std::cell::Cell;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::task::{Context, Poll, Waker};

use concord_codecs::csv::{
    Comma, Csv, CsvContext, CsvOptions, CsvProfile, CsvProfileOptions, CsvReader, CsvWriter,
    HasHeader, HeaderMode, LineEnding, NoHeader, QuoteStyle, RawRecord, RawTable, Rows, StreamRows,
};
use concord_codecs::stream::{Chunks, UploadError, from_iter};
use concord_core::{Decode, Encode, reqwest};
use http_body::Body as _;
use serde::ser::{SerializeSeq, SerializeStruct};
use serde::{Deserialize, Serialize, Serializer};

fn writer(options: CsvOptions) -> CsvWriter {
    CsvWriter::new(options).unwrap()
}

fn present() -> CsvOptions {
    CsvOptions::new(HeaderMode::Present, b',')
}

fn absent() -> CsvOptions {
    CsvOptions::new(HeaderMode::Absent, b',')
}

fn text(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap()
}

/// Polls a lazy body to its end on an always-ready source: the data frames' bytes, and the
/// transfer error if it failed.
fn drain(mut body: reqwest::Body) -> (Vec<u8>, Option<reqwest::Error>) {
    let mut cx = Context::from_waker(Waker::noop());
    let mut data = Vec::new();
    loop {
        match Pin::new(&mut body).poll_frame(&mut cx) {
            Poll::Ready(Some(Ok(frame))) => {
                if let Ok(bytes) = frame.into_data() {
                    data.extend_from_slice(&bytes);
                }
            }
            Poll::Ready(Some(Err(error))) => return (data, Some(error)),
            Poll::Ready(None) => return (data, None),
            Poll::Pending => unreachable!("an always-ready source"),
        }
    }
}

/// The `UploadError` in an error chain.
fn upload_error(error: &reqwest::Error) -> &UploadError {
    let mut current: &(dyn std::error::Error + 'static) = error;
    loop {
        if let Some(found) = current.downcast_ref::<UploadError>() {
            return found;
        }
        current = current.source().expect("an UploadError in the chain");
    }
}

// SOL-1: one serialization per record.

/// A row whose `Serialize` answers differently on every call: on its first call the field `id`
/// holding 7, on any later call the field `other` holding `MISLABELLED`. Serialized once per
/// record, the table is `id` over 7; validating one call and writing another puts `MISLABELLED`
/// under the heading `id`.
struct Shifting {
    calls: Cell<usize>,
    total: Arc<AtomicUsize>,
}

impl Shifting {
    fn new(total: &Arc<AtomicUsize>) -> Self {
        Self {
            calls: Cell::new(0),
            total: Arc::clone(total),
        }
    }

    /// One whose next serialization is the `other` one.
    fn already_called(total: &Arc<AtomicUsize>) -> Self {
        let row = Self::new(total);
        row.calls.set(1);
        row
    }
}

impl Serialize for Shifting {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let call = self.calls.get();
        self.calls.set(call + 1);
        self.total.fetch_add(1, Ordering::SeqCst);
        let mut row = serializer.serialize_struct("Shifting", 1)?;
        if call == 0 {
            row.serialize_field("id", &7)?;
        } else {
            row.serialize_field("other", "MISLABELLED")?;
        }
        row.end()
    }
}

#[test]
fn each_row_is_serialized_once_on_the_finite_routes() {
    for (options, expected) in [
        (present(), "id\r\n7\r\n7\r\n7\r\n"),
        (absent(), "7\r\n7\r\n7\r\n"),
    ] {
        let total = Arc::new(AtomicUsize::new(0));
        let rows: Vec<Shifting> = (0..3).map(|_| Shifting::new(&total)).collect();
        let body = writer(options).to_vec(Rows::new(&rows)).unwrap();
        assert_eq!(text(body), expected);
        assert_eq!(
            total.load(Ordering::SeqCst),
            3,
            "one serialization per record"
        );
        assert!(rows.iter().all(|row| row.calls.get() == 1));
    }

    // The static `Encode<Vec<T>>` route.
    let total = Arc::new(AtomicUsize::new(0));
    let rows: Vec<Shifting> = (0..3).map(|_| Shifting::new(&total)).collect();
    let body = <Csv as Encode<Vec<Shifting>>>::encode(rows).unwrap();
    assert_eq!(body.as_bytes().unwrap(), b"id\r\n7\r\n7\r\n7\r\n");
    assert_eq!(total.load(Ordering::SeqCst), 3);
}

#[test]
fn each_row_is_serialized_once_on_the_lazy_route() {
    let total = Arc::new(AtomicUsize::new(0));
    let rows: Vec<_> = (0..3)
        .map(|_| Ok::<_, Infallible>(Shifting::new(&total)))
        .collect();
    let upload = writer(present())
        .upload(StreamRows::new(from_iter(rows)))
        .unwrap();
    let (body, _) = upload.into_parts();
    let (data, error) = drain(body);
    assert!(error.is_none(), "{error:?}");
    assert_eq!(data, b"id\r\n7\r\n7\r\n7\r\n");
    assert_eq!(total.load(Ordering::SeqCst), 3);

    // The static `Encode<StreamRows<S>>` route.
    let total = Arc::new(AtomicUsize::new(0));
    let rows: Vec<_> = (0..2)
        .map(|_| Ok::<_, Infallible>(Shifting::new(&total)))
        .collect();
    let body = <Csv as Encode<StreamRows<_>>>::encode(StreamRows::new(from_iter(rows))).unwrap();
    let (data, error) = drain(body);
    assert!(error.is_none(), "{error:?}");
    assert_eq!(data, b"id\r\n7\r\n7\r\n");
    assert_eq!(total.load(Ordering::SeqCst), 2);
}

#[test]
fn a_changed_serialization_is_refused_and_nothing_mislabelled_is_written() {
    // The second record's one serialization is `other`: refused, in that serialization.
    let total = Arc::new(AtomicUsize::new(0));
    let rows = [Shifting::new(&total), Shifting::already_called(&total)];
    let error = writer(present()).to_vec(Rows::new(&rows)).unwrap_err();
    assert_eq!(
        error.label(),
        "a CSV row changes the field names of the table"
    );
    assert_eq!(error.get("record"), Some(2));
    assert_eq!(total.load(Ordering::SeqCst), 2);

    let total = Arc::new(AtomicUsize::new(0));
    let rows = vec![
        Ok::<_, Infallible>(Shifting::new(&total)),
        Ok(Shifting::already_called(&total)),
    ];
    let upload = writer(present())
        .upload(StreamRows::new(from_iter(rows)))
        .unwrap();
    let (data, error) = drain(upload.into_parts().0);
    let error = error.expect("the transfer fails");
    let upload = upload_error(&error);
    assert_eq!(upload.records(), 1);
    let codec = upload.codec().unwrap();
    assert_eq!(
        codec.label(),
        "a CSV row changes the field names of the table"
    );
    assert_eq!(codec.get("record"), Some(2));
    assert!(
        !String::from_utf8_lossy(&data).contains("MISLABELLED"),
        "{data:?}"
    );
    assert_eq!(total.load(Ordering::SeqCst), 2);
}

// ASTRA-1: escape dialects read back byte for byte.

/// An escape byte with doubled quotes.
struct BackslashDoubled;
impl CsvProfile for BackslashDoubled {
    fn options() -> CsvProfileOptions {
        CsvProfileOptions::default().escape(Some(b'\\'))
    }
}

/// An escape byte for quotes too.
struct BackslashOnly;
impl CsvProfile for BackslashOnly {
    fn options() -> CsvProfileOptions {
        CsvProfileOptions::default()
            .escape(Some(b'\\'))
            .double_quote(false)
    }
}

/// The two escape dialects of `escape`: with doubled quotes, and with escaped quotes.
fn escape_dialects(base: CsvOptions, escape: u8) -> [CsvOptions; 2] {
    [
        base.clone().escape(Some(escape)),
        base.escape(Some(escape)).double_quote(false),
    ]
}

/// The raw records the incremental reader returns from these chunks.
async fn read_raw(reader: &CsvReader, chunks: Vec<Vec<u8>>) -> Vec<Vec<Vec<u8>>> {
    let mut records = reader.read_raw(Chunks::new(chunks));
    let mut out = Vec::new();
    while let Some(record) = records.next().await {
        out.push(record.unwrap().iter().map(<[u8]>::to_vec).collect());
    }
    out
}

#[tokio::test]
async fn the_reviewed_escape_value_reads_back_in_both_quote_modes() {
    // ASTRA-1's reproduction: a literal backslash in a field that needs quotes.
    let original = r"back\slash,comma";
    for options in escape_dialects(absent(), b'\\') {
        let body = writer(options.clone())
            .to_vec(Rows::new([(original,)]))
            .unwrap();
        assert_eq!(text(body.clone()), "\"back\\\\slash,comma\"\r\n");
        let reader = CsvReader::new(options).unwrap();
        for chunks in csv_support::single_splits(&body) {
            let mut rows = reader.read::<(String,), _>(Chunks::new(chunks));
            assert_eq!(rows.next().await.unwrap().unwrap().0, original);
            assert!(rows.next().await.is_none());
        }
    }

    // The finite routes of the static types.
    let rows = vec![(original.to_owned(),)];
    let body =
        <Csv<NoHeader, Comma, BackslashDoubled> as Encode<Vec<(String,)>>>::encode(rows.clone())
            .unwrap();
    let back = <Csv<NoHeader, Comma, BackslashDoubled> as Decode<Vec<(String,)>>>::decode_body(
        CsvContext::of::<NoHeader, Comma, BackslashDoubled>().unwrap(),
        body.as_bytes().unwrap().to_vec().into(),
    )
    .unwrap();
    assert_eq!(back, rows);
    let body =
        <Csv<NoHeader, Comma, BackslashOnly> as Encode<Vec<(String,)>>>::encode(rows.clone())
            .unwrap();
    let back = <Csv<NoHeader, Comma, BackslashOnly> as Decode<Vec<(String,)>>>::decode_body(
        CsvContext::of::<NoHeader, Comma, BackslashOnly>().unwrap(),
        body.as_bytes().unwrap().to_vec().into(),
    )
    .unwrap();
    assert_eq!(back, rows);
}

/// Values around the escape byte `\` (replaced by the dialect's escape byte): literal, repeated,
/// leading and trailing escapes, and escapes before quotes, delimiters and terminators.
fn escape_corpus(escape: char) -> Vec<String> {
    [
        r"back\slash",
        r"\",
        r"\\",
        r"\\\",
        r"a\\\\b",
        r"trailing\",
        r"\leading",
        r#"\""#,
        r#"\"\""#,
        r#""\"#,
        r#""\""#,
        r"\,",
        r"a\,b",
        r",\",
        "\\\r\n",
        "\\\n",
        "\\\r",
        "x\\\ny",
        r#"say "hi" \ bye"#,
        r#"\\""""#,
        r"1\2",
        r"1.5\",
        r" \ ",
        "",
        "plain",
        ",",
        "\"",
        "\r\n",
    ]
    .iter()
    .map(|value| value.replace('\\', &escape.to_string()))
    .collect()
}

#[tokio::test]
async fn escape_values_round_trip_at_every_split_point() {
    for escape in *b"\\~" {
        let corpus = escape_corpus(char::from(escape));
        let expected: Vec<Vec<Vec<u8>>> = corpus
            .iter()
            .map(|v| vec![v.as_bytes().to_vec(), v.as_bytes().to_vec()])
            .collect();
        for style in [
            QuoteStyle::Necessary,
            QuoteStyle::Always,
            QuoteStyle::NonNumeric,
        ] {
            for ending in [LineEnding::CrLf, LineEnding::Lf] {
                let base = absent().quote_style(style).write_terminator(ending);
                for options in escape_dialects(base, escape) {
                    let body = writer(options.clone())
                        .to_vec(Rows::new(corpus.iter().map(|v| (v, v))))
                        .unwrap();
                    let reader = CsvReader::new(options.clone()).unwrap();
                    for chunks in csv_support::single_splits(&body)
                        .into_iter()
                        .chain([csv_support::byte_chunks(&body)])
                    {
                        assert_eq!(
                            read_raw(&reader, chunks).await,
                            expected,
                            "{options:?}: {:?}",
                            String::from_utf8_lossy(&body)
                        );
                    }
                }
            }
        }
    }
}

#[tokio::test]
async fn every_short_value_over_the_special_bytes_round_trips() {
    // Every value of up to four bytes over: data, the escape, the quote, the delimiter, `\r`, `\n`.
    let alphabet = *b"a\\\",\r\n";
    let mut values: Vec<Vec<u8>> = vec![Vec::new()];
    let mut layer: Vec<Vec<u8>> = vec![Vec::new()];
    for _ in 0..4 {
        layer = layer
            .iter()
            .flat_map(|prefix| {
                alphabet.iter().map(move |&b| {
                    let mut value = prefix.clone();
                    value.push(b);
                    value
                })
            })
            .collect();
        values.extend(layer.iter().cloned());
    }
    assert_eq!(values.len(), 1 + 6 + 36 + 216 + 1296);
    let rows: Vec<RawRecord> = values.iter().map(|v| RawRecord::new([v])).collect();
    let expected: Vec<Vec<Vec<u8>>> = values.iter().map(|v| vec![v.clone()]).collect();
    for style in [QuoteStyle::Necessary, QuoteStyle::Always] {
        for ending in [LineEnding::CrLf, LineEnding::Lf] {
            let base = absent().quote_style(style).write_terminator(ending);
            for options in escape_dialects(base, b'\\') {
                let body = writer(options.clone())
                    .to_vec(Rows::new(rows.iter()))
                    .unwrap();
                let reader = CsvReader::new(options.clone()).unwrap();
                let partitions = [vec![body.clone()], csv_support::byte_chunks(&body)]
                    .into_iter()
                    .chain(csv_support::random_partitions(&body, 3, 0x5eed));
                for chunks in partitions {
                    assert_eq!(read_raw(&reader, chunks).await, expected, "{options:?}");
                }
            }
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Renamed {
    #[serde(rename = r"pa\th")]
    path: String,
    #[serde(rename = r#"say "\""#)]
    say: String,
}

#[tokio::test]
async fn escape_bytes_in_header_names_read_back() {
    for options in escape_dialects(present(), b'\\') {
        // Explicit columns label positional rows.
        let columns = [r"a\b", r#"q"\"#, r"c,\", r"end\"];
        let body = writer(options.clone())
            .to_vec(Rows::new([(r"x\", r#"\""#, ",", "")]).with_columns(columns))
            .unwrap();
        let reader = CsvReader::new(options.clone()).unwrap();
        for chunks in csv_support::single_splits(&body) {
            let mut records = reader.read_raw(Chunks::new(chunks));
            let record = records.next().await.unwrap().unwrap();
            assert!(record.iter().eq([&b"x\\"[..], b"\\\"", b",", b""]));
            let headers = records.headers().unwrap();
            assert!(headers.iter().eq(columns.iter().map(|c| c.as_bytes())));
            assert!(records.next().await.is_none());
        }

        // Inferred names: the typed reader maps the read names back to the same fields.
        let rows = vec![
            Renamed {
                path: r"C:\dir\".into(),
                say: r#""\""#.into(),
            },
            Renamed {
                path: "plain".into(),
                say: String::new(),
            },
        ];
        let body = writer(options.clone()).to_vec(Rows::new(&rows)).unwrap();
        for chunks in csv_support::single_splits(&body) {
            let mut read = reader.read::<Renamed, _>(Chunks::new(chunks));
            let mut got = Vec::new();
            while let Some(row) = read.next().await {
                got.push(row.unwrap());
            }
            assert_eq!(got, rows);
        }
    }
}

#[test]
fn raw_tables_with_escape_bytes_round_trip_on_the_finite_route() {
    let table = RawTable {
        headers: Some(RawRecord::new([&b"h\\"[..], b"k\"\\"])),
        records: vec![
            RawRecord::new([&b"\\\xff\\"[..], b"\"\\\""]),
            RawRecord::new([&b"a,\\"[..], b"\\\r\n"]),
            RawRecord::new([&b""[..], b"\\"]),
        ],
    };
    let body =
        <Csv<HasHeader, Comma, BackslashOnly> as Encode<RawTable>>::encode(table.clone()).unwrap();
    let back = <Csv<HasHeader, Comma, BackslashOnly> as Decode<RawTable>>::decode_body(
        CsvContext::of::<HasHeader, Comma, BackslashOnly>().unwrap(),
        body.as_bytes().unwrap().to_vec().into(),
    )
    .unwrap();
    assert_eq!(back, table);
    let body = <Csv<HasHeader, Comma, BackslashDoubled> as Encode<RawTable>>::encode(table.clone())
        .unwrap();
    let back = <Csv<HasHeader, Comma, BackslashDoubled> as Decode<RawTable>>::decode_body(
        CsvContext::of::<HasHeader, Comma, BackslashDoubled>().unwrap(),
        body.as_bytes().unwrap().to_vec().into(),
    )
    .unwrap();
    assert_eq!(back, table);
}

/// The settings shared by this crate's writer and `csv`'s writer.
#[derive(Clone, Copy, Debug)]
struct Settings {
    delimiter: u8,
    style: QuoteStyle,
    ending: LineEnding,
    escape: Option<u8>,
    double_quote: bool,
    comment: Option<u8>,
}

impl Settings {
    const DEFAULT: Self = Self {
        delimiter: b',',
        style: QuoteStyle::Necessary,
        ending: LineEnding::CrLf,
        escape: None,
        double_quote: true,
        comment: None,
    };

    fn options(self, header: HeaderMode) -> CsvOptions {
        CsvOptions::new(header, self.delimiter)
            .quote_style(self.style)
            .write_terminator(self.ending)
            .escape(self.escape)
            .double_quote(self.double_quote)
            .comment(self.comment)
    }

    fn csv_crate(self, headers: bool) -> csv::Writer<Vec<u8>> {
        let mut builder = csv::WriterBuilder::new();
        builder
            .has_headers(headers)
            .delimiter(self.delimiter)
            .double_quote(self.double_quote)
            .comment(self.comment)
            .terminator(match self.ending {
                LineEnding::CrLf => csv::Terminator::CRLF,
                _ => csv::Terminator::Any(b'\n'),
            })
            .quote_style(match self.style {
                QuoteStyle::Always => csv::QuoteStyle::Always,
                QuoteStyle::NonNumeric => csv::QuoteStyle::NonNumeric,
                QuoteStyle::Never => csv::QuoteStyle::Never,
                _ => csv::QuoteStyle::Necessary,
            });
        if let Some(escape) = self.escape {
            builder.escape(escape);
        }
        builder.from_writer(Vec::new())
    }
}

#[derive(Serialize)]
enum Kind {
    Active,
}

#[derive(Serialize)]
struct Marker;

#[derive(Serialize)]
struct Wrapper(u32);

#[derive(Serialize)]
struct Mixed {
    small: u8,
    signed: i64,
    big: u128,
    negative: i128,
    single: f32,
    double: f64,
    flag: bool,
    letter: char,
    missing: Option<u8>,
    present: Option<&'static str>,
    unit: (),
    kind: Kind,
    marker: Marker,
    wrapped: Wrapper,
    #[serde(rename = "Text, \"quoted\"")]
    text: &'static str,
}

fn mixed_rows() -> Vec<Mixed> {
    let texts = [
        "plain",
        "",
        "a,b",
        "a;b",
        "say \"hi\"",
        "line\nbreak",
        "cr\rhere",
        "#hash",
        " padded ",
        "12",
        "-1.5e3",
        "nan",
        "inf",
        "Zoë",
    ];
    let doubles = [
        0.1,
        1.0,
        -0.0,
        1e20,
        1e-7,
        123_456_789.125,
        f64::MAX,
        f64::MIN_POSITIVE,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        2.5,
        1e16,
        1e15,
    ];
    texts
        .iter()
        .zip(doubles)
        .enumerate()
        .map(|(i, (&text, double))| Mixed {
            small: i as u8,
            signed: -(i as i64) * 1_000_000_007,
            big: u128::MAX - i as u128,
            negative: i128::MIN + i as i128,
            single: 0.1 + i as f32,
            double,
            flag: i % 2 == 0,
            letter: if i % 3 == 0 { ',' } else { 'é' },
            missing: None,
            present: (i % 2 == 1).then_some("p\"q"),
            unit: (),
            kind: Kind::Active,
            marker: Marker,
            wrapped: Wrapper(i as u32),
            text,
        })
        .collect()
}

#[test]
fn fields_without_escape_bytes_keep_the_csv_crate_bytes() {
    // The encoder writes its own bytes (for the escape rule and the limits). Where `csv` was
    // already right, that is every field without the dialect's escape byte, the bytes are
    // unchanged: compared with `csv`'s writer over every value type `csv` writes.
    let dialects = [
        Settings::DEFAULT,
        Settings {
            delimiter: b';',
            ..Settings::DEFAULT
        },
        Settings {
            delimiter: b'\t',
            ending: LineEnding::Lf,
            ..Settings::DEFAULT
        },
        Settings {
            style: QuoteStyle::Always,
            ..Settings::DEFAULT
        },
        Settings {
            style: QuoteStyle::NonNumeric,
            ..Settings::DEFAULT
        },
        Settings {
            style: QuoteStyle::Never,
            ..Settings::DEFAULT
        },
        Settings {
            comment: Some(b'#'),
            ..Settings::DEFAULT
        },
        Settings {
            escape: Some(b'\\'),
            ..Settings::DEFAULT
        },
        Settings {
            escape: Some(b'\\'),
            double_quote: false,
            ..Settings::DEFAULT
        },
        Settings {
            escape: Some(b'~'),
            double_quote: false,
            style: QuoteStyle::Always,
            ..Settings::DEFAULT
        },
    ];
    for settings in dialects {
        for (header, headers) in [(HeaderMode::Present, true), (HeaderMode::Absent, false)] {
            let ours = writer(settings.options(header))
                .to_vec(Rows::new(mixed_rows()))
                .unwrap();
            let mut theirs = settings.csv_crate(headers);
            for row in mixed_rows() {
                theirs.serialize(row).unwrap();
            }
            let theirs = theirs.into_inner().unwrap();
            // Not vacuous: every row (and the header) was written.
            assert!(theirs.iter().filter(|&&b| b == b'\n').count() >= 14 + usize::from(headers));
            assert_eq!(headers, String::from_utf8_lossy(&theirs).contains("small"));
            assert_eq!(
                String::from_utf8_lossy(&ours),
                String::from_utf8_lossy(&theirs),
                "{settings:?} {header:?}"
            );
        }
    }
}

// ASTRA-2: limits stop the encoding before an oversized record is buffered.

/// A field that records that it was serialized.
struct Later(Arc<AtomicBool>);

impl Serialize for Later {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.store(true, Ordering::SeqCst);
        serializer.serialize_u8(1)
    }
}

#[derive(Serialize)]
struct Probe {
    big: String,
    later: Later,
}

#[test]
fn an_oversized_field_stops_the_row_before_later_fields_on_both_routes() {
    let seen = Arc::new(AtomicBool::new(false));
    let probe = Probe {
        big: "x".repeat(1 << 20),
        later: Later(Arc::clone(&seen)),
    };
    let error = writer(absent().max_record_bytes(64))
        .to_vec(Rows::new([&probe]))
        .unwrap_err();
    assert_eq!(error.label(), "a CSV record exceeds the configured size");
    assert_eq!(error.context(), [("record", 1), ("limit", 64)]);
    assert!(
        !seen.load(Ordering::SeqCst),
        "the row's serialization stopped at the oversized field"
    );

    let seen = Arc::new(AtomicBool::new(false));
    let rows = vec![
        Ok::<_, Infallible>(Probe {
            big: "small".into(),
            later: Later(Arc::new(AtomicBool::new(false))),
        }),
        Ok(Probe {
            big: "x".repeat(1 << 20),
            later: Later(Arc::clone(&seen)),
        }),
    ];
    let upload = writer(absent().max_record_bytes(64))
        .upload(StreamRows::new(from_iter(rows)))
        .unwrap();
    let (data, error) = drain(upload.into_parts().0);
    let error = error.expect("the transfer fails");
    let upload = upload_error(&error);
    assert_eq!(upload.records(), 1);
    let codec = upload.codec().unwrap();
    assert_eq!(codec.label(), "a CSV record exceeds the configured size");
    assert_eq!(codec.context(), [("record", 2), ("limit", 64)]);
    assert!(!seen.load(Ordering::SeqCst));
    assert!(
        !data.contains(&b'x'),
        "no byte of the refused record is sent"
    );
}

/// A sequence of 1,000 fields that counts how many the serializer was asked to take.
struct Wide {
    attempted: Cell<usize>,
}

impl Serialize for Wide {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(1000))?;
        for i in 0..1000 {
            self.attempted.set(i + 1);
            seq.serialize_element(&1u8)?;
        }
        seq.end()
    }
}

#[test]
fn the_field_limit_stops_the_row_at_the_first_field_past_it() {
    let row = Wide {
        attempted: Cell::new(0),
    };
    let error = writer(absent().max_fields(3))
        .to_vec(Rows::new([&row]))
        .unwrap_err();
    assert_eq!(
        error.label(),
        "a CSV row has more fields than the configured limit"
    );
    assert_eq!(error.context(), [("record", 1), ("limit", 3)]);
    assert_eq!(row.attempted.get(), 4, "refused at the fourth field");
}

#[test]
fn limits_count_the_encoded_bytes_with_quotes_and_escapes() {
    // `a\,` is written `"a\\,"`: six bytes.
    for options in escape_dialects(absent(), b'\\') {
        let body = writer(options.clone().max_record_bytes(6))
            .to_vec(Rows::new([(r"a\,",)]))
            .unwrap();
        assert_eq!(body, b"\"a\\\\,\"\r\n");
        let error = writer(options.max_record_bytes(5))
            .to_vec(Rows::new([(r"a\,",)]))
            .unwrap_err();
        assert_eq!(error.label(), "a CSV record exceeds the configured size");
        assert_eq!(error.context(), [("record", 1), ("limit", 5)]);
    }
    // One empty field is written `""`: two bytes.
    assert_eq!(
        writer(absent().max_record_bytes(2))
            .to_vec(Rows::new([("",)]))
            .unwrap(),
        b"\"\"\r\n"
    );
    let error = writer(absent().max_record_bytes(1))
        .to_vec(Rows::new([("",)]))
        .unwrap_err();
    assert_eq!(error.get("limit"), Some(1));
    // The header has its own limit, with the same counting.
    for options in escape_dialects(present(), b'\\') {
        let body = writer(options.clone().max_header_bytes(6))
            .to_vec(Rows::new([(1,)]).with_columns([r"h\,"]))
            .unwrap();
        assert_eq!(body, b"\"h\\\\,\"\r\n1\r\n");
        let error = writer(options.max_header_bytes(5))
            .to_vec(Rows::new([(1,)]).with_columns([r"h\,"]))
            .unwrap_err();
        assert_eq!(error.label(), "the CSV header exceeds the configured size");
        assert_eq!(error.context(), [("limit", 5)]);
    }
}
