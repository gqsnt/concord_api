//! ST-A against the live Stripe API: a read-only, budgeted reconciliation.
//!
//! Requires `STRIPE_SECRET_KEY` (a secret or restricted key with read access to invoices and
//! balance transactions). Optional: `STRIPE_ACCOUNT=acct_...` (a connected account),
//! `STRIPE_PERIOD_START` / `STRIPE_PERIOD_END` (Unix seconds, `created[gte]` / `created[lt]`).
//! Without a key it prints `NOT_RUN` and exits 0.
//!
//! The run is bounded: 10 objects per page, at most 2 invoice pages, 1 extra line page per
//! invoice and 2 balance pages, one request at a time. It only sends `GET` requests. It prints
//! aggregates (counts, totals per currency, section outcomes, request ids), never customer data.

use std::num::NonZeroUsize;
use std::process::ExitCode;

use concord_example_stripe_app::reconciliation::{Budgets, Period, ReconcileInput, reconcile};
use concord_example_stripe_sdk::{AccountId, Limit, SecretKey, StripeContext, Timestamp};
use concord_pagination::Budget;

fn main() -> ExitCode {
    let Ok(key) = std::env::var("STRIPE_SECRET_KEY") else {
        println!("NOT_RUN: ST-A live run skipped: STRIPE_SECRET_KEY is not set");
        return ExitCode::SUCCESS;
    };
    let Ok(key) = SecretKey::new(key) else {
        println!("NOT_RUN: ST-A live run skipped: STRIPE_SECRET_KEY is not a Stripe secret key");
        return ExitCode::SUCCESS;
    };
    let account = match std::env::var("STRIPE_ACCOUNT") {
        Ok(account) => match AccountId::new(account) {
            Ok(account) => Some(account),
            Err(error) => {
                eprintln!("FAILED: STRIPE_ACCOUNT: {error}");
                return ExitCode::FAILURE;
            }
        },
        Err(_) => None,
    };
    let seconds = |name: &str| std::env::var(name).ok().and_then(|v| v.parse::<i64>().ok());
    let period = match (seconds("STRIPE_PERIOD_START"), seconds("STRIPE_PERIOD_END")) {
        (Some(start), Some(end)) => Some(Period {
            start: Timestamp(start),
            end: Timestamp(end),
        }),
        _ => None,
    };
    let one = Budget::new(NonZeroUsize::MIN);
    let two = Budget::new(NonZeroUsize::new(2).expect("non-zero"));
    let mut input = ReconcileInput::new(
        Limit::new(10).expect("valid limit"),
        Budgets {
            invoice_pages: two,
            line_pages_per_invoice: one,
            balance_pages: two,
        },
    );
    input.period = period;
    input.expand_related = true;

    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("FAILED: runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    let stripe = StripeContext::new(concord::reqwest::Client::new(), &key).with_account(account);
    let report = runtime.block_on(reconcile(&stripe, &input));

    println!("version: {}", report.version);
    println!("invoices: {}", report.invoices.len());
    for (currency, totals) in &report.invoice_totals {
        println!("invoice totals {currency}: {totals:?}");
    }
    for (currency, totals) in &report.balance_totals {
        println!("balance totals {currency}: {totals:?}");
    }
    let findings: usize = report.invoices.iter().map(|i| i.findings.len()).sum();
    println!("findings: {findings}");
    println!(
        "invoice section: {:?} ({} pages), balance section: {:?} ({} pages)",
        report.invoice_section.outcome,
        report.invoice_section.pages,
        report.balance_section.outcome,
        report.balance_section.pages
    );
    for request in &report.requests {
        println!(
            "{} -> {} {:?} rate_limited={:?}",
            request.operation, request.status, request.request_id, request.rate_limited_reason
        );
    }
    let failed = report.invoice_section.error.is_some() || report.balance_section.error.is_some();
    if failed {
        println!("FAILED: a section ended with an error (see the outcomes above)");
        ExitCode::FAILURE
    } else {
        println!("LIVE_PROVIDER_RUN: ST-A read-only reconciliation finished (bounded)");
        ExitCode::SUCCESS
    }
}
