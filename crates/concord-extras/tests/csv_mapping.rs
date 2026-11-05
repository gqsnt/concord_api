//! Header consumption and row mapping (CSV.md §3, §4, §8 items 6, 7, 9 partly): every mapping row
//! of the contract table, reordered/duplicate/empty/missing/extra names, a quoted newline in a
//! header, header-only/empty inputs, empty and whitespace fields, invalid UTF-8 typed rows versus
//! raw bytes, and the equality of the finite and incremental routes on each case.

#![cfg(feature = "csv")]

mod csv_support;

use concord_core::Decode;
use concord_core::bytes::Bytes;
use concord_extras::csv::{
    Csv, CsvContext, CsvOptions, CsvProfile, CsvProfileOptions, CsvReader, HasHeader, HeaderMode,
    NoHeader, RawTable, ReadMapping,
};
use concord_extras::stream::{Chunks, ReadError, ReadErrorKind};
use serde::Deserialize;
use serde::de::DeserializeOwned;

#[derive(Debug, PartialEq, Deserialize)]
struct Row {
    id: u32,
    name: String,
}

fn row(id: u32, name: &str) -> Row {
    Row {
        id,
        name: name.into(),
    }
}

/// All rows (or the first error) of `body` read incrementally with `options`, one byte per chunk.
async fn read<T: DeserializeOwned>(
    options: CsvOptions,
    body: &[u8],
) -> (Vec<T>, Option<ReadError>) {
    let reader = CsvReader::new(options).unwrap();
    let mut rows = reader.read::<T, _>(Chunks::new(csv_support::byte_chunks(body)));
    let mut out = Vec::new();
    while let Some(next) = rows.next().await {
        match next {
            Ok(row) => out.push(row),
            Err(e) => return (out, Some(e)),
        }
    }
    (out, None)
}

/// The finite route of a static type.
fn decode<F, T>(body: &'static [u8]) -> Result<Vec<T>, &'static str>
where
    F: Decode<Vec<T>, Context = CsvContext>,
{
    let head_map = concord_core::reqwest::header::HeaderMap::new();
    let head = concord_core::ResponseHead::new(
        concord_core::reqwest::StatusCode::OK,
        concord_core::reqwest::Version::HTTP_11,
        &head_map,
    );
    let context = F::context(&head).map_err(|e| e.label())?;
    F::decode_body(context, Bytes::from_static(body)).map_err(|e| e.label())
}

fn label(e: &ReadError) -> &'static str {
    e.codec().unwrap().label()
}

// ---------------------------------------------------------------------------------------------
// Row 1: HasHeader, default mapping: names from the header.

#[tokio::test]
async fn has_header_default_maps_named_fields_in_any_column_order() {
    let body = b"name,id\nLyon,1\nZo\xc3\xab,2\n";
    let (rows, error) = read::<Row>(CsvOptions::new(HeaderMode::Present, b','), body).await;
    assert!(error.is_none());
    assert_eq!(rows, [row(1, "Lyon"), row(2, "Zoë")]);
    assert_eq!(
        decode::<Csv, Row>(body),
        Ok(vec![row(1, "Lyon"), row(2, "Zoë")])
    );
}

#[tokio::test]
async fn has_header_default_keeps_tuples_and_sequences_positional() {
    let body = b"name,id\nLyon,1\n";
    let (rows, _) = read::<(String, u32)>(CsvOptions::new(HeaderMode::Present, b','), body).await;
    assert_eq!(rows, [("Lyon".to_owned(), 1)]);
    assert_eq!(
        decode::<Csv, Vec<String>>(body),
        Ok(vec![vec!["Lyon".to_owned(), "1".to_owned()]])
    );
}

#[tokio::test]
async fn a_quoted_newline_in_a_header_names_a_field() {
    #[derive(Debug, PartialEq, Deserialize)]
    struct Display {
        #[serde(rename = "display\nname")]
        display: String,
        id: u8,
    }
    let body = b"\"display\nname\",id\r\n\"A,B\",1\r\n";
    let (rows, error) = read::<Display>(CsvOptions::new(HeaderMode::Present, b','), body).await;
    assert!(error.is_none());
    assert_eq!(
        rows,
        [Display {
            display: "A,B".into(),
            id: 1
        }]
    );
}

#[tokio::test]
async fn duplicate_and_empty_names_are_refused_by_named_mapping() {
    for (body, expected, field) in [
        (
            &b"id,name,id\n1,a,2\n"[..],
            "the CSV header repeats a name",
            3,
        ),
        (b"id,,name\n1,x,a\n", "the CSV header has an empty name", 2),
    ] {
        let (rows, error) = read::<Row>(CsvOptions::new(HeaderMode::Present, b','), body).await;
        assert!(rows.is_empty());
        let error = error.unwrap();
        assert_eq!(error.kind(), ReadErrorKind::Format);
        assert_eq!(label(&error), expected);
        assert_eq!(error.codec().unwrap().get("field"), Some(field));
        assert_eq!(error.position().records, 0);
        assert_eq!(
            decode::<Csv, Row>(Box::leak(body.to_vec().into_boxed_slice())),
            Err(expected)
        );
    }
}

