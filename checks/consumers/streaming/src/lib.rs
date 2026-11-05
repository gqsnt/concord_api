//! A downstream consumer of the 4.4 additions, built outside the workspace (and, by
//! `scripts/consumers.sh --packaged`, against the packaged `.crate` files): type-level CSV in a
//! declaration and through its incremental reader, JSON Lines, and a `concord-pagination` chain.
//! Only `concord`'s `csv` and `jsonl` features are enabled (no `json`, no reqwest defaults).

use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use concord::csv::{Csv, HasHeader, Semicolon};
use concord::jsonl::JsonLines;

/// One exported row.
#[derive(Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Row {
    /// Identifier.
    pub id: u32,
    /// Name.
    pub name: String,
}

/// The export dialect: header row, semicolons.
pub type Export = Csv<HasHeader, Semicolon>;

concord::api! {
    /// An export service whose formats come from the streaming features.
    pub client Exports() {
        base "https://exports.example.test/v1";
        /// A semicolon CSV report.
        GET report() -> Vec<Row> { path "report"; response Export; }
        /// The same rows as JSON Lines.
        GET lines() -> Vec<Row> { path "lines"; response JsonLines; }
    }
}

struct Noop;

impl Wake for Noop {
    fn wake(self: Arc<Self>) {}
}

/// Polls a future to completion on the current thread (the readers and chains here never wait
/// on I/O; a cooperative `Pending` is simply polled again).
pub fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::from(Arc::new(Noop));
    let mut cx = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
            return value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use concord::jsonl::{JsonLinesOptions, JsonLinesReader};
    use concord::stream::Chunks;
    use concord_pagination::{Budget, Page, Resume, sequential};
    use std::num::NonZeroUsize;

    #[test]
    fn type_level_csv_declares_and_reads_incrementally() {
        let api = Exports::new(concord::reqwest::Client::new());
        let prepared = api.report().prepare().unwrap();
        assert_eq!(prepared.request().headers()["accept"], "text/csv");
        let mut rows = Export::reader()
            .unwrap()
            .read::<Row, _>(Chunks::new([&b"id;na"[..], &b"me\r\n1;one\r\n2;t"[..], &b"wo\r\n"[..]]));
        let mut got = Vec::new();
        while let Some(row) = block_on(rows.next()) {
            got.push(row.unwrap());
        }
        assert_eq!(got, [Row { id: 1, name: "one".into() }, Row { id: 2, name: "two".into() }]);
    }

    #[test]
    fn json_lines_read_across_chunks() {
        let _ = Exports::new(concord::reqwest::Client::new()).lines().prepare().unwrap();
        let reader = JsonLinesReader::new(JsonLinesOptions::json_lines());
        let mut rows = reader.read::<Row, _>(Chunks::new([&b"{\"id\":1,\"na"[..], &b"me\":\"one\"}\n"[..]]));
        assert_eq!(block_on(rows.next()).unwrap().unwrap(), Row { id: 1, name: "one".into() });
        assert!(block_on(rows.next()).is_none());
    }

    #[test]
    fn a_cursor_chain_walks_to_exhaustion() {
        let mut chain = sequential(
            Resume::<u32>::Start,
            Budget::new(NonZeroUsize::new(10).unwrap()),
            |key: Option<u32>| async move {
                let at = key.unwrap_or(0);
                Ok::<_, std::convert::Infallible>(Page::new(vec![at], (at < 2).then_some(at + 1)))
            },
        );
        let mut items = Vec::new();
        while let Some(page) = block_on(chain.next()) {
            items.extend(page.unwrap().items);
        }
        assert_eq!(items, [0, 1, 2]);
    }
}
