//! SH-B — bulk catalog export: submit (or attach to) a bulk query, poll it, and stream its
//! JSON Lines result into a caller-owned bounded sink.
//!
//! # What it does
//!
//! 1. **Start.** [`ExportStart::Submit`] sends `bulkOperationRunQuery` — a mutation that creates
//!    a job on the shop, so it needs a [`JobCreationOptIn`] — or [`ExportStart::Attach`] follows
//!    an operation that already exists (a read).
//! 2. **Poll.** `bulkOperation(id:)` up to [`ExportRequest::max_polls`] times, awaiting the
//!    caller's `wait` future between polls (the library owns no timer). `objectCount` is
//!    reported as progress.
//! 3. **Finish.** `COMPLETED` with a `url` downloads it; `COMPLETED` with `url: null` is an empty
//!    successful export (nothing to download); `FAILED` with `partialDataUrl` downloads the
//!    partial output and says so; `FAILED` without it, `CANCELED` and `EXPIRED` end without data.
//! 4. **Stream.** The signed URL is fetched natively on its own host **without** the access
//!    token, opened with the SDK's `concord::jsonl` reader (status checked before the body is
//!    read), and read record by record: each line becomes one normalized [`ExportRecord`] handed
//!    to the sink at once. A variant carries its product id (`__parentId`), so nothing is grouped
//!    or joined and the workflow holds at most one record plus the reader's bounded line buffer.
//!
//! # Policies
//!
//! - Malformed lines ([`MalformedLines`]): stop at the first one, or skip and count up to a
//!   bound. Only a bad line ended by a line feed can be skipped: the reader reports it as not
//!   terminal (`ReadError::is_terminal`) and goes on with the next line. A bad final line without
//!   a line feed, an over-long line, a transport failure or a byte-order mark is terminal and
//!   always ends the stream, reported as [`ExportOutcome::Interrupted`].
//! - The sink decides to stop ([`SinkControl::Stop`]): the response is dropped at once (its
//!   connection closed, not drained); the remote job is not canceled.
//! - Nothing is retried; a failure keeps the operation id so the caller can attach again.

use std::fmt;
use std::future::Future;
use std::num::NonZeroU32;

use concord::jsonl::RecordErrors;
use concord::reqwest::{self, StatusCode};
use concord::stream::{OpenError, Position, ReadError};
use concord_example_shopify_sdk::bulk_export::{self, BulkRecord};
use concord_example_shopify_sdk::graphql::UserError;
use concord_example_shopify_sdk::ops::bulk::{
    BulkOperation, BulkOperationErrorCode, BulkOperationQuery, BulkOperationStatus,
};
use concord_example_shopify_sdk::ops::catalog::ProductStatus;
use concord_example_shopify_sdk::scalars::{
    BulkOperationId, DateTime, Decimal, InventoryItemId, ProductId, ProductVariantId,
};
use concord_example_shopify_sdk::{
    CatalogExport, Data, GraphqlError, RequestError, ShopContext, SignedUrl,
};

use crate::cost::CostLedger;

/// Permission to create a bulk job (`bulkOperationRunQuery` is a write). Construct it only for
/// an offline fixture or a development shop whose owner opted in: a live binary requires an
/// explicit environment opt-in before it builds one.
#[derive(Clone, Copy, Debug)]
pub struct JobCreationOptIn(());

impl JobCreationOptIn {
    /// The caller confirms the target is a test server or a development shop that may receive
    /// a new bulk job.
    pub fn development_shop_confirmed() -> Self {
        Self(())
    }
}

/// How the export starts.
#[derive(Clone, Debug)]
pub enum ExportStart {
    /// Submit the selection as a new bulk query (a write).
    Submit(CatalogExport, JobCreationOptIn),
    /// Follow an existing operation (a read): poll it and download its result.
    Attach(BulkOperationId),
}

/// What to do with a malformed line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MalformedLines {
    /// The first malformed line ends the export.
    Stop,
    /// Skip malformed lines (each counted), and end the export at the one after `max`.
    Skip {
        /// Lines skipped at most.
        max: u64,
    },
}

/// The inputs of one export.
#[derive(Clone, Debug)]
pub struct ExportRequest {
    /// Submit or attach.
    pub start: ExportStart,
    /// Status polls at most.
    pub max_polls: NonZeroU32,
    /// The longest result line accepted, in bytes.
    pub max_line_bytes: usize,
    /// The malformed-line policy.
    pub malformed: MalformedLines,
}

