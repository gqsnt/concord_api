use serde::ser::{Serialize, Serializer, SerializeStruct};
use std::{convert::Infallible, pin::Pin, task::{Context, Poll, Waker}};
use http_body::Body;

const NAMES: [&str; 8] = ["id", "comma,name", "semi;name", "tab\tname", "line\nname", "quote\"'name", "escape\\name", "unicode❤"];
struct Row;
impl Serialize for Row {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok,S::Error> {
        let mut row = serializer.serialize_struct("Row", NAMES.len())?;
        for (i, name) in NAMES.iter().enumerate() { row.serialize_field(name, &format!("{i}={name}"))?; }
        row.end()
    }
}
fn bytes(mut body: concord::reqwest::Body) -> Vec<u8> {
    let mut out = Vec::new();
    let mut cx = Context::from_waker(Waker::noop());
    loop { match Pin::new(&mut body).poll_frame(&mut cx) {
        Poll::Ready(Some(Ok(frame))) => out.extend(frame.into_data().unwrap()),
        Poll::Ready(None) => return out,
        other => panic!("unexpected body poll {other:?}"),
    }}
}
macro_rules! implementation {
    ($module:ident, $lib:ident) => { mod $module {
        use super::*;
        use $lib::csv::{CsvWriter, CsvOptions, HeaderMode, QuoteStyle, LineEnding, Rows, StreamRows};
        use $lib::stream::from_iter;
        pub fn encode(delimiter:u8, quote:u8, style:usize, double:bool, lf:bool, header:bool, route:usize) -> (Vec<u8>, Vec<u8>) {
            let options = CsvOptions::new(if header {HeaderMode::Present} else {HeaderMode::Absent}, delimiter)
                .quote(quote).quote_style([QuoteStyle::Necessary,QuoteStyle::Always,QuoteStyle::NonNumeric,QuoteStyle::Never][style])
                .escape(Some(b'\\')).double_quote(double).comment(Some(b'#'))
                .write_terminator(if lf {LineEnding::Lf} else {LineEnding::CrLf});
            let options = if route == 1 || route == 3 { options.columns(NAMES) } else { options };
            let writer = CsvWriter::new(options).unwrap();
            let rows = Rows::new([Row,Row]);
            let rows = if route == 2 || route == 3 {rows.with_columns(NAMES)} else {rows};
            let finite = writer.to_vec(rows).unwrap();
            let stream = StreamRows::new(from_iter([Ok::<_,Infallible>(Row),Ok(Row)]));
            let stream = if route == 2 || route == 3 {stream.with_columns(NAMES)} else {stream};
            let lazy = bytes(writer.upload(stream).unwrap().into_parts().0);
            (finite,lazy)
        }
        pub fn boundary(limit:usize, route:usize) -> Result<Vec<u8>,String> {
            let options = CsvOptions::new(HeaderMode::Present,b',').max_header_bytes(limit);
            let options = if route == 1 || route == 3 {options.columns(NAMES)} else {options};
            let writer = CsvWriter::new(options).unwrap();
            let rows = Rows::new([Row]);
            let rows = if route == 2 || route == 3 {rows.with_columns(NAMES)} else {rows};
            writer.to_vec(rows).map_err(|e| e.to_string())
        }
        pub fn static_encode() -> Vec<u8> {
            use concord_core::Encode;
            bytes($lib::csv::Csv::<$lib::csv::HasHeader>::encode(Rows::new([Row,Row])).unwrap())
        }
    }};
}
implementation!(current, concord_extras);
implementation!(before, concord_extras_before);

#[test]
fn accepted_headers_and_all_rows_match_before_header_fix() {
    let mut configs=0;
    for delimiter in [b',', b';', b'\t', b'|', b':'] { for quote in [b'"',b'\'',b'~'] {
        for style in 0..4 { for double in [true,false] { for lf in [true,false] {
            for header in [true,false] { for route in 0..4 {
                let old=before::encode(delimiter,quote,style,double,lf,header,route);
                let new=current::encode(delimiter,quote,style,double,lf,header,route);
                assert_eq!(old.0,old.1,"old finite/lazy");
                assert_eq!(new.0,new.1,"new finite/lazy");
                assert_eq!(old,new,"delimiter={delimiter} quote={quote} style={style} double={double} lf={lf} header={header} route={route}");
                configs += 1;
            }}
        }}}}
    }
    assert_eq!(current::static_encode(),before::static_encode());
    let mut checks=0;
    for limit in 1..170 { for route in 0..4 {
        assert_eq!(current::boundary(limit,route), before::boundary(limit,route));
        checks+=1;
    }}
    println!("{configs} dialect/header/column-route configurations, finite + lazy, byte-identical to e27a0958^; static Encode equal; {checks} header-limit acceptance/output comparisons equal");
}