#[tokio::test]
async fn positional_and_raw_reading_keep_duplicate_and_empty_names() {
    let body = b"id,id,\n1,2,x\n";
    let positional =
        CsvOptions::new(HeaderMode::Present, b',').read_mapping(ReadMapping::Positional);
    let (rows, error) = read::<(u8, u8, String)>(positional, body).await;
    assert!(error.is_none());
    assert_eq!(rows, [(1, 2, "x".to_owned())]);

    let reader = CsvReader::new(CsvOptions::new(HeaderMode::Present, b',')).unwrap();
    let mut raw = reader.read_raw(Chunks::new([body.to_vec()]));
    let record = raw.next().await.unwrap().unwrap();
    assert_eq!(record.iter().collect::<Vec<_>>(), [&b"1"[..], b"2", b"x"]);
    let headers = raw.headers().unwrap();
    assert_eq!(headers.iter().collect::<Vec<_>>(), [&b"id"[..], b"id", b""]);

    let table = decode_raw::<Csv>(body);
    assert_eq!(
        table.headers.unwrap().to_strings().unwrap(),
        ["id", "id", ""]
    );
}

fn decode_raw<F: Decode<RawTable, Context = CsvContext>>(body: &'static [u8]) -> RawTable {
    let head_map = concord_core::reqwest::header::HeaderMap::new();
    let head = concord_core::ResponseHead::new(
        concord_core::reqwest::StatusCode::OK,
        concord_core::reqwest::Version::HTTP_11,
        &head_map,
    );
    F::decode_body(F::context(&head).unwrap(), Bytes::from_static(body)).unwrap()
}

#[tokio::test]
async fn missing_extra_and_defaulted_fields_follow_serde() {
    // Missing: a decode error naming the record (no value in the text).
    let (rows, error) = read::<Row>(CsvOptions::new(HeaderMode::Present, b','), b"id\n1\n").await;
    assert!(rows.is_empty());
    let error = error.unwrap();
    assert_eq!(error.kind(), ReadErrorKind::Decode);
    assert_eq!(
        label(&error),
        "a CSV record does not decode into the requested row type"
    );
    assert_eq!(error.codec().unwrap().get("record"), Some(1));

    // Extra columns are ignored by an ordinary derive...
    let body = b"id,extra,name\n1,zzz,Lyon\n";
    let (rows, error) = read::<Row>(CsvOptions::new(HeaderMode::Present, b','), body).await;
    assert!(error.is_none());
    assert_eq!(rows, [row(1, "Lyon")]);

    // ...and refused by `deny_unknown_fields`.
    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    #[allow(dead_code)]
    struct Strict {
        id: u32,
        name: String,
    }
    let (_, error) = read::<Strict>(CsvOptions::new(HeaderMode::Present, b','), body).await;
    assert_eq!(error.unwrap().kind(), ReadErrorKind::Decode);

    // A defaulted field may be absent.
    #[derive(Debug, PartialEq, Deserialize)]
    struct Defaulted {
        id: u32,
        #[serde(default)]
        name: String,
    }
    let (rows, error) =
        read::<Defaulted>(CsvOptions::new(HeaderMode::Present, b','), b"id\n7\n").await;
    assert!(error.is_none());
    assert_eq!(
        rows,
        [Defaulted {
            id: 7,
            name: String::new()
        }]
    );
}

// ---------------------------------------------------------------------------------------------
// Row 2: NoHeader, default mapping: positional.

#[tokio::test]
async fn no_header_default_is_positional_including_structs_by_declaration_order() {
    let body = b"1,Lyon\n2,Zo\xc3\xab\n";
    let (rows, error) = read::<Row>(CsvOptions::new(HeaderMode::Absent, b','), body).await;
    assert!(error.is_none());
    assert_eq!(rows, [row(1, "Lyon"), row(2, "Zoë")]);
    let (tuples, _) = read::<(u32, String)>(CsvOptions::new(HeaderMode::Absent, b','), body).await;
    assert_eq!(tuples, [(1, "Lyon".to_owned()), (2, "Zoë".to_owned())]);
    assert_eq!(
        decode::<Csv<NoHeader>, Row>(body),
        Ok(vec![row(1, "Lyon"), row(2, "Zoë")])
    );
}

// ---------------------------------------------------------------------------------------------
// Row 3: NoHeader with supplied names.