/// One poll, handed to the caller's `wait` before it.
#[derive(Clone, Debug)]
pub struct PollWait {
    /// The poll about to be made, from 1.
    pub poll: u32,
    /// The status seen last.
    pub status: Option<BulkOperationStatus>,
    /// The `objectCount` seen last.
    pub object_count: Option<u64>,
}

/// The sink's answer to a record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SinkControl {
    /// Send the next record.
    Continue,
    /// Stop the export now (consumer cancellation).
    Stop,
}

/// A caller-owned consumer of normalized records. The workflow hands over each record as it is
/// read and keeps none; the sink owns any storage (and its bound).
pub trait RecordSink {
    /// Takes one record.
    fn accept(&mut self, record: ExportRecord) -> SinkControl;
}

/// A sink that keeps at most `capacity` records in memory and stops the export when full.
#[derive(Debug)]
pub struct BoundedSink {
    capacity: usize,
    /// The records kept, in order.
    pub records: Vec<ExportRecord>,
}

impl BoundedSink {
    /// A sink for at most `capacity` records.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            records: Vec::new(),
        }
    }
}

impl RecordSink for BoundedSink {
    fn accept(&mut self, record: ExportRecord) -> SinkControl {
        self.records.push(record);
        if self.records.len() >= self.capacity {
            SinkControl::Stop
        } else {
            SinkControl::Continue
        }
    }
}

/// One normalized output record: a product, or a variant that names its product.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportRecord {
    /// A product line.
    Product {
        /// The product.
        id: ProductId,
        /// Title.
        title: String,
        /// Handle.
        handle: String,
        /// Status.
        status: ProductStatus,
        /// Vendor.
        vendor: String,
        /// Last update.
        updated_at: DateTime,
    },
    /// A variant line.
    Variant {
        /// The variant.
        id: ProductVariantId,
        /// Its product (`__parentId`).
        product_id: ProductId,
        /// Title.
        title: String,
        /// SKU (nullable).
        sku: Option<String>,
        /// Price.
        price: Decimal,
        /// Sellable quantity (nullable).
        inventory_quantity: Option<i64>,
        /// Inventory item.
        inventory_item_id: InventoryItemId,
        /// Whether the item is tracked.
        tracked: bool,
    },
}

impl From<BulkRecord> for ExportRecord {
    fn from(record: BulkRecord) -> Self {
        match record {
            BulkRecord::Product(p) => Self::Product {
                id: p.id,
                title: p.title,
                handle: p.handle,
                status: p.status,
                vendor: p.vendor,
                updated_at: p.updated_at,
            },
            BulkRecord::Variant(v) => Self::Variant {
                id: v.id,
                product_id: v.product_id,
                title: v.title,
                sku: v.sku,
                price: v.price,
                inventory_quantity: v.inventory_quantity,
                inventory_item_id: v.inventory_item.id,
                tracked: v.inventory_item.tracked,
            },
        }
    }
}

/// How the export ended.
pub enum ExportOutcome {
    /// The complete result was streamed to its end.
    Complete,
    /// `COMPLETED` with `url: null`: the query produced no results; nothing was downloaded.
    EmptyCompleted,
    /// `FAILED` with `partialDataUrl`: the partial output was streamed to its end. Not a complete
    /// export.
    Partial {
        /// Why the job failed.
        error_code: Option<BulkOperationErrorCode>,
    },
    /// `FAILED` without partial output.
    FailedWithoutData {
        /// Why the job failed.
        error_code: Option<BulkOperationErrorCode>,
    },
    /// `CANCELED`.
    Canceled,
    /// `EXPIRED`: the result URL expired.
    Expired,
    /// The poll budget ran out before a terminal status; attach again later.
    StillRunning,
    /// `bulkOperationRunQuery` returned `userErrors` (another operation in progress, a limit, an
    /// invalid query); no job exists.
    Rejected {
        /// The user errors.
        user_errors: Vec<UserError>,
    },
    /// The operation id is unknown (`bulkOperation` is `null`).
    NotFound,
    /// A submit or status request failed (request error, or GraphQL errors without the
    /// expected data — throttling among them).
    RequestFailed {
        /// Which request.
        stage: RequestStage,
        /// The failure.
        failure: RequestFailure,
    },
    /// The signed download was refused before its body was read: a non-2xx status (an expired
    /// or revoked signature is a `403`). The unread response is kept in the error.
    DownloadRefused {
        /// The reader's open error (status, unread response).
        error: OpenError,
    },
    /// The download request failed before a response (connection refused, timeout). The
    /// underlying reqwest error has its URL removed.
    DownloadFailed {
        /// The transport error, URL removed.
        error: reqwest::Error,
    },
    /// Reading stopped after some records: a transport failure mid-body, a malformed line (with
    /// [`MalformedLines::Stop`] or beyond its bound), an over-long line or a byte-order mark.
    Interrupted {
        /// The read error (kind, position; no content).
        error: ReadError,
    },
    /// The sink asked to stop.
    SinkStopped,
}

