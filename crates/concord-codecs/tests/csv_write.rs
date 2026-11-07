//! Write semantics (CSV.md §5, §8 items 7, 8, 10 partly): header inference with renames,
//! explicit columns (labels, never reordering), tuples, empty tables, conflicting column sources,
//! same-width schema changes (the conditional-serialization regression), width, CRLF/LF, quoting,
//! raw tables, limits, and round trips through the reader.

#![cfg(feature = "csv")]

mod csv_support;

use concord_codecs::csv::{
    Comma, Csv, CsvOptions, CsvProfile, CsvProfileOptions, CsvReader, CsvWriter, HasHeader,
    HeaderMode, LineEnding, NoHeader, QuoteStyle, RawRecord, RawTable, Rows, Semicolon,
};
use concord_codecs::stream::Chunks;
use concord_core::Encode;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Item {
    id: u32,
    #[serde(rename = "Display Name")]
    name: String,
    price: Option<f64>,
}

fn items() -> Vec<Item> {
    vec![
        Item {
            id: 1,
            name: "Lyon, FR".into(),
            price: Some(2.5),
        },
        Item {
            id: 2,
            name: "say \"hi\"\nnow".into(),
            price: None,
        },
    ]
}

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

#[test]
fn named_rows_infer_the_header_with_renames_and_crlf() {
    let body = writer(present()).to_vec(Rows::new(items())).unwrap();
    assert_eq!(
        text(body),
        "id,Display Name,price\r\n1,\"Lyon, FR\",2.5\r\n2,\"say \"\"hi\"\"\nnow\",\r\n"
    );
    // The static route writes the same bytes.
    let body = concord_codecs::csv::Csv::<HasHeader>::writer()
        .unwrap()
        .to_vec(Rows::new(items()))
        .unwrap();
    assert!(text(body).starts_with("id,Display Name,price\r\n"));
}

#[test]
fn no_header_never_writes_one() {
    let body = writer(absent()).to_vec(Rows::new(items())).unwrap();
    assert_eq!(
        text(body),
        "1,\"Lyon, FR\",2.5\r\n2,\"say \"\"hi\"\"\nnow\",\r\n"
    );
    let empty: Vec<Item> = Vec::new();
    assert_eq!(writer(absent()).to_vec(Rows::new(empty)).unwrap(), b"");
}

#[test]
fn explicit_columns_label_positions_and_must_equal_named_fields_in_order() {
    // Tuples with explicit names.
    let body = writer(present())
        .to_vec(Rows::new([(1, "a"), (2, "b")]).with_columns(["id", "name"]))
        .unwrap();
    assert_eq!(text(body), "id,name\r\n1,a\r\n2,b\r\n");

    // Named rows whose fields are exactly the columns.
    let ok = writer(present())
        .to_vec(Rows::new(items()).with_columns(["id", "Display Name", "price"]))
        .unwrap();
    assert!(text(ok).starts_with("id,Display Name,price\r\n"));

    // A different order is refused: columns never reorder fields (`name,id` above `id,name`
    // values would be corrupt).
    let error = writer(present())
        .to_vec(Rows::new(items()).with_columns(["Display Name", "id", "price"]))
        .unwrap_err();
    assert_eq!(
        error.label(),
        "the CSV row's field names are not the columns, in order (columns do not reorder fields)"
    );
    assert_eq!(error.get("record"), Some(1));
}

#[test]
fn tuples_with_a_header_need_explicit_columns() {
    let error = writer(present()).to_vec(Rows::new([(1, "a")])).unwrap_err();
    assert_eq!(
        error.label(),
        "a CSV header needs column names: these rows have no field names (give explicit columns)"
    );
    // Without a header, tuples are fine.
    assert_eq!(
        text(writer(absent()).to_vec(Rows::new([(1, "a")])).unwrap()),
        "1,a\r\n"
    );
}

