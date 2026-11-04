//! The handoff's synthetic CSV seeds (C01–C15, see `tests/data/csv/PROVENANCE.md`): their exact
//! bytes, then their documented expectations on the finite route, the incremental route, every
//! single split point, every two-split partition, one byte per chunk and pseudo-random
//! partitions. CSV.md §8 items 6, 9, 11, 16 (seed part).

#![cfg(feature = "csv")]

mod csv_support;

use concord_extras::csv::{
    Comma, Csv, CsvOptions, CsvProfile, CsvProfileOptions, CsvReader, Delimiter, HasHeader,
    HeaderMode, NoHeader, ReadMapping, Semicolon, Tab,
};
use concord_extras::stream::{Chunks, ReadErrorKind};
use csv_support::{
    Outcome, byte_chunks, double_splits, finite, random_partitions, single_splits, streamed,
};

struct Case {
    id: &'static str,
    bytes: &'static [u8],
    literal: &'static [u8],
    options: CsvOptions,
    finite: fn(&[u8]) -> Outcome,
    expected: Outcome,
}

/// C04's static profile: headerless, columns `id,name`.
struct Supplied;
impl CsvProfile for Supplied {
    fn options() -> CsvProfileOptions {
        CsvProfileOptions::default().columns(["id", "name"])
    }
}

fn cases() -> Vec<Case> {
    use ReadErrorKind::Format;
    let present = |d| CsvOptions::new(HeaderMode::Present, d);
    let absent = |d| CsvOptions::new(HeaderMode::Absent, d);
    vec![
        Case {
            id: "C01",
            bytes: include_bytes!("data/csv/comma.csv"),
            literal: "id,name\r\n1,Lyon\r\n2,Zoë\r\n".as_bytes(),
            options: present(b','),
            finite: finite::<Csv>,
            expected: Outcome::ok(Some(&["id", "name"]), &[&["1", "Lyon"], &["2", "Zoë"]]),
        },
        Case {
            id: "C02",
            bytes: include_bytes!("data/csv/semicolon_multiline.csv"),
            literal: b"id;note\r\n1;\"alpha;beta\ncontinued\"\r\n2;\"said \"\"yes\"\"\"\r\n",
            options: present(b';'),
            finite: finite::<Csv<HasHeader, Semicolon>>,
            expected: Outcome::ok(
                Some(&["id", "note"]),
                &[&["1", "alpha;beta\ncontinued"], &["2", "said \"yes\""]],
            ),
        },
        Case {
            id: "C03",
            bytes: include_bytes!("data/csv/headerless.csv"),
            literal: b"001,Alice\n002,Bob\n",
            options: absent(b','),
            finite: finite::<Csv<NoHeader>>,
            expected: Outcome::ok(None, &[&["001", "Alice"], &["002", "Bob"]]),
        },
        Case {
            id: "C04",
            bytes: include_bytes!("data/csv/supplied_columns.csv"),
            literal: b"001;Alice\n002;Bob\n",
            options: absent(b';').read_mapping(ReadMapping::columns(["id", "name"])),
            finite: finite::<Csv<NoHeader, Semicolon, Supplied>>,
            expected: Outcome::ok(None, &[&["001", "Alice"], &["002", "Bob"]]),
        },
        Case {
            id: "C05",
            bytes: include_bytes!("data/csv/tab.csv"),
            literal: "id\tname\n1\t東京\n".as_bytes(),
            options: present(b'\t'),
            finite: finite::<Csv<HasHeader, Tab>>,
            expected: Outcome::ok(Some(&["id", "name"]), &[&["1", "東京"]]),
        },
        Case {
            id: "C06",
            bytes: include_bytes!("data/csv/unit_separator.csv"),
            literal: b"1\x1fhello\n2\x1fworld\n",
            options: absent(0x1f),
            finite: finite::<Csv<NoHeader, Delimiter<0x1f>>>,
            expected: Outcome::ok(None, &[&["1", "hello"], &["2", "world"]]),
        },
        Case {
            id: "C07",
            bytes: include_bytes!("data/csv/multiline_header.csv"),
            literal: b"\"display\nname\",id\r\n\"A,B\",1\r\n",
            options: present(b','),
            finite: finite::<Csv>,
            expected: Outcome::ok(Some(&["display\nname", "id"]), &[&["A,B", "1"]]),
        },
        Case {
            id: "C08",
            bytes: include_bytes!("data/csv/bom.csv"),
            literal: b"\xef\xbb\xbfid,name\n1,Alice\n",
            options: present(b','),
            finite: finite::<Csv>,
            expected: Outcome::ok(Some(&["id", "name"]), &[&["1", "Alice"]]),
        },
        Case {
            id: "C09",
            bytes: include_bytes!("data/csv/header_only.csv"),
            literal: b"id,name\r\n",
            options: present(b','),
            finite: finite::<Csv>,
            expected: Outcome::ok(Some(&["id", "name"]), &[]),
        },
        Case {
            id: "C10",
            bytes: include_bytes!("data/csv/empty_headed.csv"),
            literal: b"",
            options: present(b','),
            finite: finite::<Csv>,
            expected: Outcome::ok(None, &[]).failing(Format, "the CSV input has no header record"),
        },
        Case {
            id: "C11",
            bytes: include_bytes!("data/csv/empty_headerless.csv"),
            literal: b"",
            options: absent(b','),
            finite: finite::<Csv<NoHeader, Comma>>,
            expected: Outcome::ok(None, &[]),
        },
        Case {
            id: "C12",
            bytes: include_bytes!("data/csv/duplicate_header.csv"),
            literal: b"id,id\n1,2\n",
            options: present(b','),
            finite: finite::<Csv>,
            expected: Outcome::ok(None, &[]).failing(Format, "the CSV header repeats a name"),
        },
        Case {
            id: "C13",
            bytes: include_bytes!("data/csv/width_error.csv"),
            literal: b"id,name\n1,Alice\n2\n",
            options: present(b','),
            finite: finite::<Csv>,
            expected: Outcome::ok(Some(&["id", "name"]), &[&["1", "Alice"]]).failing(
                Format,
                "a CSV record does not have as many fields as the header",
            ),
        },
        Case {
            id: "C14",
            bytes: include_bytes!("data/csv/invalid_utf8.csv"),
            literal: b"id,name\n1,\xff\n",
            options: present(b','),
            finite: finite::<Csv>,
            expected: Outcome::ok(Some(&["id", "name"]), &[])
                .failing(Format, "a CSV field is not valid UTF-8"),
        },
        Case {
            id: "C15",
            bytes: include_bytes!("data/csv/whitespace_and_hash.csv"),
            literal: b"id,name\n1,  value  \n2,#ordinary data\n",
            options: present(b','),
            finite: finite::<Csv>,
            expected: Outcome::ok(
                Some(&["id", "name"]),
                &[&["1", "  value  "], &["2", "#ordinary data"]],
            ),
        },
    ]
}