impl ExportOutcome {
    /// Whether every record of a complete result (or an empty one) was delivered.
    pub fn is_complete(&self) -> bool {
        matches!(self, Self::Complete | Self::EmptyCompleted)
    }
}

impl fmt::Debug for ExportOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Complete => f.write_str("Complete"),
            Self::EmptyCompleted => f.write_str("EmptyCompleted"),
            Self::Partial { error_code } => f
                .debug_struct("Partial")
                .field("error_code", error_code)
                .finish(),
            Self::FailedWithoutData { error_code } => f
                .debug_struct("FailedWithoutData")
                .field("error_code", error_code)
                .finish(),
            Self::Canceled => f.write_str("Canceled"),
            Self::Expired => f.write_str("Expired"),
            Self::StillRunning => f.write_str("StillRunning"),
            Self::Rejected { user_errors } => f
                .debug_struct("Rejected")
                .field("user_errors", user_errors)
                .finish(),
            Self::NotFound => f.write_str("NotFound"),
            Self::RequestFailed { stage, failure } => f
                .debug_struct("RequestFailed")
                .field("stage", stage)
                .field("failure", failure)
                .finish(),
            Self::DownloadRefused { error } => f
                .debug_struct("DownloadRefused")
                .field("error", error)
                .finish(),
            Self::DownloadFailed { error } => f
                .debug_struct("DownloadFailed")
                .field("timeout", &error.is_timeout())
                .field("connect", &error.is_connect())
                .finish(),
            Self::Interrupted { error } => {
                f.debug_struct("Interrupted").field("error", error).finish()
            }
            Self::SinkStopped => f.write_str("SinkStopped"),
        }
    }
}

/// Which GraphQL request failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestStage {
    /// `bulkOperationRunQuery`.
    Submit,
    /// `bulkOperation(id:)`.
    Status,
}

/// A GraphQL request that gave no usable answer.
pub enum RequestFailure {
    /// No envelope (Concord's error or a version mismatch).
    Request(RequestError),
    /// An envelope without the expected data; its errors say why (`THROTTLED`, `ACCESS_DENIED`).
    NoData {
        /// The response's errors.
        errors: Vec<GraphqlError>,
    },
}

impl RequestFailure {
    /// Whether the answer was `THROTTLED`.
    pub fn is_throttled(&self) -> bool {
        matches!(self, Self::NoData { errors } if errors.iter().any(|e| e.code() == Some(&concord_example_shopify_sdk::ErrorCode::Throttled)))
    }
}

impl fmt::Debug for RequestFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(e) => f.debug_tuple("Request").field(e).finish(),
            Self::NoData { errors } => f.debug_struct("NoData").field("errors", errors).finish(),
        }
    }
}

/// Records delivered to the sink.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Delivered {
    /// Product records.
    pub products: u64,
    /// Variant records.
    pub variants: u64,
}