#[test]
fn empty_tables() {
    let empty: Vec<Item> = Vec::new();
    // With a header: explicit columns give a header-only body...
    assert_eq!(
        text(
            writer(present())
                .to_vec(Rows::new(empty.clone()).with_columns(["id", "Display Name", "price"]))
                .unwrap()
        ),
        "id,Display Name,price\r\n"
    );
    // ...from the profile too...
    assert_eq!(
        text(
            writer(present().columns(["id"]))
                .to_vec(Rows::new(empty.clone()))
                .unwrap()
        ),
        "id\r\n"
    );
    // ...and without columns it is an error, not a silent empty body.
    let error = writer(present())
        .to_vec(Rows::new(empty.clone()))
        .unwrap_err();
    assert_eq!(
        error.label(),
        "an empty CSV table with a header needs explicit columns"
    );
    assert_eq!(
        <Csv as Encode<Vec<Item>>>::encode(empty)
            .unwrap_err()
            .label(),
        "an empty CSV table with a header needs explicit columns"
    );
}

#[test]
fn conflicting_column_sources_are_refused() {
    let options = present().columns(["id", "Display Name", "price"]);
    let error = writer(options.clone())
        .to_vec(Rows::new(items()).with_columns(["id", "name", "price"]))
        .unwrap_err();
    assert_eq!(
        error.label(),
        "the payload's CSV columns conflict with the configured columns"
    );
    // Equal sources agree.
    assert!(
        writer(options)
            .to_vec(Rows::new(items()).with_columns(["id", "Display Name", "price"]))
            .is_ok()
    );
    // Invalid payload columns.
    for (columns, label) in [
        (vec!["a", "a"], "a CSV column list repeats a name"),
        (vec!["a", ""], "a CSV column name is empty"),
        (vec![], "a CSV column list is empty"),
    ] {
        let error = writer(absent())
            .to_vec(Rows::new([(1, 2)]).with_columns(columns))
            .unwrap_err();
        assert_eq!(error.label(), label);
    }
}

/// A row whose `Serialize` writes a different single field depending on its value: the same
/// width (1) on every row, different names.
struct Conditional(Result<u32, u32>);

impl Serialize for Conditional {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("Conditional", 1)?;
        match self.0 {
            Ok(a) => s.serialize_field("approved", &a)?,
            Err(r) => s.serialize_field("rejected", &r)?,
        }
        s.end()
    }
}

#[test]
fn a_same_width_change_of_field_names_is_refused() {
    let rows = || vec![Conditional(Ok(10)), Conditional(Err(99))];

    // The hazard: `csv`'s own writer accepts it and writes 99 under the heading `approved`.
    let mut plain = csv::Writer::from_writer(Vec::new());
    for row in rows() {
        plain.serialize(row).unwrap();
    }
    assert_eq!(text(plain.into_inner().unwrap()), "approved\n10\n99\n");

    // This writer refuses the second row, with or without a header.
    for options in [present(), absent()] {
        let error = writer(options).to_vec(Rows::new(rows())).unwrap_err();
        assert_eq!(
            error.label(),
            "a CSV row changes the field names of the table"
        );
        assert_eq!(error.get("record"), Some(2));
    }

    // `skip_serializing_if` swapping two optional fields: same width, different names.
    #[derive(Serialize)]
    struct Sparse {
        #[serde(skip_serializing_if = "Option::is_none")]
        a: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        b: Option<u8>,
    }
    let error = writer(present())
        .to_vec(Rows::new([
            Sparse {
                a: Some(1),
                b: None,
            },
            Sparse {
                a: None,
                b: Some(2),
            },
        ]))
        .unwrap_err();
    assert_eq!(
        error.label(),
        "a CSV row changes the field names of the table"
    );
    // Named then positional rows are a change too.
    #[derive(Serialize)]
    #[serde(untagged)]
    enum Mixed {
        Named { a: u8 },
        Tuple(u8),
    }
    let error = writer(absent())
        .to_vec(Rows::new([Mixed::Named { a: 1 }, Mixed::Tuple(2)]))
        .unwrap_err();
    assert_eq!(
        error.label(),
        "a CSV row changes the field names of the table"
    );
}

