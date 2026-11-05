use astra_recheck_probes::drain;
use concord_core::Encode;
use concord_extras::{csv::*, stream::from_iter};
use serde::{
    Serialize, Serializer,
    ser::{SerializeSeq, SerializeStruct},
};
use std::{
    cell::Cell,
    convert::Infallible,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering::SeqCst},
    },
};
struct Changing {
    calls: Cell<usize>,
    total: Arc<AtomicUsize>,
    wrong: bool,
}
impl Serialize for Changing {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let n = self.calls.get();
        self.calls.set(n + 1);
        self.total.fetch_add(1, SeqCst);
        let mut r = s.serialize_struct("Changing", 1)?;
        r.serialize_field(
            if n == 0 && !self.wrong { "id" } else { "other" },
            &if n == 0 { 7 } else { 88 },
        )?;
        r.end()
    }
}
fn changing(t: &Arc<AtomicUsize>, wrong: bool) -> Changing {
    Changing {
        calls: Cell::new(0),
        total: t.clone(),
        wrong,
    }
}
#[test]
fn once_schema_and_header_policy() {
    let mut cases = 0;
    for header in [HeaderMode::Present, HeaderMode::Absent] {
        for columns in [false, true] {
            for lazy in [false, true] {
                let total = Arc::new(AtomicUsize::new(0));
                let data = vec![changing(&total, false), changing(&total, false)];
                let writer = CsvWriter::new(CsvOptions::new(header, b',')).unwrap();
                let out = if lazy {
                    let mut rows =
                        StreamRows::new(from_iter(data.into_iter().map(Ok::<_, Infallible>)));
                    if columns {
                        rows = rows.with_columns(["id"]);
                    }
                    let body = writer.upload(rows).unwrap().into_parts().0;
                    assert_eq!(total.load(SeqCst), 0);
                    drain(body).unwrap()
                } else {
                    let mut rows = Rows::new(data);
                    if columns {
                        rows = rows.with_columns(["id"]);
                    }
                    writer.to_vec(rows).unwrap()
                };
                assert_eq!(total.load(SeqCst), 2);
                assert_eq!(
                    out,
                    if header == HeaderMode::Present {
                        b"id\r\n7\r\n7\r\n".to_vec()
                    } else {
                        b"7\r\n7\r\n".to_vec()
                    }
                );
                let data = vec![changing(&total, false), changing(&total, true)];
                let err = writer.to_vec(Rows::new(data)).unwrap_err();
                assert_eq!(err.get("record"), Some(2));
                assert!(err.label().contains("field names"));
                cases += 1;
            }
        }
    }
    let total = Arc::new(AtomicUsize::new(0));
    let body = <Csv as Encode<Vec<Changing>>>::encode(vec![changing(&total, false)]).unwrap();
    assert_eq!(body.as_bytes().unwrap(), b"id\r\n7\r\n");
    assert_eq!(total.load(SeqCst), 1);
    let total = Arc::new(AtomicUsize::new(0));
    let rows = StreamRows::new(from_iter(vec![Ok::<_, Infallible>(changing(
        &total, false,
    ))]));
    let body = <Csv as Encode<StreamRows<_>>>::encode(rows).unwrap();
    assert_eq!(total.load(SeqCst), 0);
    assert_eq!(drain(body).unwrap(), b"id\r\n7\r\n");
    assert_eq!(total.load(SeqCst), 1);
    println!(
        "one row serialization: {cases} header/explicit/lazy combinations plus static finite/lazy Encode; changed schema refused"
    );
}
struct Many(Arc<AtomicUsize>);
impl Serialize for Many {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut seq = s.serialize_seq(Some(1000))?;
        for _ in 0..1000 {
            self.0.fetch_add(1, SeqCst);
            seq.serialize_element(&1)?;
        }
        seq.end()
    }
}
#[test]
fn encoded_limits_and_short_circuit() {
    let calls = Arc::new(AtomicUsize::new(0));
    let e = CsvWriter::new(CsvOptions::new(HeaderMode::Absent, b',').max_fields(3))
        .unwrap()
        .to_vec(Rows::new([Many(calls.clone())]))
        .unwrap_err();
    assert_eq!(calls.load(SeqCst), 4);
    assert_eq!(e.get("limit"), Some(3));
    let mut checks = 0;
    for term in [LineEnding::Lf, LineEnding::CrLf] {
        for double in [false, true] {
            for header in [HeaderMode::Present, HeaderMode::Absent] {
                let base = CsvOptions::new(header, b',')
                    .escape(Some(b'\\'))
                    .double_quote(double)
                    .write_terminator(term);
                let row = ("\\\",", "");
                let cols = ["h\\\",", "j"];
                let full = CsvWriter::new(base.clone())
                    .unwrap()
                    .to_vec(Rows::new([row]).with_columns(cols))
                    .unwrap();
                let termlen = if term == LineEnding::Lf { 1 } else { 2 };
                let encoded_row = CsvWriter::new(
                    CsvOptions::new(HeaderMode::Absent, b',')
                        .escape(Some(b'\\'))
                        .double_quote(double)
                        .write_terminator(term),
                )
                .unwrap()
                .to_vec(Rows::new([row]))
                .unwrap();
                let needed = encoded_row.len() - termlen;
                for max in 1..=needed + 2 {
                    let w = CsvWriter::new(base.clone().max_record_bytes(max)).unwrap();
                    let result = w.to_vec(Rows::new([row]).with_columns(cols));
                    assert_eq!(result.is_ok(), max >= needed);
                    if let Err(e) = result {
                        assert_eq!(e.get("limit"), Some(max as u64));
                        assert_eq!(e.get("record"), Some(1));
                    }
                    let upload = w
                        .upload(
                            StreamRows::new(from_iter([Ok::<_, Infallible>(row)]))
                                .with_columns(cols),
                        )
                        .unwrap();
                    assert_eq!(drain(upload.into_parts().0).is_ok(), max >= needed);
                    checks += 2;
                }
                if header == HeaderMode::Present {
                    let headlen = full.len() - encoded_row.len() - termlen;
                    for max in 1..=headlen + 2 {
                        let w = CsvWriter::new(base.clone().max_header_bytes(max)).unwrap();
                        let output = w.to_vec(Rows::new([row]).with_columns(cols));
                        assert_eq!(output.is_ok(), max >= headlen);
                        checks += 1;
                    }
                }
            }
        }
    }
    println!(
        "encoded record/header limit cases={checks}; quotes/escapes/delimiters counted, LF/CRLF excluded, max_fields stops at attempt 4"
    );
}