/// The typed result of [`export_catalog`].
///
/// `Debug` shows statuses, counts, error kinds and positions; never the token, a signed URL or
/// record contents.
#[derive(Debug)]
pub struct ExportReport {
    /// The operation, once one is known (submitted or attached).
    pub operation: Option<BulkOperationId>,
    /// Whether this run created it.
    pub submitted: bool,
    /// Status polls made.
    pub polls: u32,
    /// The status seen last.
    pub status: Option<BulkOperationStatus>,
    /// The `objectCount` seen last (progress).
    pub object_count: Option<u64>,
    /// The `fileSize` reported for `url`.
    pub file_size: Option<u64>,
    /// Whether output was downloaded from `partialDataUrl` (partial output).
    pub partial_output: bool,
    /// Records the sink received.
    pub delivered: Delivered,
    /// Malformed lines skipped ([`MalformedLines::Skip`]).
    pub skipped_lines: u64,
    /// Where the reader stood at the end (records, bytes), if a download was opened.
    pub position: Option<Position>,
    /// The reader's largest retained line buffer, in bytes (the bound it kept).
    pub peak_retained_bytes: usize,
    /// GraphQL errors that came with usable answers.
    pub accepted_errors: Vec<GraphqlError>,
    /// The cost reports of every decoded response.
    pub cost: CostLedger,
    /// How it ended.
    pub outcome: ExportOutcome,
}

impl ExportReport {
    fn new() -> Self {
        Self {
            operation: None,
            submitted: false,
            polls: 0,
            status: None,
            object_count: None,
            file_size: None,
            partial_output: false,
            delivered: Delivered::default(),
            skipped_lines: 0,
            position: None,
            peak_retained_bytes: 0,
            accepted_errors: Vec::new(),
            cost: CostLedger::default(),
            outcome: ExportOutcome::StillRunning,
        }
    }

    fn observe(&mut self, op: &BulkOperation) {
        self.status = Some(op.status.clone());
        self.object_count = Some(op.object_count.0);
        self.file_size = op.file_size.map(|s| s.0);
    }
}

/// Runs SH-B for `shop`; see the [module documentation](self). `wait` is awaited before every
/// status poll except the first poll of an attached operation (the caller's backoff: a timer,
/// or nothing in tests).
pub async fn export_catalog<S, W, Fut>(
    shop: &ShopContext,
    request: &ExportRequest,
    sink: &mut S,
    mut wait: W,
) -> ExportReport
where
    S: RecordSink + ?Sized,
    W: FnMut(PollWait) -> Fut,
    Fut: Future<Output = ()>,
{
    let mut report = ExportReport::new();
    let (id, mut needs_wait) = match &request.start {
        ExportStart::Attach(id) => (id.clone(), false),
        ExportStart::Submit(selection, _opt_in) => match submit(shop, selection, &mut report).await
        {
            Some(op) => {
                report.submitted = true;
                report.observe(&op);
                (op.id, true)
            }
            None => return report,
        },
    };
    report.operation = Some(id.clone());

    let mut terminal = None;
    while report.polls < request.max_polls.get() {
        if needs_wait {
            wait(PollWait {
                poll: report.polls + 1,
                status: report.status.clone(),
                object_count: report.object_count,
            })
            .await;
        }
        needs_wait = true;
        report.polls += 1;
        let answer = shop
            .send(
                shop.admin()
                    .bulk_operation(BulkOperationQuery { id: id.clone() })
                    .prepare(),
            )
            .await;
        let answer = match answer {
            Ok(answer) => answer,
            Err(e) => {
                report.outcome = ExportOutcome::RequestFailed {
                    stage: RequestStage::Status,
                    failure: RequestFailure::Request(e),
                };
                return report;
            }
        };
        report.cost.record(answer.cost());
        let data = match answer.data {
            Data::Present(data) => data,
            Data::Absent | Data::Null => {
                report.outcome = ExportOutcome::RequestFailed {
                    stage: RequestStage::Status,
                    failure: RequestFailure::NoData {
                        errors: answer.errors,
                    },
                };
                return report;
            }
        };
        report.accepted_errors.extend(answer.errors);
        let Some(op) = data.bulk_operation else {
            report.outcome = ExportOutcome::NotFound;
            return report;
        };
        report.observe(&op);
        if op.status.is_terminal() {
            terminal = Some(op);
            break;
        }
    }
    let Some(op) = terminal else {
        report.outcome = ExportOutcome::StillRunning;
        return report;
    };

    report.outcome = match op.status {
        BulkOperationStatus::Completed => match &op.url {
            Some(url) => {
                download(
                    shop,
                    request,
                    url,
                    sink,
                    &mut report,
                    ExportOutcome::Complete,
                )
                .await
            }
            None => ExportOutcome::EmptyCompleted,
        },
        BulkOperationStatus::Failed => match &op.partial_data_url {
            Some(url) => {
                report.partial_output = true;
                let done = ExportOutcome::Partial {
                    error_code: op.error_code.clone(),
                };
                download(shop, request, url, sink, &mut report, done).await
            }
            None => ExportOutcome::FailedWithoutData {
                error_code: op.error_code.clone(),
            },
        },
        BulkOperationStatus::Canceled => ExportOutcome::Canceled,
        BulkOperationStatus::Expired => ExportOutcome::Expired,
        // `is_terminal` admits only the four above.
        _ => ExportOutcome::StillRunning,
    };
    report
}