#[tokio::test]
async fn no_header_supplied_columns_name_the_positions() {
    let options = CsvOptions::new(HeaderMode::Absent, b',')
        .read_mapping(ReadMapping::columns(["name", "id"]));
    let (rows, error) = read::<Row>(options.clone(), b"Lyon,1\nZo\xc3\xab,2\n").await;
    assert!(error.is_none());
    assert_eq!(rows, [row(1, "Lyon"), row(2, "Zoë")]);

    // The supplied columns are the table width.
    let (rows, error) = read::<Row>(options, b"Lyon,1\nx,2,extra\n").await;
    assert_eq!(rows, [row(1, "Lyon")]);
    let error = error.unwrap();
    assert_eq!(
        label(&error),
        "a CSV record does not have as many fields as the columns"
    );
    assert_eq!(
        error.codec().unwrap().context(),
        [("record", 2), ("fields", 3), ("expected", 2)]
    );
}

/// A static headerless profile whose columns name the positions.
struct NameFirst;
impl CsvProfile for NameFirst {
    fn options() -> CsvProfileOptions {
        CsvProfileOptions::default().columns(["name", "id"])
    }
}

#[test]
fn a_headerless_profile_with_columns_maps_by_name() {
    assert_eq!(
        decode::<Csv<NoHeader, concord_extras::csv::Comma, NameFirst>, Row>(b"Lyon,1\n"),
        Ok(vec![row(1, "Lyon")])
    );
}

// ---------------------------------------------------------------------------------------------
// Row 4: HasHeader with explicit positional mapping; header validation by the profile.

#[tokio::test]
async fn has_header_positional_ignores_the_names_but_consumes_the_header() {
    let options = CsvOptions::new(HeaderMode::Present, b',').read_mapping(ReadMapping::Positional);
    let (rows, error) = read::<Row>(options, b"anything,else\n1,Lyon\n").await;
    assert!(error.is_none());
    assert_eq!(rows, [row(1, "Lyon")]);
}

#[tokio::test]
async fn supplied_columns_with_a_header_are_refused_as_ambiguous() {
    let options = CsvOptions::new(HeaderMode::Present, b',')
        .read_mapping(ReadMapping::columns(["id", "name"]));
    let error = CsvReader::new(options).unwrap_err();
    assert_eq!(
        error.label(),
        "ReadMapping::Columns with a header is ambiguous: the header names the columns (validate it with the profile's columns)"
    );

    struct Ambiguous;
    impl CsvProfile for Ambiguous {
        fn options() -> CsvProfileOptions {
            CsvProfileOptions::default().read_mapping(ReadMapping::columns(["id", "name"]))
        }
    }
    assert_eq!(
        decode::<Csv<HasHeader, concord_extras::csv::Comma, Ambiguous>, Row>(b"id,name\n1,a\n"),
        Err(error.label())
    );
}

#[tokio::test]
async fn profile_columns_validate_the_wire_header_without_replacing_it() {
    let options = CsvOptions::new(HeaderMode::Present, b',').columns(["id", "name"]);
    let (rows, error) = read::<Row>(options.clone(), b"id,name\n1,Lyon\n").await;
    assert!(error.is_none());
    assert_eq!(rows, [row(1, "Lyon")]);

    for (body, field) in [
        (&b"name,id\nLyon,1\n"[..], 1),
        (b"id,name,extra\n1,Lyon,x\n", 3),
        (b"id\n1\n", 2),
    ] {
        let (rows, error) = read::<Row>(options.clone(), body).await;
        assert!(rows.is_empty());
        let error = error.unwrap();
        assert_eq!(error.kind(), ReadErrorKind::Format);
        assert_eq!(
            label(&error),
            "the CSV header does not match the expected columns"
        );
        assert_eq!(error.codec().unwrap().get("field"), Some(field));
    }
    // With positional mapping too.
    let (_, error) = read::<Row>(
        options.read_mapping(ReadMapping::Positional),
        b"x,y\n1,Lyon\n",
    )
    .await;
    assert_eq!(
        label(&error.unwrap()),
        "the CSV header does not match the expected columns"
    );
}

// ---------------------------------------------------------------------------------------------
// §4 defaults: empty inputs, header only, empty and whitespace fields.

#[tokio::test]
async fn empty_and_header_only_inputs() {
    let present = CsvOptions::new(HeaderMode::Present, b',');
    let absent = CsvOptions::new(HeaderMode::Absent, b',');
    let comments = present.clone().comment(Some(b'#'));

    let (rows, error) = read::<Row>(present.clone(), b"id,name\n").await;
    assert!(rows.is_empty() && error.is_none());
    let (rows, error) = read::<Row>(present.clone(), b"id,name").await;
    assert!(rows.is_empty() && error.is_none());

    for (options, body) in [
        (present.clone(), &b""[..]),
        (present.clone(), b"\n\r\n\n"),
        (comments, b"# only\n#comments\n\n"),
    ] {
        let (rows, error) = read::<Row>(options, body).await;
        assert!(rows.is_empty());
        let error = error.unwrap();
        assert_eq!(error.kind(), ReadErrorKind::Format);
        assert_eq!(label(&error), "the CSV input has no header record");
    }
    assert_eq!(
        decode::<Csv, Row>(b""),
        Err("the CSV input has no header record")
    );

    let (rows, error) = read::<Row>(absent.clone(), b"").await;
    assert!(rows.is_empty() && error.is_none());
    let (rows, error) = read::<Row>(absent, b"\n\n").await;
    assert!(rows.is_empty() && error.is_none());
    assert_eq!(decode::<Csv<NoHeader>, Row>(b""), Ok(vec![]));
}

