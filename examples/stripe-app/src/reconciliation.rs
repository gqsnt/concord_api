//! ST-A — Stripe v1 billing reconciliation.
//!
//! [`reconcile`] walks a period's invoices (the parent list) and, for each invoice whose
//! embedded `lines` list is not complete, the invoice's line items (the related list), then
//! optionally the period's balance transactions. It returns a [`ReconciliationReport`]: every
//! invoice with its line items and relationship findings, totals kept separate per currency,
//! the provider request metadata of every page, and one [`Section`] per traversal saying whether
//! it completed, was bounded or failed, with the cursor to resume it.
//!
//! # Traversal policy
//!
//! - Every list is a dependent cursor chain ([`CursorList::pages_from`], `concord_pagination`'s
//!   `Pages`): the next request is built only from the previous page's last id; pages are
//!   fetched one at a time, and child chains run one after another inside the parent walk.
//!   Nothing is fetched in parallel and no cursor is derived from a count.
//! - An invoice's embedded `lines` list is the first page of its line chain: when
//!   `lines.has_more` is true, the chain resumes after the embedded list's last id
//!   ([`CursorList::after_embedded`]); the embedded page is not charged to the line budget.
//! - Budgets ([`Budgets`]) are page counts per chain (`concord_pagination::Budget`, applied as
//!   the traversal's `max_pages`): the invoice chain, each invoice's line chain, the balance
//!   chain. A chain that reaches its budget while Stripe has more is `Bounded` with its next
//!   cursor (the traversal's `PagesError::Bounded`, never a silent end).
//! - A failed page (a rate limit, a protocol violation, a transport failure) ends that chain; the
//!   report keeps the typed error, the pages already collected and the cursor of the failed page.
//!   A failed line chain marks that invoice's lines incomplete and the walk continues with the
//!   next invoice; a failed invoice chain ends the invoice section. Nothing is retried here.
//! - **Resuming.** [`ReconcileInput::invoices`] and [`ReconcileInput::balance`] take the
//!   `Resume` value of an earlier report's section. The SDK refuses a cursor from another
//!   account, filter, page size or direction before sending anything. Invoices collected before
//!   the interruption are not fetched again; the caller combines both reports (records of a call
//!   followed by those of its resumption have no duplicates or gaps if the list did not change in
//!   between: Stripe lists are live, not snapshots).
//!
//! # Totals and findings
//!
//! Totals are per currency ([`InvoiceTotals`], [`BalanceTotals`]); amounts in different
//! currencies are never added. Line amounts are summed per invoice in the invoice's currency.
//! Findings ([`Finding`]) are observations for review: a line naming another invoice, a line in
//! another currency, a line of another subscription than the invoice's, and — only when every line
//! was collected — a line sum different from the invoice's `subtotal` (discount semantics make
//! this a review item, not necessarily an error).

use std::collections::BTreeMap;
use std::num::NonZeroUsize;

use concord_example_stripe_sdk::models::{
    BalanceTransaction, Customer, Invoice, InvoiceLineItem, InvoiceStatus, SubscriptionStatus,
};
use concord_example_stripe_sdk::pages::{Cursor, CursorList, ListRequest, StripePage};
use concord_example_stripe_sdk::send::RateLimitedReason;
use concord_example_stripe_sdk::{
    AccountId, ApiVersion, BalanceTransactionId, Currency, CustomerId, Error, ExpandPath,
    Expandable, InvoiceId, InvoiceLineItemId, Limit, Object, PriceId, ResponseMeta, StripeContext,
    SubscriptionId, Timestamp,
};
use concord_pagination::{Budget, Outcome, PagesError, Resume};

/// A creation-time window: `created[gte]` and `created[lt]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Period {
    /// Inclusive start.
    pub start: Timestamp,
    /// Exclusive end.
    pub end: Timestamp,
}

/// Page budgets, one per chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budgets {
    /// Invoice pages in this call.
    pub invoice_pages: Budget,
    /// Line-item pages per invoice, after its embedded first page.
    pub line_pages_per_invoice: Budget,
    /// Balance-transaction pages in this call.
    pub balance_pages: Budget,
}

impl Budgets {
    /// The same budget for every chain.
    pub fn uniform(pages: NonZeroUsize) -> Self {
        let budget = Budget::new(pages);
        Self {
            invoice_pages: budget,
            line_pages_per_invoice: budget,
            balance_pages: budget,
        }
    }
}

