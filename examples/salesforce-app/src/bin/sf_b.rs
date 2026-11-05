//! SF-B against a live **sandbox** org. Creating a Bulk query job needs `SF_SANDBOX_JOBS=1`; the
//! ingest sub-scenario writes Contacts and needs `SF_SANDBOX_MUTATIONS=1`. Without credentials or
//! the opt-in, each part prints `NOT_RUN` and the binary exits 0.

#[path = "live/mod.rs"]
mod live;

use std::num::NonZeroUsize;
use std::time::Duration;

use concord::stream::from_iter;
use concord_example_salesforce_app::ingest::{ContactInsert, IngestInput, ingest_round_trip};
use concord_example_salesforce_app::sf_b::{ExportInput, bulk_csv_export};
use concord_example_salesforce_sdk::bulk::{BulkDialect, ColumnDelimiter, LineEnding};
use concord_example_salesforce_sdk::csv::{FieldText, IngestValue};
use concord_pagination::{Budget, OnError, Order};

fn n(v: usize) -> NonZeroUsize {
    NonZeroUsize::new(v).expect("non-zero")
}

async fn wait(_: u32) {
    tokio::time::sleep(Duration::from_secs(2)).await;
}

fn main() {
    let ctx = match live::context() {
        Ok(ctx) => ctx,
        Err(reason) => {
            println!("NOT_RUN: SF-B live export not configured ({reason})");
            println!("NOT_RUN: SF-B live ingest not configured ({reason})");
            return;
        }
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    let dialect = BulkDialect::new(ColumnDelimiter::Semicolon, LineEnding::Crlf);
    let dir = std::env::var("SF_B_WORK_DIR").unwrap_or_else(|_| "target/sf-b-live".to_owned());
    let mut failed = false;

    if live::opted_in("SF_SANDBOX_JOBS") {
        let input = ExportInput {
            email_suffix: None,
            limit: Some(1000),
            dialect,
            window: n(2),
            job_budget: Budget::new(n(50)),
            discovery_budget: Budget::new(n(20)),
            backlog: n(5),
            order: Order::Ordered,
            on_error: OnError::Stop,
            max_polls: n(60),
            max_part_bytes: Some(512 * 1024 * 1024),
            staging: format!("{dir}/staging").into(),
            output: format!("{dir}/contacts.csv").into(),
        };
        match runtime.block_on(bulk_csv_export(&ctx, &input, wait)) {
            Ok(report) => println!(
                "SF-B live export: job {} {:?}, {} parts completed, {} failed, discovery complete: {}",
                report.job,
                report.status,
                report.completed.len(),
                report.failed.len(),
                report.discovery.complete
            ),
            Err(error) => {
                println!("FAILED: SF-B live export: {error}");
                failed = true;
            }
        }
    } else {
        println!(
            "NOT_RUN: SF-B live export creates a Bulk query job; set SF_SANDBOX_JOBS=1 for a sandbox"
        );
    }

    if live::opted_in("SF_SANDBOX_MUTATIONS") {
        let rows = vec![ContactInsert {
            last_name: FieldText::new("Concord SDK smoke").expect("valid"),
            first_name: IngestValue::Keep,
            email: IngestValue::Keep,
            loyalty_tier: IngestValue::Keep,
            description: IngestValue::text(
                "created by the concord-example-salesforce-app sf_b binary",
            )
            .expect("valid"),
        }];
        let input = IngestInput {
            dialect,
            upload_limit: 1024 * 1024,
            max_polls: n(60),
            max_result_rows: 100,
        };
        let source = from_iter(rows.into_iter().map(Ok::<_, std::io::Error>));
        match runtime.block_on(ingest_round_trip(&ctx, &input, source, wait)) {
            Ok(report) => println!(
                "SF-B live ingest: job {} {:?}, {} uploaded rows ({} bytes buffered), {} successful, {} failed",
                report.job,
                report.state,
                report.upload.rows,
                report.upload.buffered_bytes,
                report.successful.rows.len(),
                report.failed.rows.len()
            ),
            Err(error) => {
                println!("FAILED: SF-B live ingest: {error}");
                failed = true;
            }
        }
    } else {
        println!(
            "NOT_RUN: SF-B live ingest writes Contacts; set SF_SANDBOX_MUTATIONS=1 for a sandbox"
        );
    }
    if failed {
        std::process::exit(1);
    }
}