#[tokio::test]
async fn empty_and_whitespace_fields_are_values() {
    #[derive(Debug, PartialEq, Deserialize)]
    struct Opt {
        id: u32,
        name: Option<String>,
        note: String,
    }
    let body = b"id,name,note\n1,,   \n2,\"\",\" x \"\n";
    let (rows, error) = read::<Opt>(CsvOptions::new(HeaderMode::Present, b','), body).await;
    assert!(error.is_none());
    assert_eq!(
        rows,
        [
            Opt {
                id: 1,
                name: None,
                note: "   ".into()
            },
            Opt {
                id: 2,
                name: None,
                note: " x ".into()
            }
        ]
    );
    let (strings, _) = read::<Vec<String>>(CsvOptions::new(HeaderMode::Present, b','), body).await;
    assert_eq!(strings[0], ["1", "", "   "]);
}

// ---------------------------------------------------------------------------------------------
// UTF-8: typed rows refuse invalid sequences; raw records keep them.

#[tokio::test]
async fn invalid_utf8_is_a_format_error_for_typed_rows_and_bytes_for_raw_records() {
    let body = b"id,name\n1,Lyon\n2,ab\xffcd\n3,ok\n";
    let (rows, error) = read::<Row>(CsvOptions::new(HeaderMode::Present, b','), body).await;
    assert_eq!(rows, [row(1, "Lyon")]);
    let error = error.unwrap();
    assert_eq!(error.kind(), ReadErrorKind::Format);
    assert_eq!(label(&error), "a CSV field is not valid UTF-8");
    assert_eq!(
        error.codec().unwrap().context(),
        [("record", 2), ("field", 2)]
    );
    assert_eq!(error.position().records, 1);

    // Even a field the row type ignores.
    #[derive(Deserialize)]
    #[allow(dead_code)]
    struct IdOnly {
        id: u32,
    }
    let (_, error) = read::<IdOnly>(CsvOptions::new(HeaderMode::Present, b','), body).await;
    assert_eq!(label(&error.unwrap()), "a CSV field is not valid UTF-8");

    let reader = CsvReader::new(CsvOptions::new(HeaderMode::Present, b',')).unwrap();
    let mut raw = reader.read_raw(Chunks::new([body.to_vec()]));
    let mut fields = Vec::new();
    while let Some(record) = raw.next().await {
        fields.push(record.unwrap().get(1).unwrap().to_vec());
    }
    assert_eq!(
        fields,
        [b"Lyon".to_vec(), b"ab\xffcd".to_vec(), b"ok".to_vec()]
    );
    let table = decode_raw::<Csv>(body);
    assert_eq!(table.records[1].get(1), Some(&b"ab\xffcd"[..]));
    assert_eq!(
        table.records[1].to_strings().unwrap_err().label(),
        "a CSV field is not valid UTF-8"
    );

    // An invalid header is refused by named mapping, kept by raw reading.
    let (_, error) = read::<Row>(
        CsvOptions::new(HeaderMode::Present, b','),
        b"i\xffd,name\n1,a\n",
    )
    .await;
    assert_eq!(label(&error.unwrap()), "the CSV header is not valid UTF-8");
}

#[tokio::test]
async fn a_decode_error_names_the_record_and_field_without_the_value() {
    let body = b"id,name\n1,Lyon\nnot-a-number-s3cr3t,x\n";
    let (rows, error) = read::<Row>(CsvOptions::new(HeaderMode::Present, b','), body).await;
    assert_eq!(rows, [row(1, "Lyon")]);
    let error = error.unwrap();
    assert_eq!(error.kind(), ReadErrorKind::Decode);
    assert_eq!(
        error.codec().unwrap().context(),
        [("record", 2), ("field", 1)]
    );
    let shown = format!("{error} {error:?}");
    assert!(!shown.contains("s3cr3t"), "{shown}");
    assert_eq!(
        error.to_string(),
        "a record does not decode: a CSV record does not decode into the requested row type (record 2, field 1) (after 1 records, 37 bytes)"
    );
    // The unredacted source is deliberate access only.
    assert!(error.source_unredacted().is_some());
}
