//! SF-A against a live org (read-only: describe and SOQL queries, at most two pages per
//! section). Without `SF_INSTANCE_URL` and `SF_ACCESS_TOKEN` it prints `NOT_RUN` and exits 0.

#[path = "live/mod.rs"]
#[allow(dead_code)]
mod live;

use std::num::NonZeroUsize;

use concord_example_salesforce_app::sf_a::{Completion, ExtractionInput, crm_extraction};
use concord_pagination::Budget;

fn main() {
    let ctx = match live::context() {
        Ok(ctx) => ctx,
        Err(reason) => {
            println!("NOT_RUN: SF-A live read-only smoke not configured ({reason})");
            return;
        }
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    let input = ExtractionInput::new(Budget::new(NonZeroUsize::new(2).expect("non-zero")));
    match runtime.block_on(crm_extraction(&ctx, &input)) {
        Ok(report) => {
            let state = |c: &Completion| match c {
                Completion::Complete => "complete",
                Completion::Bounded { .. } => "bounded",
                Completion::Interrupted { .. } => "interrupted",
                Completion::RestartRequired { .. } => "restart-required",
            };
            println!(
                "SF-A live: {} accounts ({}), {} contacts ({}), missing optional fields: {:?} / {:?}",
                report.accounts.records.len(),
                state(&report.accounts.completion),
                report.contacts.records.len(),
                state(&report.contacts.completion),
                report.account_schema.missing_optional,
                report.contact_schema.missing_optional,
            );
        }
        Err(error) => {
            println!("FAILED: {error}");
            std::process::exit(1);
        }
    }
}