/// What to reconcile.
#[derive(Clone, Debug)]
pub struct ReconcileInput {
    /// Only this customer's invoices (`customer=`).
    pub customer: Option<CustomerId>,
    /// Only invoices and balance transactions created in this window.
    pub period: Option<Period>,
    /// Only invoices with this status.
    pub status: Option<InvoiceStatus>,
    /// Expand each invoice's customer and subscription (`expand[]=data.customer` and
    /// `expand[]=data.parent.subscription_details.subscription`) and each balance transaction's
    /// source (`expand[]=data.source`). Without it, those fields are ids.
    pub expand_related: bool,
    /// Page size of every list.
    pub page_size: Limit,
    /// Page budgets.
    pub budgets: Budgets,
    /// Where the invoice chain starts (`Resume::Start`, or a report's resume point); `None`
    /// skips invoices.
    pub invoices: Option<Resume<Cursor<InvoiceId>>>,
    /// Where the balance chain starts; `None` skips balance transactions.
    pub balance: Option<Resume<Cursor<BalanceTransactionId>>>,
}

impl ReconcileInput {
    /// Both sections from the start, with these budgets and page size, no filter, no expansion.
    pub fn new(page_size: Limit, budgets: Budgets) -> Self {
        Self {
            customer: None,
            period: None,
            status: None,
            expand_related: false,
            page_size,
            budgets,
            invoices: Some(Resume::Start),
            balance: Some(Resume::Start),
        }
    }
}

/// The state of one chain.
#[derive(Debug)]
pub struct Section<I> {
    /// How the chain ended; `None` when it was not requested.
    pub outcome: Option<Outcome>,
    /// Pages fetched successfully in this call (an embedded first page is not counted).
    pub pages: u64,
    /// Where to resume (`Some` when bounded or failed): pass it back in the next input.
    pub resume: Option<Resume<Cursor<I>>>,
    /// The typed error that ended the chain, if one did.
    pub error: Option<Error>,
}

impl<I> Section<I> {
    fn not_requested() -> Self {
        Self {
            outcome: None,
            pages: 0,
            resume: None,
            error: None,
        }
    }

    fn complete_without_pages() -> Self {
        Self {
            outcome: Some(Outcome::Exhausted),
            pages: 0,
            resume: None,
            error: None,
        }
    }

    /// Whether the chain was requested and fetched everything.
    pub fn is_complete(&self) -> bool {
        self.outcome.is_some_and(Outcome::is_success)
    }
}

/// Provider metadata of one response (successful page or failure).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestRecord {
    /// The operation, such as `GET /v1/invoices`.
    pub operation: &'static str,
    /// The HTTP status.
    pub status: u16,
    /// `Request-Id`.
    pub request_id: Option<String>,
    /// `Stripe-Rate-Limited-Reason`.
    pub rate_limited_reason: Option<RateLimitedReason>,
    /// `Stripe-Should-Retry`.
    pub should_retry: Option<bool>,
}

/// The customer of an invoice, in whichever form Stripe returned it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomerRef {
    /// The customer id (both forms).
    pub id: CustomerId,
    /// Email, when the customer was expanded.
    pub email: Option<String>,
    /// Whether the customer was expanded.
    pub expanded: bool,
}

/// The subscription that generated an invoice, in whichever form Stripe returned it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubscriptionRef {
    /// The subscription id (both forms).
    pub id: SubscriptionId,
    /// Status, when the subscription was expanded.
    pub status: Option<SubscriptionStatus>,
    /// Whether it was expanded.
    pub expanded: bool,
}

/// One line item of an invoice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineRecord {
    /// The line id.
    pub id: InvoiceLineItemId,
    /// Amount.
    pub amount: i64,
    /// Currency.
    pub currency: Currency,
    /// The subscription it belongs to, if any.
    pub subscription: Option<SubscriptionId>,
    /// Its price, if any.
    pub price: Option<PriceId>,
}

/// An observation about an invoice and its lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Finding {
    /// The line's `invoice` field names another invoice.
    LineOfAnotherInvoice {
        /// The line.
        line: InvoiceLineItemId,
        /// The invoice it names.
        names: InvoiceId,
    },
    /// The line's currency differs from the invoice's.
    LineCurrencyDiffers {
        /// The line.
        line: InvoiceLineItemId,
    },
    /// The line belongs to another subscription than the invoice's.
    LineOfAnotherSubscription {
        /// The line.
        line: InvoiceLineItemId,
    },
    /// Every line was collected and their sum differs from the invoice's `subtotal`.
    LineSumDiffersFromSubtotal {
        /// The invoice's subtotal.
        subtotal: i64,
        /// The sum of its lines.
        line_sum: i64,
    },
}