#[test]
fn width_mismatches_are_refused() {
    let rows = vec![vec!["a", "b"], vec!["c"]];
    let error = writer(absent()).to_vec(Rows::new(rows)).unwrap_err();
    assert_eq!(
        error.label(),
        "a CSV row does not have as many fields as the table"
    );
    assert_eq!(
        error.context(),
        [("record", 2), ("fields", 1), ("expected", 2)]
    );
    // Explicit columns set the width from the first row on.
    let error = writer(present())
        .to_vec(Rows::new([vec![1, 2, 3]]).with_columns(["a", "b"]))
        .unwrap_err();
    assert_eq!(
        error.context(),
        [("record", 1), ("fields", 3), ("expected", 2)]
    );
    // `flexible` allows it.
    assert_eq!(
        text(
            writer(absent().flexible(true))
                .to_vec(Rows::new(vec![vec!["a", "b"], vec!["c"]]))
                .unwrap()
        ),
        "a,b\r\nc\r\n"
    );
}

#[test]
fn unsupported_row_shapes_are_refused() {
    #[derive(Serialize)]
    struct Nested {
        a: u8,
        b: (u8, u8),
    }
    let error = writer(present())
        .to_vec(Rows::new([Nested { a: 1, b: (2, 3) }]))
        .unwrap_err();
    assert_eq!(
        error.label(),
        "a named CSV field holds a nested container (only scalars can be named fields)"
    );
    let map: std::collections::BTreeMap<&str, u8> = [("a", 1)].into();
    let error = writer(absent()).to_vec(Rows::new([map])).unwrap_err();
    assert_eq!(
        error.label(),
        "a CSV row cannot be a map (or contain a flattened struct)"
    );
    let error = writer(absent())
        .to_vec(Rows::new([Vec::<u8>::new()]))
        .unwrap_err();
    assert_eq!(error.label(), "a CSV row serializes to no fields");
}

#[test]
fn line_endings_quoting_and_comment_bytes() {
    let lf = writer(absent().write_terminator(LineEnding::Lf))
        .to_vec(Rows::new([(1, "a"), (2, "b")]))
        .unwrap();
    assert_eq!(text(lf), "1,a\n2,b\n");

    let always = writer(absent().quote_style(QuoteStyle::Always))
        .to_vec(Rows::new([(1, "a")]))
        .unwrap();
    assert_eq!(text(always), "\"1\",\"a\"\r\n");

    // A semicolon dialect quotes semicolons, not commas.
    let semi = writer(CsvOptions::new(HeaderMode::Absent, b';'))
        .to_vec(Rows::new([("a;b", "c,d")]))
        .unwrap();
    assert_eq!(text(semi), "\"a;b\";c,d\r\n");

    // With a comment byte, a field starting with it is quoted (so it is not read as a comment).
    let commented = writer(absent().comment(Some(b'#')))
        .to_vec(Rows::new([("#not a comment", 1)]))
        .unwrap();
    assert_eq!(text(commented), "\"#not a comment\",1\r\n");

    // An escape dialect.
    let escaped = writer(absent().double_quote(false).escape(Some(b'\\')))
        .to_vec(Rows::new([("say \"hi\"", 1)]))
        .unwrap();
    assert_eq!(text(escaped), "\"say \\\"hi\\\"\",1\r\n");
}

#[test]
fn a_record_larger_than_the_limit_is_refused() {
    let w = writer(absent().max_record_bytes(8));
    assert_eq!(
        text(w.to_vec(Rows::new([("abc", "def")])).unwrap()),
        "abc,def\r\n"
    );
    let error = w.to_vec(Rows::new([("abcd", "efgh")])).unwrap_err();
    assert_eq!(error.label(), "a CSV record exceeds the configured size");
    let error = writer(absent().max_fields(2))
        .to_vec(Rows::new([(1, 2, 3)]))
        .unwrap_err();
    assert_eq!(
        error.label(),
        "a CSV row has more fields than the configured limit"
    );
}

