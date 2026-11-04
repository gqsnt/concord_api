//! ST-A end to end against the loopback server and the fixture-backed fake in
//! `support/reconciliation_fake.rs`: forward dependent cursors over a parent list (invoices)
//! and a related list (line items continuing each invoice's embedded `lines`), expandable fields
//! in id and object form with repeated `expand[]`, totals per currency, rate-limit metadata,
//! interrupted and resumed collection, an empty terminal page, a malformed non-terminal page,
//! refused cursors (other direction, other account) and account isolation.
//!
//! Expected values are computed by hand from the fixtures (amounts listed in
//! `fixtures/MANIFEST.json`), not from the client.
//!
//! Run: `cargo test -p concord-example-stripe-app --test st_a_billing_reconciliation`.

#[path = "support/reconciliation_fake.rs"]
mod reconciliation_fake;

use std::collections::BTreeMap;
use std::num::NonZeroUsize;

use concord_example_stripe_app::reconciliation::{
    BalanceTotals, Budgets, Finding, InvoiceTotals, Period, ReconcileInput, ReconciliationReport,
    reconcile,
};
use concord_example_stripe_sdk::models::SubscriptionStatus;
use concord_example_stripe_sdk::pages::{Cursor, Direction};
use concord_example_stripe_sdk::send::RateLimitedReason;
use concord_example_stripe_sdk::{
    AccountId, Currency, InvoiceId, InvoiceLineItemId, Limit, ProtocolError, SecretKey,
    StripeContext, Timestamp,
};
use concord_example_testserver::{Recorded, TestServer};
use concord_pagination::{Budget, Outcome, Resume};
use reconciliation_fake::{BillingFake, Faults};

const KEY: &str = "sk_test_SYNTHETIC_sta_secret";
const JANUARY: Period = Period {
    start: Timestamp(1_767_225_600),
    end: Timestamp(1_769_904_000),
};

async fn start(faults: Faults) -> TestServer {
    let fake = BillingFake::new(KEY);
    fake.set_faults(faults);
    TestServer::start(move |request| fake.respond(request)).await
}

fn context(server: &TestServer, account: &str) -> StripeContext {
    StripeContext::new(
        concord::reqwest::Client::new(),
        &SecretKey::new(KEY).unwrap(),
    )
    .with_base_url(format!("{}/v1", server.url()))
    .unwrap()
    .with_account(Some(AccountId::new(account).unwrap()))
}

fn input(expand: bool) -> ReconcileInput {
    let mut input = ReconcileInput::new(
        Limit::new(2).unwrap(),
        Budgets::uniform(NonZeroUsize::new(5).unwrap()),
    );
    input.period = Some(JANUARY);
    input.expand_related = expand;
    input
}

fn currency(code: &str) -> Currency {
    Currency::new(code).unwrap()
}

fn invoice_ids(report: &ReconciliationReport) -> Vec<&str> {
    report.invoices.iter().map(|i| i.id.as_str()).collect()
}

fn pairs(request: &Recorded) -> Vec<(String, String)> {
    request.query_pairs()
}