#[test]
fn the_seeds_are_the_documented_bytes() {
    for case in cases() {
        assert_eq!(case.bytes, case.literal, "{}", case.id);
    }
}

#[tokio::test]
async fn every_seed_reads_as_documented_on_the_incremental_route() {
    for case in cases() {
        let reader = CsvReader::new(case.options.clone()).unwrap();
        let whole = streamed(&reader, Chunks::new([case.bytes.to_vec()])).await;
        assert_eq!(whole, case.expected, "{}", case.id);
    }
}

#[test]
fn every_seed_reads_as_documented_on_the_finite_route() {
    for case in cases() {
        let got = (case.finite)(case.bytes);
        let expected = case.expected.clone().labels_only();
        if expected.error.is_some() {
            // The finite route returns the error alone: no partial rows.
            assert_eq!(got.error, expected.error, "{}", case.id);
            assert!(got.records.is_empty(), "{}", case.id);
        } else {
            assert_eq!(got, expected, "{}", case.id);
        }
    }
}

#[tokio::test]
async fn every_split_point_and_partition_gives_the_same_outcome() {
    for case in cases() {
        let reader = CsvReader::new(case.options.clone()).unwrap();
        let mut partitions = single_splits(case.bytes);
        partitions.extend(double_splits(case.bytes));
        partitions.push(byte_chunks(case.bytes));
        partitions.extend(random_partitions(
            case.bytes,
            64,
            0x5eed ^ case.bytes.len() as u64,
        ));
        for chunks in partitions {
            let got = streamed(&reader, Chunks::new(chunks.clone())).await;
            assert_eq!(got, case.expected, "{} {:?}", case.id, chunks);
        }
    }
}

