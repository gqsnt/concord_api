//! **Generic transport fixture — not Salesforce evidence.**
//!
//! The Salesforce ingest path uploads a spooled body of known length, because the provider
//! documents a size limit and no chunked uploads (see `sf_b_ingest_round_trip`). This file keeps
//! the *generic* one-shot incremental encoder route under test, with the Salesforce dialect's
//! options: `CsvWriter::upload(StreamRows)` produces a lazy body that preparation does not poll
//! and that the loopback server receives with `Transfer-Encoding: chunked`. The loopback server
//! accepts chunked bodies; Salesforce's acceptance of them is unverified, so nothing here is sent
//! to a Salesforce-shaped endpoint. Command:
//! `cargo test -p concord-example-salesforce-app --test generic_streaming_upload_transport`.

mod support;

use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};

use concord::csv::StreamRows;
use concord_example_salesforce_app::ingest::{CONTACT_INSERT_COLUMNS, ContactInsert};
use concord_example_salesforce_sdk::bulk::{BulkDialect, ColumnDelimiter, LineEnding};
use concord_example_salesforce_sdk::csv::{FieldColumns, FieldText, IngestValue, SalesforceCsv};
use concord_example_testserver::{Reply, TestServer};
use futures_core::Stream;

const DIALECT: BulkDialect = BulkDialect::new(ColumnDelimiter::Pipe, LineEnding::Lf);

/// A one-shot source that counts how often it is polled and can fail after `fail_after` rows.
struct Counted {
    rows: std::vec::IntoIter<ContactInsert>,
    polls: Arc<AtomicUsize>,
    fail_after: Option<usize>,
    yielded: usize,
}

impl Stream for Counted {
    type Item = Result<ContactInsert, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        if self.fail_after == Some(self.yielded) {
            self.fail_after = None;
            return Poll::Ready(Some(Err(std::io::Error::other("source failed"))));
        }
        self.yielded += 1;
        Poll::Ready(self.rows.next().map(Ok))
    }
}

fn rows(n: usize) -> Vec<ContactInsert> {
    (0..n)
        .map(|i| ContactInsert {
            last_name: FieldText::new(format!("Name|{i}")).unwrap(),
            first_name: IngestValue::text("First").unwrap(),
            email: if i % 2 == 0 {
                IngestValue::Null
            } else {
                IngestValue::Keep
            },
            loyalty_tier: IngestValue::Keep,
            description: IngestValue::text(format!("line\n{i}")).unwrap(),
        })
        .collect()
}

#[tokio::test]
async fn generic_one_shot_stream_is_polled_only_by_the_transport_and_sent_chunked() {
    let server = TestServer::start(|_| Reply::new(201)).await;
    let csv = SalesforceCsv::new(DIALECT);
    let columns = FieldColumns::new(CONTACT_INSERT_COLUMNS).unwrap();
    let writer = csv.writer(&columns).unwrap();
    let polls = Arc::new(AtomicUsize::new(0));
    let source = Counted {
        rows: rows(2_000).into_iter(),
        polls: Arc::clone(&polls),
        fail_after: None,
        yielded: 0,
    };
    let upload = writer.upload(StreamRows::new(source)).unwrap();
    assert_eq!(
        polls.load(Ordering::SeqCst),
        0,
        "preparing the body polls nothing"
    );
    let http = concord::reqwest::Client::new();
    let mut request = http
        .put(format!("{}/transport-fixture/upload", server.url()))
        .build()
        .unwrap();
    upload.apply(&mut request);
    assert_eq!(polls.load(Ordering::SeqCst), 0);
    assert_eq!(
        request.headers()["content-type"],
        "text/csv; header=present",
        "the generic codec's media type (the Salesforce path sends plain text/csv)"
    );
    let response = http.execute(request).await.unwrap();
    assert_eq!(response.status().as_u16(), 201);
    assert!(
        polls.load(Ordering::SeqCst) > 2_000,
        "the transport polled the source once through"
    );

    let received = server.single();
    assert!(
        received.chunked,
        "unknown length: chunked transfer on this generic fixture"
    );
    // Finite and streaming routes with equal options produce the same table.
    let spooled = csv.spool(&columns, rows(2_000), 16 * 1024 * 1024).unwrap();
    assert_eq!(received.body.as_slice(), spooled.body().as_ref());
    server.shutdown().await;
}

#[tokio::test]
async fn generic_stream_source_failure_fails_the_transfer_after_it_started() {
    let server = TestServer::start(|_| Reply::new(201)).await;
    let csv = SalesforceCsv::new(DIALECT);
    let columns = FieldColumns::new(CONTACT_INSERT_COLUMNS).unwrap();
    let writer = csv.writer(&columns).unwrap();
    let source = Counted {
        rows: rows(10).into_iter(),
        polls: Arc::new(AtomicUsize::new(0)),
        fail_after: Some(5),
        yielded: 0,
    };
    let upload = writer.upload(StreamRows::new(source)).unwrap();
    let http = concord::reqwest::Client::new();
    let mut request = http
        .put(format!("{}/transport-fixture/upload", server.url()))
        .build()
        .unwrap();
    upload.apply(&mut request);
    let error = http.execute(request).await.unwrap_err();
    // A late failure: no retry, no replay; the server never sees a complete chunked body.
    assert!(error.is_request() || error.is_body(), "{error:?}");
    let complete = server
        .requests()
        .iter()
        .any(|r| r.body_text().matches("Name|").count() == 10);
    assert!(!complete);
    server.shutdown().await;
}