fn owned(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

#[tokio::test]
async fn st_a_reconciles_parent_and_related_lists_with_expansions_and_totals() {
    let server = start(Faults::default()).await;
    let alpha = context(&server, "acct_ALPHA");
    let report = reconcile(&alpha, &input(true)).await;
    assert!(report.is_complete(), "{report:?}");

    // Wire: two invoice pages (forward cursor from the last id), one line continuation (from the
    // embedded list's last id), two balance pages. Serial, never `ending_before`.
    let requests = server.requests();
    let targets: Vec<(&str, Vec<String>)> = requests
        .iter()
        .map(|r| (r.path(), r.query_all("starting_after")))
        .collect();
    assert_eq!(
        targets,
        vec![
            ("/v1/invoices", vec![]),
            ("/v1/invoices/in_A3/lines", vec!["il_A3b".to_owned()]),
            ("/v1/invoices", vec!["in_A2".to_owned()]),
            ("/v1/balance_transactions", vec![]),
            ("/v1/balance_transactions", vec!["txn_A3".to_owned()]),
        ]
    );
    assert_eq!(
        requests[0].target,
        "/v1/invoices?created%5Bgte%5D=1767225600&created%5Blt%5D=1769904000&limit=2\
         &expand%5B%5D=data.customer&expand%5B%5D=data.parent.subscription_details.subscription"
    );
    assert_eq!(
        pairs(&requests[2]),
        owned(&[
            ("created[gte]", "1767225600"),
            ("created[lt]", "1769904000"),
            ("limit", "2"),
            ("starting_after", "in_A2"),
            ("expand[]", "data.customer"),
            ("expand[]", "data.parent.subscription_details.subscription"),
        ])
    );
    assert_eq!(
        requests[1].target,
        "/v1/invoices/in_A3/lines?limit=2&starting_after=il_A3b"
    );
    assert_eq!(
        pairs(&requests[3]),
        owned(&[
            ("created[gte]", "1767225600"),
            ("created[lt]", "1769904000"),
            ("limit", "2"),
            ("expand[]", "data.source"),
        ])
    );
    for request in &requests {
        assert_eq!(request.header("stripe-account"), Some("acct_ALPHA"));
        assert!(request.query_all("ending_before").is_empty());
    }

    // Parent/related relationships and both expandable fields in object form.
    assert_eq!(invoice_ids(&report), ["in_A3", "in_A2", "in_A1"]);
    let in_a3 = &report.invoices[0];
    let lines: Vec<&str> = in_a3.lines.iter().map(|l| l.id.as_str()).collect();
    assert_eq!(lines, ["il_A3a", "il_A3b", "il_A3c", "il_A3d"]);
    assert_eq!(in_a3.line_sum(), 3600);
    assert_eq!(in_a3.lines_section.outcome, Some(Outcome::Exhausted));
    assert_eq!(in_a3.lines_section.pages, 1);
    assert!(in_a3.customer.expanded);
    assert_eq!(in_a3.customer.email.as_deref(), Some("a1@example.test"));
    let subscription = in_a3.subscription.as_ref().unwrap();
    assert_eq!(subscription.id.as_str(), "sub_A1");
    assert_eq!(subscription.status, Some(SubscriptionStatus::Active));
    assert_eq!(
        in_a3.lines[3].subscription, None,
        "il_A3d is an invoice item without subscription"
    );
    let in_a2 = &report.invoices[1];
    assert!(in_a2.subscription.is_none());
    assert_eq!(in_a2.lines_section.pages, 0, "embedded lines were complete");
    assert!(report.invoices.iter().all(|i| i.findings.is_empty()));

    // Totals per currency, never mixed.
    assert_eq!(
        report.invoice_totals,
        BTreeMap::from([
            (
                currency("eur"),
                InvoiceTotals {
                    invoices: 1,
                    total: 1500,
                    amount_paid: 1500,
                    amount_due: 1500,
                    line_amount: 1500,
                    incomplete_lines: 0
                }
            ),
            (
                currency("usd"),
                InvoiceTotals {
                    invoices: 2,
                    total: 4800,
                    amount_paid: 3600,
                    amount_due: 4800,
                    line_amount: 4800,
                    incomplete_lines: 0
                }
            ),
        ])
    );
    assert_eq!(
        report.balance_totals,
        BTreeMap::from([
            (
                currency("eur"),
                BalanceTotals {
                    transactions: 1,
                    amount: 1500,
                    fee: 60,
                    net: 1440
                }
            ),
            (
                currency("usd"),
                BalanceTotals {
                    transactions: 2,
                    amount: 400,
                    fee: 100,
                    net: 300
                }
            ),
        ])
    );
    let sources: Vec<(&str, &str)> = report
        .balance_sources
        .iter()
        .map(|(id, object)| (id.as_str(), object.as_str()))
        .collect();
    assert_eq!(
        sources,
        [
            ("txn_A4", "charge"),
            ("txn_A3", "charge"),
            ("txn_A2", "payout")
        ]
    );

    // Sections and provider request metadata.
    assert_eq!(report.invoice_section.outcome, Some(Outcome::Exhausted));
    assert_eq!(report.invoice_section.pages, 2);
    assert!(report.invoice_section.resume.is_none());
    assert_eq!(report.balance_section.pages, 2);
    let ids: Vec<Option<&str>> = report
        .requests
        .iter()
        .map(|r| r.request_id.as_deref())
        .collect();
    assert_eq!(
        ids,
        [
            Some("req_st_a_001"),
            Some("req_st_a_002"),
            Some("req_st_a_003"),
            Some("req_st_a_004"),
            Some("req_st_a_005")
        ]
    );
    assert_eq!(report.version.as_str(), "2026-10-28.endive");
    assert!(!format!("{report:?}").contains("acct_ALPHA"));
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn st_a_identifier_forms_findings_empty_terminal_and_account_isolation() {
    let server = start(Faults::default()).await;
    let alpha = context(&server, "acct_ALPHA");
    let beta = context(&server, "acct_BETA");

    // No expansion: the same fields in identifier form, no `expand[]` on the wire.
    let mut plain = input(false);
    plain.balance = None;
    let report = reconcile(&alpha, &plain).await;
    assert!(report.is_complete());
    assert!(report.invoices.iter().all(|i| !i.customer.expanded));
    let in_a1 = &report.invoices[2];
    assert_eq!(in_a1.customer.id.as_str(), "cus_A1");
    assert_eq!(in_a1.customer.email, None);
    let subscription = in_a1.subscription.as_ref().unwrap();
    assert!(!subscription.expanded && subscription.status.is_none());
    assert!(report.balance_section.outcome.is_none());

    // Account BETA: its own data, findings, and an empty terminal balance page.
    let report_beta = reconcile(&beta, &input(true)).await;
    assert_eq!(invoice_ids(&report_beta), ["in_B1"]);
    let in_b1 = &report_beta.invoices[0];
    assert_eq!(
        in_b1.findings,
        vec![
            Finding::LineOfAnotherInvoice {
                line: InvoiceLineItemId::new("il_B1a").unwrap(),
                names: InvoiceId::new("in_B9").unwrap(),
            },
            Finding::LineOfAnotherSubscription {
                line: InvoiceLineItemId::new("il_B1a").unwrap(),
            },
            Finding::LineSumDiffersFromSubtotal {
                subtotal: 900,
                line_sum: 1000
            },
        ]
    );
    assert_eq!(
        in_b1.subscription.as_ref().unwrap().status,
        Some(SubscriptionStatus::PastDue)
    );
    assert_eq!(
        report_beta.balance_section.outcome,
        Some(Outcome::Exhausted)
    );
    assert_eq!(report_beta.balance_section.pages, 1);
    assert!(report_beta.balance_transactions.is_empty());
    assert!(report_beta.balance_totals.is_empty());

    let requests = server.requests();
    let (alpha_requests, beta_requests): (Vec<&Recorded>, Vec<&Recorded>) = requests
        .iter()
        .partition(|r| r.header("stripe-account") == Some("acct_ALPHA"));
    assert_eq!(alpha_requests.len(), 3);
    assert!(
        alpha_requests
            .iter()
            .all(|r| r.query_all("expand[]").is_empty())
    );
    assert_eq!(beta_requests.len(), 2);
    assert!(
        beta_requests
            .iter()
            .all(|r| r.header("stripe-account") == Some("acct_BETA"))
    );
    server.shutdown().await;
}

#[tokio::test]
async fn st_a_rate_limit_interrupts_and_resumed_collection_completes() {
    let server = start(Faults {
        rate_limit_second_invoice_page: true,
        ..Faults::default()
    })
    .await;
    let alpha = context(&server, "acct_ALPHA");
    let mut first_input = input(true);
    first_input.balance = None;
    let first = reconcile(&alpha, &first_input).await;

    assert!(!first.is_complete());
    assert_eq!(invoice_ids(&first), ["in_A3", "in_A2"]);
    let section = &first.invoice_section;
    assert_eq!(section.outcome, Some(Outcome::Failed));
    assert_eq!(section.pages, 1);
    let error = section.error.as_ref().unwrap();
    assert!(error.is_rate_limited());
    let limited = first.requests.last().unwrap();
    assert_eq!(limited.status, 429);
    assert_eq!(
        limited.rate_limited_reason,
        Some(RateLimitedReason::GlobalRate)
    );
    assert_eq!(limited.should_retry, Some(true));
    assert_eq!(limited.request_id.as_deref(), Some("req_st_a_003"));
    let Some(Resume::At(cursor)) = section.resume.clone() else {
        panic!("a failed chain keeps its restart point: {section:?}");
    };
    assert_eq!(cursor.direction(), Direction::Forward);
    assert_eq!(cursor.id().as_str(), "in_A2");
    assert!(!format!("{section:?}").contains("in_A2"));

    // Resume from the failed page: only the rest is fetched.
    let mut resumed_input = first_input.clone();
    resumed_input.invoices = Some(Resume::At(cursor.clone()));
    let resumed = reconcile(&alpha, &resumed_input).await;
    assert!(resumed.is_complete(), "{resumed:?}");
    assert_eq!(invoice_ids(&resumed), ["in_A1"]);
    let mut all = invoice_ids(&first);
    all.extend(invoice_ids(&resumed));
    assert_eq!(all, ["in_A3", "in_A2", "in_A1"]);
    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[3].query_all("starting_after"), ["in_A2"]);

    // The same cursor is refused, before any request, for another account or another filter.
    let beta = context(&server, "acct_BETA");
    let leaked = reconcile(&beta, &resumed_input).await;
    assert_eq!(leaked.invoice_section.outcome, Some(Outcome::Failed));
    assert_eq!(
        leaked.invoice_section.error.as_ref().unwrap().protocol(),
        Some(ProtocolError::CursorBindingMismatch)
    );
    let mut other_filter = resumed_input.clone();
    other_filter.period = None;
    let refused = reconcile(&alpha, &other_filter).await;
    assert_eq!(
        refused.invoice_section.error.as_ref().unwrap().protocol(),
        Some(ProtocolError::CursorBindingMismatch)
    );
    // And a cursor of the other direction.
    let (_, id, binding) = cursor.into_parts();
    let mut backward = resumed_input.clone();
    backward.invoices = Some(Resume::At(Cursor::from_parts(
        Direction::Backward,
        id,
        binding,
    )));
    let refused = reconcile(&alpha, &backward).await;
    assert_eq!(
        refused.invoice_section.error.as_ref().unwrap().protocol(),
        Some(ProtocolError::IncompatibleDirection)
    );
    assert_eq!(server.requests().len(), 4, "refused cursors sent nothing");
    server.shutdown().await;
}

#[tokio::test]
async fn st_a_malformed_line_page_and_bounded_budget() {
    let server = start(Faults {
        malformed_lines: true,
        ..Faults::default()
    })
    .await;
    let alpha = context(&server, "acct_ALPHA");
    let mut malformed = input(false);
    malformed.balance = None;
    let report = reconcile(&alpha, &malformed).await;

    // `has_more: true` with no objects is a typed protocol error for that invoice's lines; the
    // walk continues with the other invoices.
    let in_a3 = &report.invoices[0];
    assert_eq!(in_a3.lines_section.outcome, Some(Outcome::Failed));
    assert_eq!(
        in_a3.lines_section.error.as_ref().unwrap().protocol(),
        Some(ProtocolError::EmptyNonTerminalPage)
    );
    let Some(Resume::At(cursor)) = &in_a3.lines_section.resume else {
        panic!("the failed line page keeps its restart point");
    };
    assert_eq!(cursor.id().as_str(), "il_A3b");
    assert_eq!(in_a3.lines.len(), 2, "the embedded page is kept");
    assert!(
        in_a3.findings.is_empty(),
        "no subtotal judgement on partial lines"
    );
    assert_eq!(invoice_ids(&report), ["in_A3", "in_A2", "in_A1"]);
    assert_eq!(report.invoice_section.outcome, Some(Outcome::Exhausted));
    assert_eq!(report.invoice_totals[&currency("usd")].incomplete_lines, 1);
    assert!(!report.is_complete());

    // A one-page invoice budget: bounded, with the next cursor.
    let mut bounded = input(false);
    bounded.balance = None;
    bounded.budgets.invoice_pages = Budget::new(NonZeroUsize::new(1).unwrap());
    let report = reconcile(&alpha, &bounded).await;
    assert_eq!(report.invoice_section.outcome, Some(Outcome::Bounded));
    assert_eq!(invoice_ids(&report), ["in_A3", "in_A2"]);
    let Some(Resume::At(next)) = &report.invoice_section.resume else {
        panic!("a bounded chain keeps its next cursor");
    };
    assert_eq!(next.id().as_str(), "in_A2");
    assert!(report.invoice_section.error.is_none());
    server.shutdown().await;
}