/// One reconciled invoice.
#[derive(Debug)]
pub struct InvoiceRecord {
    /// The invoice id.
    pub id: InvoiceId,
    /// Its number, once finalized.
    pub number: Option<String>,
    /// Status.
    pub status: Option<InvoiceStatus>,
    /// Currency.
    pub currency: Currency,
    /// Total.
    pub total: i64,
    /// Subtotal.
    pub subtotal: i64,
    /// Amount paid.
    pub amount_paid: i64,
    /// Amount due.
    pub amount_due: i64,
    /// Its customer.
    pub customer: CustomerRef,
    /// The subscription that generated it, if any.
    pub subscription: Option<SubscriptionRef>,
    /// Its line items collected in this call, in Stripe's order.
    pub lines: Vec<LineRecord>,
    /// The line chain (complete without pages when the embedded list was complete).
    pub lines_section: Section<InvoiceLineItemId>,
    /// Findings.
    pub findings: Vec<Finding>,
}

impl InvoiceRecord {
    /// The sum of the collected line amounts in the invoice's currency.
    pub fn line_sum(&self) -> i64 {
        self.lines
            .iter()
            .filter(|l| l.currency == self.currency)
            .map(|l| l.amount)
            .sum()
    }
}

/// Invoice totals of one currency.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InvoiceTotals {
    /// Invoices.
    pub invoices: u64,
    /// Sum of `total`.
    pub total: i64,
    /// Sum of `amount_paid`.
    pub amount_paid: i64,
    /// Sum of `amount_due`.
    pub amount_due: i64,
    /// Sum of collected line amounts.
    pub line_amount: i64,
    /// Invoices whose lines are incomplete (their line amounts are partial).
    pub incomplete_lines: u64,
}

/// Balance-transaction totals of one currency.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BalanceTotals {
    /// Transactions.
    pub transactions: u64,
    /// Sum of `amount`.
    pub amount: i64,
    /// Sum of `fee`.
    pub fee: i64,
    /// Sum of `net`.
    pub net: i64,
}

/// The ST-A result.
#[derive(Debug)]
pub struct ReconciliationReport {
    /// The API version sent.
    pub version: ApiVersion,
    /// The context's connected account (redacted in `Debug`).
    pub account: Option<AccountId>,
    /// Invoices collected in this call.
    pub invoices: Vec<InvoiceRecord>,
    /// Invoice totals per currency.
    pub invoice_totals: BTreeMap<Currency, InvoiceTotals>,
    /// The invoice chain.
    pub invoice_section: Section<InvoiceId>,
    /// Balance totals per currency.
    pub balance_totals: BTreeMap<Currency, BalanceTotals>,
    /// Balance transaction ids collected in this call, in order.
    pub balance_transactions: Vec<BalanceTransactionId>,
    /// Balance sources seen in expanded form (`object` name per transaction), when expanded.
    pub balance_sources: Vec<(BalanceTransactionId, String)>,
    /// The balance chain.
    pub balance_section: Section<BalanceTransactionId>,
    /// Provider metadata of every response, in order.
    pub requests: Vec<RequestRecord>,
}

impl ReconciliationReport {
    /// Whether every requested section, and every invoice's lines, is complete.
    pub fn is_complete(&self) -> bool {
        let invoices = self.invoice_section.outcome.is_none() || self.invoice_section.is_complete();
        let balance = self.balance_section.outcome.is_none() || self.balance_section.is_complete();
        invoices && balance && self.invoices.iter().all(|i| i.lines_section.is_complete())
    }
}