#[test]
fn raw_tables_round_trip_bytes() {
    let table = RawTable {
        headers: Some(RawRecord::new(["id", "bytes"])),
        records: vec![
            RawRecord::new([&b"1"[..], b"\xff\xfe"]),
            RawRecord::new([&b"2"[..], b"a,\"b\""]),
        ],
    };
    let body = <Csv as Encode<RawTable>>::encode(table.clone()).unwrap();
    let bytes = futures_free_bytes(body);
    assert_eq!(bytes, b"id,bytes\r\n1,\xff\xfe\r\n2,\"a,\"\"b\"\"\"\r\n");

    let back = <Csv as concord_core::Decode<RawTable>>::decode_body(
        concord_codecs::csv::CsvContext::of::<HasHeader, Comma, concord_codecs::csv::StandardCsv>()
            .unwrap(),
        bytes.into(),
    )
    .unwrap();
    assert_eq!(back, table);

    // `Debug` shows field counts, never values.
    let shown = format!("{table:?}");
    assert!(!shown.contains("bytes") && !shown.contains("id"), "{shown}");

    // A header-less table must not carry a header, and a headed one needs it.
    assert_eq!(
        <Csv<NoHeader> as Encode<RawTable>>::encode(table.clone())
            .unwrap_err()
            .label(),
        "a headerless CSV table cannot carry a header record"
    );
    assert_eq!(
        <Csv as Encode<RawTable>>::encode(RawTable::default())
            .unwrap_err()
            .label(),
        "a CSV table with a header needs its header record"
    );
}

/// The bytes of a finite body.
fn futures_free_bytes(body: concord_core::reqwest::Body) -> Vec<u8> {
    body.as_bytes().expect("a finite body").to_vec()
}

#[test]
fn static_content_types_follow_the_header_policy() {
    assert_eq!(
        <Csv as Encode<Vec<Item>>>::CONTENT_TYPE,
        Some("text/csv; header=present")
    );
    assert_eq!(
        <Csv<NoHeader, Semicolon> as Encode<Vec<Item>>>::CONTENT_TYPE,
        Some("text/csv; header=absent")
    );
    assert_eq!(
        <Csv<NoHeader> as Encode<Rows<Vec<Item>>>>::CONTENT_TYPE,
        Some("text/csv; header=absent")
    );
    assert_eq!(
        <Csv as Encode<RawTable>>::CONTENT_TYPE,
        Some("text/csv; header=present")
    );
    assert_eq!(Csv::<HasHeader>::CONTENT_TYPE, "text/csv; header=present");
    assert_eq!(writer(absent()).content_type(), "text/csv; header=absent");
    assert_eq!(writer(present()).content_type(), "text/csv; header=present");
}

/// A profile of a provider: semicolons, LF, explicit columns.
struct Provider;
impl CsvProfile for Provider {
    fn options() -> CsvProfileOptions {
        CsvProfileOptions::default()
            .write_terminator(LineEnding::Lf)
            .columns(["id", "Display Name", "price"])
    }
}

#[tokio::test]
async fn written_tables_read_back_equal_on_both_routes() {
    type ProviderCsv = Csv<HasHeader, Semicolon, Provider>;
    let body = futures_free_bytes(<ProviderCsv as Encode<Vec<Item>>>::encode(items()).unwrap());
    assert_eq!(
        text(body.clone()),
        "id;Display Name;price\n1;Lyon, FR;2.5\n2;\"say \"\"hi\"\"\nnow\";\n"
    );
    let finite = <ProviderCsv as concord_core::Decode<Vec<Item>>>::decode_body(
        concord_codecs::csv::CsvContext::of::<HasHeader, Semicolon, Provider>().unwrap(),
        body.clone().into(),
    )
    .unwrap();
    assert_eq!(finite, items());

    let reader = ProviderCsv::reader().unwrap();
    for chunks in csv_support::single_splits(&body) {
        let mut rows = reader.read::<Item, _>(Chunks::new(chunks));
        let mut got = Vec::new();
        while let Some(row) = rows.next().await {
            got.push(row.unwrap());
        }
        assert_eq!(got, items());
    }
    // The runtime writer with the same options writes the same bytes.
    let runtime = CsvWriter::new(ProviderCsv::options()).unwrap();
    assert_eq!(runtime.to_vec(Rows::new(items())).unwrap(), body);
    let _ = CsvReader::new(ProviderCsv::options()).unwrap();
}