#[derive(Debug, PartialEq, serde::Deserialize)]
struct Named {
    id: String,
    name: String,
}

#[tokio::test]
async fn named_and_supplied_mappings_of_the_seeds_on_every_split() {
    let expect = |pairs: &[(&str, &str)]| -> Vec<Named> {
        pairs
            .iter()
            .map(|(id, name)| Named {
                id: (*id).into(),
                name: (*name).into(),
            })
            .collect()
    };
    // C01 (named from the header), C04 (supplied columns, headerless), C08 (named after a BOM).
    let seeds: [(&[u8], CsvOptions, Vec<Named>); 3] = [
        (
            include_bytes!("data/csv/comma.csv"),
            CsvOptions::new(HeaderMode::Present, b','),
            expect(&[("1", "Lyon"), ("2", "Zoë")]),
        ),
        (
            include_bytes!("data/csv/supplied_columns.csv"),
            CsvOptions::new(HeaderMode::Absent, b';')
                .read_mapping(ReadMapping::columns(["id", "name"])),
            expect(&[("001", "Alice"), ("002", "Bob")]),
        ),
        (
            include_bytes!("data/csv/bom.csv"),
            CsvOptions::new(HeaderMode::Present, b','),
            expect(&[("1", "Alice")]),
        ),
    ];
    for (bytes, options, expected) in seeds {
        let reader = CsvReader::new(options).unwrap();
        for chunks in single_splits(bytes) {
            let mut rows = reader.read::<Named, _>(Chunks::new(chunks));
            let mut got = Vec::new();
            while let Some(row) = rows.next().await {
                got.push(row.unwrap());
            }
            assert_eq!(got, expected);
        }
    }
    // The static C04 profile (columns as the names of a headerless table) decodes the same.
    let finite =
        <Csv<NoHeader, Semicolon, Supplied> as concord_core::Decode<Vec<Named>>>::decode_body(
            concord_extras::csv::CsvContext::of::<NoHeader, Semicolon, Supplied>().unwrap(),
            concord_core::bytes::Bytes::from_static(include_bytes!(
                "data/csv/supplied_columns.csv"
            )),
        )
        .unwrap();
    assert_eq!(finite, expect(&[("001", "Alice"), ("002", "Bob")]));
}

#[tokio::test]
async fn static_aliases_and_runtime_options_read_the_seeds_equally() {
    let pairs: [(CsvOptions, CsvOptions, &[u8]); 5] = [
        (
            Csv::<HasHeader>::options(),
            CsvOptions::new(HeaderMode::Present, b','),
            include_bytes!("data/csv/comma.csv"),
        ),
        (
            Csv::<HasHeader, Semicolon>::options(),
            CsvOptions::new(HeaderMode::Present, b';'),
            include_bytes!("data/csv/semicolon_multiline.csv"),
        ),
        (
            Csv::<NoHeader>::options(),
            CsvOptions::new(HeaderMode::Absent, b','),
            include_bytes!("data/csv/headerless.csv"),
        ),
        (
            Csv::<HasHeader, Tab>::options(),
            CsvOptions::new(HeaderMode::Present, b'\t'),
            include_bytes!("data/csv/tab.csv"),
        ),
        (
            Csv::<NoHeader, Delimiter<0x1f>>::options(),
            CsvOptions::new(HeaderMode::Absent, 0x1f),
            include_bytes!("data/csv/unit_separator.csv"),
        ),
    ];
    for (fixed, runtime, bytes) in pairs {
        assert_eq!(fixed, runtime);
        let a = streamed(
            &CsvReader::new(fixed).unwrap(),
            Chunks::new([bytes.to_vec()]),
        )
        .await;
        let b = streamed(
            &CsvReader::new(runtime).unwrap(),
            Chunks::new([bytes.to_vec()]),
        )
        .await;
        assert_eq!(a, b);
    }
}