/// Runs ST-A. See the [module documentation](self).
///
/// Every outcome — including failures — is in the report (no `Err`): a reconciliation keeps what
/// it collected.
pub async fn reconcile(stripe: &StripeContext, input: &ReconcileInput) -> ReconciliationReport {
    let mut report = ReconciliationReport {
        version: stripe.version().clone(),
        account: stripe.account().cloned(),
        invoices: Vec::new(),
        invoice_totals: BTreeMap::new(),
        invoice_section: Section::not_requested(),
        balance_totals: BTreeMap::new(),
        balance_transactions: Vec::new(),
        balance_sources: Vec::new(),
        balance_section: Section::not_requested(),
        requests: Vec::new(),
    };
    if let Some(from) = &input.invoices {
        report.invoice_section = walk_invoices(stripe, input, from.clone(), &mut report).await;
    }
    if let Some(from) = &input.balance {
        report.balance_section = walk_balance(stripe, input, from.clone(), &mut report).await;
    }
    for invoice in &report.invoices {
        let totals = report
            .invoice_totals
            .entry(invoice.currency.clone())
            .or_default();
        totals.invoices += 1;
        totals.total += invoice.total;
        totals.amount_paid += invoice.amount_paid;
        totals.amount_due += invoice.amount_due;
        totals.line_amount += invoice.line_sum();
        if !invoice.lines_section.is_complete() {
            totals.incomplete_lines += 1;
        }
    }
    report
}

fn expansions(input: &ReconcileInput, paths: &[&str]) -> Vec<ExpandPath> {
    if !input.expand_related {
        return Vec::new();
    }
    paths
        .iter()
        .map(|p| ExpandPath::new(*p).expect("static expansion paths are valid"))
        .collect()
}

fn record(report: &mut ReconciliationReport, operation: &'static str, meta: &ResponseMeta) {
    report.requests.push(RequestRecord {
        operation,
        status: meta.status,
        request_id: meta.request_id.clone(),
        rate_limited_reason: meta.rate_limited_reason.clone(),
        should_retry: meta.should_retry,
    });
}

/// Drains one chain: records every page's metadata, hands each page to `each`, and turns how
/// the traversal ended into a [`Section`].
async fn drain<R, F>(
    list: &CursorList<R>,
    from: Resume<Cursor<<R::Item as Object>::Id>>,
    budget: Budget,
    operation: &'static str,
    report: &mut ReconciliationReport,
    mut each: F,
) -> Section<<R::Item as Object>::Id>
where
    R: ListRequest + Send + 'static,
    R::Item: Send + 'static,
    <R::Item as Object>::Id: Send + 'static,
    F: AsyncFnMut(StripePage<R::Item>, &mut ReconciliationReport),
{
    let mut pages = list.pages_from(from).max_pages(budget);
    let (outcome, resume, error) = loop {
        match pages.next_page().await {
            Ok(Some(page)) => {
                record(report, operation, &page.meta);
                each(page, report).await;
            }
            Ok(None) => break (Outcome::Exhausted, None, None),
            Err(PagesError::Provider { error, failed, .. }) => {
                if let Some(meta) = error.meta() {
                    record(report, operation, meta);
                }
                break (Outcome::Failed, Some(failed), Some(error));
            }
            // The budget, reached while Stripe had more: the next cursor is the resume point.
            Err(stopped) => break (stopped.outcome(), stopped.resume().cloned(), None),
        }
    };
    Section {
        outcome: Some(outcome),
        pages: pages.progress().succeeded,
        resume,
        error,
    }
}

async fn walk_invoices(
    stripe: &StripeContext,
    input: &ReconcileInput,
    from: Resume<Cursor<InvoiceId>>,
    report: &mut ReconciliationReport,
) -> Section<InvoiceId> {
    let mut template = stripe
        .api()
        .invoices()
        .list()
        .set_limit(input.page_size)
        .maybe_customer(input.customer.clone())
        .maybe_status(input.status)
        .set_expand(expansions(
            input,
            &[
                "data.customer",
                "data.parent.subscription_details.subscription",
            ],
        ));
    if let Some(period) = input.period {
        template = template
            .set_created_gte(period.start)
            .set_created_lt(period.end);
    }
    let list = match CursorList::forward(template) {
        Ok(list) => list,
        Err(error) => return failed_before_start(error),
    };
    drain(
        &list,
        from,
        input.budgets.invoice_pages,
        "GET /v1/invoices",
        report,
        async |page, report| {
            for invoice in page.items {
                let record = reconcile_invoice(stripe, input, invoice, report).await;
                report.invoices.push(record);
            }
        },
    )
    .await
}

fn failed_before_start<I>(error: Error) -> Section<I> {
    Section {
        outcome: Some(Outcome::Failed),
        pages: 0,
        resume: None,
        error: Some(error),
    }
}

