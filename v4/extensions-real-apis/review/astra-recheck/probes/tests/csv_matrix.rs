use astra_recheck_probes::drain;
use concord_extras::csv::*;
use concord_extras::stream::from_iter;
use serde::{Serialize, Serializer};
use std::convert::Infallible;
#[derive(Serialize)]
struct Unit;
#[derive(Serialize)]
struct Wrap(i128);
#[derive(Serialize)]
enum Kind {
    Unit,
    New(u64),
}
struct Blob(Vec<u8>);
impl Serialize for Blob {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(&self.0)
    }
}
#[derive(Serialize)]
struct All {
    a: i8,
    b: i16,
    c: i32,
    d: i64,
    e: i128,
    f: u8,
    g: u16,
    h: u32,
    i: u64,
    j: u128,
    k: f32,
    l: f64,
    m: bool,
    n: char,
    o: Option<u8>,
    p: Option<u8>,
    q: (),
    r: Unit,
    s: Wrap,
    t: Kind,
    u: Kind,
    v: String,
    w: Blob,
}
fn rows() -> Vec<All> {
    let mut seed = 0x4256317ae89u64;
    (0..128)
        .map(|i| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            All {
                a: i8::MIN,
                b: i16::MIN,
                c: i32::MIN,
                d: i64::MIN,
                e: i128::MIN,
                f: u8::MAX,
                g: u16::MAX,
                h: u32::MAX,
                i: u64::MAX,
                j: u128::MAX,
                k: match i {
                    0 => -0.0,
                    1 => f32::INFINITY,
                    2 => f32::NEG_INFINITY,
                    3 => f32::NAN,
                    _ => f32::from_bits(seed as u32),
                },
                l: match i {
                    0 => -0.0,
                    1 => f64::INFINITY,
                    2 => f64::NEG_INFINITY,
                    3 => f64::NAN,
                    _ => f64::from_bits(seed),
                },
                m: i % 2 == 0,
                n: 'é',
                o: None,
                p: Some(3),
                q: (),
                r: Unit,
                s: Wrap(-7),
                t: Kind::Unit,
                u: Kind::New(99),
                v: [
                    "", "a,b;\t|", "#comment", "line\r\n", "'\"", "NaN", "1e-5", "é水", "\0",
                ][i % 9]
                    .into(),
                w: Blob(vec![0, 0xff, b',', b'\n', b'\'', b'"']),
            }
        })
        .collect()
}
#[test]
fn csv_crate_byte_matrix() {
    let data = rows();
    let mut cases = 0;
    for delimiter in [b',', b';', b'\t', b'|', 0, b'1'] {
        for quote in [b'"', b'\'', b'q'] {
            for style in [
                QuoteStyle::Necessary,
                QuoteStyle::Always,
                QuoteStyle::NonNumeric,
                QuoteStyle::Never,
            ] {
                for ending in [LineEnding::CrLf, LineEnding::Lf] {
                    for mode in 0..3 {
                        for header in [HeaderMode::Absent, HeaderMode::Present] {
                            let escape = (mode > 0).then_some(b'~');
                            let double_quote = mode != 2;
                            let options = CsvOptions::new(header, delimiter)
                                .quote(quote)
                                .quote_style(style)
                                .write_terminator(ending)
                                .escape(escape)
                                .double_quote(double_quote)
                                .comment(Some(b'#'));
                            let mut builder = csv::WriterBuilder::new();
                            builder
                                .has_headers(header == HeaderMode::Present)
                                .delimiter(delimiter)
                                .quote(quote)
                                .double_quote(double_quote)
                                .comment(Some(b'#'))
                                .quote_style(match style {
                                    QuoteStyle::Always => csv::QuoteStyle::Always,
                                    QuoteStyle::NonNumeric => csv::QuoteStyle::NonNumeric,
                                    QuoteStyle::Never => csv::QuoteStyle::Never,
                                    _ => csv::QuoteStyle::Necessary,
                                })
                                .terminator(match ending {
                                    LineEnding::CrLf => csv::Terminator::CRLF,
                                    _ => csv::Terminator::Any(b'\n'),
                                });
                            if let Some(e) = escape {
                                builder.escape(e);
                            }
                            let mut old = builder.from_writer(Vec::new());
                            for row in &data {
                                old.serialize(row).unwrap();
                            }
                            let old = old.into_inner().unwrap();
                            let new = CsvWriter::new(options.clone())
                                .unwrap()
                                .to_vec(Rows::new(&data))
                                .unwrap();
                            assert_eq!(new, old, "options={options:?}");
                            cases += 1;
                        }
                    }
                }
            }
        }
    }
    println!(
        "csv-crate compatibility: {cases} dialect/header configurations x {} all-type rows",
        data.len()
    );
}
#[test]
fn empty_fields_and_empty_tables() {
    for style in [
        QuoteStyle::Necessary,
        QuoteStyle::Always,
        QuoteStyle::NonNumeric,
        QuoteStyle::Never,
    ] {
        let options = CsvOptions::new(HeaderMode::Absent, b',').quote_style(style);
        let writer = CsvWriter::new(options).unwrap();
        for row in [
            vec![""],
            vec!["", ""],
            vec!["", "", ""],
            vec!["1", ""],
            vec!["", "1"],
        ] {
            let mut old = csv::WriterBuilder::new()
                .has_headers(false)
                .terminator(csv::Terminator::CRLF)
                .quote_style(match style {
                    QuoteStyle::Always => csv::QuoteStyle::Always,
                    QuoteStyle::NonNumeric => csv::QuoteStyle::NonNumeric,
                    QuoteStyle::Never => csv::QuoteStyle::Never,
                    _ => csv::QuoteStyle::Necessary,
                })
                .from_writer(Vec::new());
            old.serialize(&row).unwrap();
            assert_eq!(
                writer.to_vec(Rows::new([row])).unwrap(),
                old.into_inner().unwrap(),
                "{style:?}"
            );
        }
    }
    for header in [HeaderMode::Absent, HeaderMode::Present] {
        for explicit in [false, true] {
            for lazy in [false, true] {
                let w = CsvWriter::new(CsvOptions::new(header, b',')).unwrap();
                let output = if lazy {
                    let mut rows =
                        StreamRows::new(from_iter(Vec::<Result<(u8,), Infallible>>::new()));
                    if explicit {
                        rows = rows.with_columns(["id"]);
                    }
                    drain(w.upload(rows).unwrap().into_parts().0).map_err(|e| e.to_string())
                } else {
                    let mut rows = Rows::new(Vec::<(u8,)>::new());
                    if explicit {
                        rows = rows.with_columns(["id"]);
                    }
                    w.to_vec(rows).map_err(|e| e.to_string())
                };
                if header == HeaderMode::Present && !explicit {
                    assert!(output.is_err());
                } else {
                    assert_eq!(
                        output.unwrap(),
                        if header == HeaderMode::Present {
                            b"id\r\n".to_vec()
                        } else {
                            vec![]
                        }
                    );
                }
            }
        }
    }
    println!(
        "single/multiple empty fields match csv; 8 empty-table header/columns/lazy combinations pass"
    );
}
fn core_read(
    data: &[u8],
    delimiter: u8,
    quote: u8,
    escape: u8,
    double_quote: bool,
) -> Vec<Vec<Vec<u8>>> {
    let mut reader = csv_core::ReaderBuilder::new()
        .delimiter(delimiter)
        .quote(quote)
        .escape(Some(escape))
        .double_quote(double_quote)
        .build();
    let mut at = 0;
    let mut out = vec![0; data.len() + 1];
    let mut ends = vec![0; 10000];
    let mut records = Vec::new();
    loop {
        let (result, nread, nout, nends) = reader.read_record(&data[at..], &mut out, &mut ends);
        at += nread;
        match result {
            csv_core::ReadRecordResult::Record => {
                let mut p = 0;
                records.push(
                    ends[..nends]
                        .iter()
                        .map(|&end| {
                            let f = out[p..end].to_vec();
                            p = end;
                            f
                        })
                        .collect(),
                );
                assert_eq!(p, nout);
            }
            csv_core::ReadRecordResult::End => break,
            csv_core::ReadRecordResult::InputEmpty => {}
            other => panic!("{other:?}"),
        }
    }
    records
}
#[test]
fn escapes_against_direct_csv_core_reader() {
    let mut cases = 0;
    for (delimiter, quote, escape) in [(b',', b'"', b'\\'), (b';', b'\'', b'~'), (b'|', b'q', 0)] {
        let alphabet = [b'a', delimiter, quote, escape, b'\r', b'\n', 0xff];
        let mut vals = vec![vec![]];
        let mut level = vec![vec![]];
        for _ in 0..4 {
            level = level
                .into_iter()
                .flat_map(|v| {
                    alphabet.map(|b| {
                        let mut n = v.clone();
                        n.push(b);
                        n
                    })
                })
                .collect();
            vals.extend(level.clone());
        }
        for double in [false, true] {
            for style in [
                QuoteStyle::Necessary,
                QuoteStyle::Always,
                QuoteStyle::NonNumeric,
            ] {
                for ending in [LineEnding::Lf, LineEnding::CrLf] {
                    let options = CsvOptions::new(HeaderMode::Absent, delimiter)
                        .quote(quote)
                        .escape(Some(escape))
                        .double_quote(double)
                        .quote_style(style)
                        .write_terminator(ending);
                    let raw: Vec<_> = vals.iter().map(|v| RawRecord::new([v])).collect();
                    let writer = CsvWriter::new(options).unwrap();
                    let finite = writer.to_vec(Rows::new(&raw)).unwrap();
                    let lazy = drain(
                        writer
                            .upload(StreamRows::new(from_iter(
                                raw.into_iter().map(Ok::<_, Infallible>),
                            )))
                            .unwrap()
                            .into_parts()
                            .0,
                    )
                    .unwrap();
                    assert_eq!(finite, lazy);
                    let expected: Vec<_> = vals.iter().map(|v| vec![v.clone()]).collect();
                    assert_eq!(
                        core_read(&finite, delimiter, quote, escape, double),
                        expected
                    );
                    cases += 1;
                }
            }
        }
    }
    println!(
        "direct csv-core roundtrip: {cases} dialects x 2801 arbitrary-byte values; finite == lazy"
    );
}
