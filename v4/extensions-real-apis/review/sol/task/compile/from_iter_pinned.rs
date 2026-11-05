use serde::Serialize;
use std::convert::Infallible;
use std::marker::PhantomPinned;
#[derive(Serialize)]
struct Row {
    id: u32,
    #[serde(skip)]
    pin: PhantomPinned,
}
fn main() {
    let rows = concord::stream::from_iter(vec![Ok::<_, Infallible>(Row {
        id: 1,
        pin: PhantomPinned,
    })]);
    let _upload =
        concord::jsonl::JsonLinesWriter::new(concord::jsonl::JsonLinesOptions::json_lines())
            .upload(concord::jsonl::StreamRecords::new(rows));
}