async fn reconcile_invoice(
    stripe: &StripeContext,
    input: &ReconcileInput,
    invoice: Invoice,
    report: &mut ReconciliationReport,
) -> InvoiceRecord {
    let subscription = invoice.subscription().map(|s| SubscriptionRef {
        id: s.id().clone(),
        status: s.object().map(|s| s.status),
        expanded: s.is_expanded(),
    });
    let customer = customer_ref(&invoice.customer);
    let mut lines: Vec<InvoiceLineItem> = Vec::new();
    let template = stripe
        .api()
        .invoices()
        .lines(invoice.id.clone())
        .set_limit(input.page_size);
    let lines_section = match CursorList::forward(template) {
        Err(error) => failed_before_start(error),
        Ok(list) => match list.after_embedded(&invoice.lines) {
            Err(error) => failed_before_start(error),
            Ok(continuation) => {
                lines.extend(invoice.lines.data);
                match continuation {
                    None => Section::complete_without_pages(),
                    Some(cursor) => {
                        drain(
                            &list,
                            Resume::At(cursor),
                            input.budgets.line_pages_per_invoice,
                            "GET /v1/invoices/{id}/lines",
                            report,
                            async |page, _report| lines.extend(page.items),
                        )
                        .await
                    }
                }
            }
        },
    };
    let named: Vec<(InvoiceLineItemId, InvoiceId)> = lines
        .iter()
        .filter_map(|line| Some((line.id.clone(), line.invoice.clone()?)))
        .collect();
    let mut record = InvoiceRecord {
        id: invoice.id,
        number: invoice.number,
        status: invoice.status,
        currency: invoice.currency,
        total: invoice.total,
        subtotal: invoice.subtotal,
        amount_paid: invoice.amount_paid,
        amount_due: invoice.amount_due,
        customer,
        subscription,
        lines: lines
            .into_iter()
            .map(|line| LineRecord {
                subscription: line.subscription().cloned(),
                price: line.price().cloned(),
                id: line.id,
                amount: line.amount,
                currency: line.currency,
            })
            .collect(),
        lines_section,
        findings: Vec::new(),
    };
    record.findings = findings(&record, &named);
    record
}

fn customer_ref(customer: &Expandable<Customer>) -> CustomerRef {
    CustomerRef {
        id: customer.id().clone(),
        email: customer.object().and_then(|c| c.email.clone()),
        expanded: customer.is_expanded(),
    }
}

fn findings(record: &InvoiceRecord, named: &[(InvoiceLineItemId, InvoiceId)]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (line, names) in named {
        if *names != record.id {
            findings.push(Finding::LineOfAnotherInvoice {
                line: line.clone(),
                names: names.clone(),
            });
        }
    }
    for line in &record.lines {
        if line.currency != record.currency {
            findings.push(Finding::LineCurrencyDiffers {
                line: line.id.clone(),
            });
        }
        if let (Some(invoice), Some(own)) = (&record.subscription, &line.subscription)
            && invoice.id != *own
        {
            findings.push(Finding::LineOfAnotherSubscription {
                line: line.id.clone(),
            });
        }
    }
    if record.lines_section.is_complete() && record.line_sum() != record.subtotal {
        findings.push(Finding::LineSumDiffersFromSubtotal {
            subtotal: record.subtotal,
            line_sum: record.line_sum(),
        });
    }
    findings
}

async fn walk_balance(
    stripe: &StripeContext,
    input: &ReconcileInput,
    from: Resume<Cursor<BalanceTransactionId>>,
    report: &mut ReconciliationReport,
) -> Section<BalanceTransactionId> {
    let mut template = stripe
        .api()
        .balance_transactions()
        .list()
        .set_limit(input.page_size)
        .set_expand(expansions(input, &["data.source"]));
    if let Some(period) = input.period {
        template = template
            .set_created_gte(period.start)
            .set_created_lt(period.end);
    }
    let list = match CursorList::forward(template) {
        Ok(list) => list,
        Err(error) => return failed_before_start(error),
    };
    drain(
        &list,
        from,
        input.budgets.balance_pages,
        "GET /v1/balance_transactions",
        report,
        async |page, report| {
            for transaction in page.items {
                add_balance(report, transaction);
            }
        },
    )
    .await
}

fn add_balance(report: &mut ReconciliationReport, transaction: BalanceTransaction) {
    let totals = report
        .balance_totals
        .entry(transaction.currency.clone())
        .or_default();
    totals.transactions += 1;
    totals.amount += transaction.amount;
    totals.fee += transaction.fee;
    totals.net += transaction.net;
    if let Some(source) = transaction.source.as_ref().and_then(Expandable::object) {
        report
            .balance_sources
            .push((transaction.id.clone(), source.object.clone()));
    }
    report.balance_transactions.push(transaction.id);
}