async fn submit(
    shop: &ShopContext,
    selection: &CatalogExport,
    report: &mut ExportReport,
) -> Option<BulkOperation> {
    let answer = match shop
        .send(shop.admin().run_bulk_query(selection.run()).prepare())
        .await
    {
        Ok(answer) => answer,
        Err(e) => {
            report.outcome = ExportOutcome::RequestFailed {
                stage: RequestStage::Submit,
                failure: RequestFailure::Request(e),
            };
            return None;
        }
    };
    report.cost.record(answer.cost());
    let payload = match answer.data {
        Data::Present(data) => data.bulk_operation_run_query,
        Data::Absent | Data::Null => None,
    };
    let Some(payload) = payload else {
        report.outcome = ExportOutcome::RequestFailed {
            stage: RequestStage::Submit,
            failure: RequestFailure::NoData {
                errors: answer.errors,
            },
        };
        return None;
    };
    report.accepted_errors.extend(answer.errors);
    match payload.bulk_operation {
        Some(op) if payload.user_errors.is_empty() => Some(op),
        _ => {
            report.outcome = ExportOutcome::Rejected {
                user_errors: payload.user_errors,
            };
            None
        }
    }
}

async fn download<S: RecordSink + ?Sized>(
    shop: &ShopContext,
    request: &ExportRequest,
    url: &SignedUrl,
    sink: &mut S,
    report: &mut ExportReport,
    done: ExportOutcome,
) -> ExportOutcome {
    // Native on purpose: the signed URL is not a declared endpoint, is used verbatim and must not
    // carry the shop token. Its transport error is reqwest's, so the URL (the signature) is
    // stripped here.
    let response = match shop.http().execute(shop.result_download(url)).await {
        Ok(response) => response,
        Err(error) => {
            return ExportOutcome::DownloadFailed {
                error: error.without_url(),
            };
        }
    };
    let (record_errors, max_skips) = match request.malformed {
        MalformedLines::Stop => (RecordErrors::Stop, 0),
        MalformedLines::Skip { max } => (RecordErrors::Yield, max),
    };
    let reader = bulk_export::result_reader(request.max_line_bytes, record_errors);
    let mut records = match bulk_export::open_result::<BulkRecord>(&reader, response) {
        Ok(records) => records,
        Err(error) => return ExportOutcome::DownloadRefused { error },
    };
    // A skipped line is an error the reader did not end the read on (`is_terminal() == false`,
    // only in `Yield` mode); a refused byte-order mark, a limit or a transport failure is
    // terminal and interrupts the export. (Before `ReadError::is_terminal` existed, this loop
    // had to treat an end right after a skipped error as an interruption: FRICTION.md F7.)
    let outcome = loop {
        let record = match records.next().await {
            None => break done,
            Some(Ok(record)) => record,
            Some(Err(error)) => {
                if !error.is_terminal() && report.skipped_lines < max_skips {
                    report.skipped_lines += 1;
                    continue;
                }
                break ExportOutcome::Interrupted { error };
            }
        };
        match &record {
            BulkRecord::Product(_) => report.delivered.products += 1,
            BulkRecord::Variant(_) => report.delivered.variants += 1,
        }
        if sink.accept(record.into()) == SinkControl::Stop {
            break ExportOutcome::SinkStopped;
        }
    };
    report.position = Some(records.position());
    report.peak_retained_bytes = records.peak_retained_bytes();
    // Dropping the records drops the response: an unfinished body's connection is closed.
    drop(records);
    outcome
}

/// A `StatusCode` helper for reports: whether a refused download looks like an expired or
/// revoked signature (`403`) rather than a missing object (`404`) or a server error.
pub fn signature_refused(error: &OpenError) -> bool {
    error.status_code() == StatusCode::FORBIDDEN
}
