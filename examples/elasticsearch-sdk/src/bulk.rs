//! The bulk API: an NDJSON request of operations, and the per-item results correlated with them.
//!
//! The bulk reference states that the response's `items` are "the result of each operation in
//! the bulk request, in the order they were submitted", and that `errors` is `true` when "one or
//! more of the operations in the bulk request did not complete successfully". This module turns
//! those two facts into a [`BulkReport`]: item `n` is matched with operation `n`, and the match is
//! checked (count, action, document id) before any outcome is believed.

use std::fmt;
use std::num::NonZeroUsize;
use std::sync::Arc;

use serde::{Deserialize, Serialize, Serializer};
use serde_json::value::RawValue;

use crate::error::{Error, ErrorCause, Invalid, Protocol};
use crate::model::{DocumentId, IndexName, Refresh};

/// The hard cap on operations per bulk request in this SDK.
pub const MAX_OPERATIONS: usize = 10_000;

/// The hard cap on bulk body bytes: Elasticsearch's documented default maximum HTTP request size
/// (100mb).
pub const MAX_BYTES: usize = 100 * 1024 * 1024;

/// Bounds of one [`BulkRequest`], checked as operations are added.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BulkLimits {
    max_operations: NonZeroUsize,
    max_bytes: NonZeroUsize,
}

impl BulkLimits {
    /// At most `max_operations` operations (≤ [`MAX_OPERATIONS`]) and `max_bytes` body bytes
    /// (≤ [`MAX_BYTES`]).
    pub fn new(max_operations: usize, max_bytes: usize) -> Result<Self, Error> {
        match (
            NonZeroUsize::new(max_operations),
            NonZeroUsize::new(max_bytes),
        ) {
            (Some(ops), Some(bytes)) if ops.get() <= MAX_OPERATIONS && bytes.get() <= MAX_BYTES => {
                Ok(Self {
                    max_operations: ops,
                    max_bytes: bytes,
                })
            }
            _ => Err(Error::Invalid(Invalid::BulkLimits)),
        }
    }

    /// The operation limit.
    pub fn max_operations(&self) -> usize {
        self.max_operations.get()
    }

    /// The byte limit.
    pub fn max_bytes(&self) -> usize {
        self.max_bytes.get()
    }
}

impl Default for BulkLimits {
    /// 1,000 operations and 8 MiB.
    fn default() -> Self {
        Self {
            max_operations: NonZeroUsize::new(1_000).expect("non-zero"),
            max_bytes: NonZeroUsize::new(8 * 1024 * 1024).expect("non-zero"),
        }
    }
}

/// A bulk action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, concord::WireDisplay)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    /// Add or replace a document.
    Index,
    /// Add a document only if its id does not exist yet.
    Create,
    /// Apply a partial document to an existing document.
    Update,
    /// Remove a document.
    Delete,
}

/// One NDJSON line of a bulk body: compact JSON text, without its line feed (the `Ndjson`
/// format writes one after every line, the last included). `Debug` shows its length only.
#[derive(Clone)]
pub struct BulkLine(Arc<RawValue>);

impl BulkLine {
    fn new(text: String) -> Result<Self, Error> {
        let raw =
            RawValue::from_string(text).map_err(|_| Error::Invalid(Invalid::DocumentEncoding))?;
        Ok(Self(Arc::from(raw)))
    }

    /// The line's JSON text.
    pub fn as_str(&self) -> &str {
        self.0.get()
    }

    /// The line's length in bytes, without its line feed.
    pub fn byte_len(&self) -> usize {
        self.0.get().len()
    }
}

impl Serialize for BulkLine {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl fmt::Debug for BulkLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BulkLine(<{} bytes>)", self.byte_len())
    }
}

#[derive(Serialize)]
struct Meta<'a> {
    #[serde(rename = "_index")]
    index: &'a IndexName,
    #[serde(rename = "_id")]
    id: &'a DocumentId,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum ActionLine<'a> {
    Index(Meta<'a>),
    Create(Meta<'a>),
    Update(Meta<'a>),
    Delete(Meta<'a>),
}

/// One submitted operation, kept so that its item can be checked and it can be resubmitted.
#[derive(Clone)]
struct Operation {
    action: Action,
    id: DocumentId,
    action_line: BulkLine,
    source: Option<BulkLine>,
}

impl Operation {
    fn new(action: Action, index: &str, id: &str, source: Option<String>) -> Result<Self, Error> {
        let index = IndexName::new(index)?;
        let id = DocumentId::new(id)?;
        let meta = Meta {
            index: &index,
            id: &id,
        };
        let line = match action {
            Action::Index => ActionLine::Index(meta),
            Action::Create => ActionLine::Create(meta),
            Action::Update => ActionLine::Update(meta),
            Action::Delete => ActionLine::Delete(meta),
        };
        let text =
            serde_json::to_string(&line).map_err(|_| Error::Invalid(Invalid::DocumentEncoding))?;
        Ok(Self {
            action,
            action_line: BulkLine::new(text)?,
            source: source.map(BulkLine::new).transpose()?,
            id,
        })
    }

    /// Body bytes of this operation: each line and its line feed.
    fn bytes(&self) -> usize {
        self.action_line.byte_len() + 1 + self.source.as_ref().map_or(0, |s| s.byte_len() + 1)
    }
}

/// Serializes a document or partial document as one compact JSON object.
fn object_json<T: Serialize + ?Sized>(value: &T) -> Result<String, Error> {
    let text =
        serde_json::to_string(value).map_err(|_| Error::Invalid(Invalid::DocumentEncoding))?;
    if !text.starts_with('{') {
        return Err(Error::Invalid(Invalid::DocumentNotObject));
    }
    // Compact serde_json output escapes line breaks; only a raw JSON value can carry one, and
    // it would split the NDJSON line ("make sure that the JSON actions and sources are not
    // pretty printed").
    if text.contains(['\n', '\r']) {
        return Err(Error::Invalid(Invalid::DocumentLineBreak));
    }
    Ok(text)
}

/// A bounded bulk request: operations in submission order, each with an explicit document id.
///
/// Every operation names its index and a caller-chosen `_id`: the id is how an item is checked
/// against its operation, and it makes a deliberate replay detectable (a replayed `create` of a
/// document that exists fails instead of writing twice). Adding an operation that would exceed
/// the [`BulkLimits`] fails and leaves the request unchanged. `Debug` shows counts, never
/// documents.
#[derive(Clone, Default)]
pub struct BulkRequest {
    operations: Vec<Operation>,
    bytes: usize,
    limits: BulkLimits,
    refresh: Option<Refresh>,
}

impl BulkRequest {
    /// An empty request with the default limits.
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty request with these limits.
    pub fn with_limits(limits: BulkLimits) -> Self {
        Self {
            limits,
            ..Self::default()
        }
    }

    /// The `refresh` query parameter to send (none by default).
    pub fn set_refresh(&mut self, refresh: Option<Refresh>) -> &mut Self {
        self.refresh = refresh;
        self
    }

    /// `index`: add or replace document `id` in `index` with `document` (a JSON object).
    pub fn index<T: Serialize + ?Sized>(
        &mut self,
        index: &str,
        id: &str,
        document: &T,
    ) -> Result<&mut Self, Error> {
        let source = object_json(document)?;
        self.push(Operation::new(Action::Index, index, id, Some(source))?)
    }

    /// `create`: add document `id` to `index` only if no document with that id exists.
    pub fn create<T: Serialize + ?Sized>(
        &mut self,
        index: &str,
        id: &str,
        document: &T,
    ) -> Result<&mut Self, Error> {
        let source = object_json(document)?;
        self.push(Operation::new(Action::Create, index, id, Some(source))?)
    }

    /// `update`: apply `partial` (a JSON object) as the `doc` of an existing document.
    pub fn update<T: Serialize + ?Sized>(
        &mut self,
        index: &str,
        id: &str,
        partial: &T,
    ) -> Result<&mut Self, Error> {
        let partial = object_json(partial)?;
        let source = format!("{{\"doc\":{partial}}}");
        self.push(Operation::new(Action::Update, index, id, Some(source))?)
    }

    /// `delete`: remove document `id` from `index` (no source line).
    pub fn delete(&mut self, index: &str, id: &str) -> Result<&mut Self, Error> {
        self.push(Operation::new(Action::Delete, index, id, None)?)
    }

    fn push(&mut self, operation: Operation) -> Result<&mut Self, Error> {
        if self.operations.len() >= self.limits.max_operations() {
            return Err(Error::Invalid(Invalid::TooManyOperations));
        }
        let bytes = self.bytes + operation.bytes();
        if bytes > self.limits.max_bytes() {
            return Err(Error::Invalid(Invalid::TooManyBytes));
        }
        self.bytes = bytes;
        self.operations.push(operation);
        Ok(self)
    }

    /// The number of operations.
    pub fn len(&self) -> usize {
        self.operations.len()
    }

    /// Whether the request has no operation.
    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    /// The body size in bytes, line feeds included.
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// The `refresh` parameter.
    pub fn refresh(&self) -> Option<Refresh> {
        self.refresh
    }

    /// The limits.
    pub fn limits(&self) -> BulkLimits {
        self.limits
    }

    /// The body lines in order (action line, then its source line if any), for the generated
    /// `bulk` endpoint. The lines share the request's text; nothing is copied.
    pub fn lines(&self) -> Vec<BulkLine> {
        let mut lines = Vec::with_capacity(self.operations.len() * 2);
        for operation in &self.operations {
            lines.push(operation.action_line.clone());
            lines.extend(operation.source.clone());
        }
        lines
    }
}

impl fmt::Debug for BulkRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BulkRequest")
            .field("operations", &self.operations.len())
            .field("bytes", &self.bytes)
            .field("limits", &self.limits)
            .field("refresh", &self.refresh)
            .finish()
    }
}

/// The bulk response as Elasticsearch sends it (the generated `bulk` endpoint's output).
#[derive(Debug, Deserialize)]
#[non_exhaustive]
pub struct BulkResponse {
    /// Milliseconds it took to process the request.
    pub took: u64,
    /// `true` if one or more operations did not complete successfully.
    pub errors: bool,
    /// One item per operation, in submission order.
    pub items: Vec<BulkResponseItem>,
}

/// One bulk item: the action it reports, and its fields.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BulkResponseItem {
    /// An `index` item.
    Index(ItemStatus),
    /// A `create` item.
    Create(ItemStatus),
    /// An `update` item.
    Update(ItemStatus),
    /// A `delete` item.
    Delete(ItemStatus),
}

impl BulkResponseItem {
    /// The action the item reports.
    pub fn action(&self) -> Action {
        self.parts().0
    }

    /// The item's fields.
    pub fn status(&self) -> &ItemStatus {
        self.parts().1
    }

    fn parts(&self) -> (Action, &ItemStatus) {
        match self {
            BulkResponseItem::Index(s) => (Action::Index, s),
            BulkResponseItem::Create(s) => (Action::Create, s),
            BulkResponseItem::Update(s) => (Action::Update, s),
            BulkResponseItem::Delete(s) => (Action::Delete, s),
        }
    }

    fn into_parts(self) -> (Action, ItemStatus) {
        match self {
            BulkResponseItem::Index(s) => (Action::Index, s),
            BulkResponseItem::Create(s) => (Action::Create, s),
            BulkResponseItem::Update(s) => (Action::Update, s),
            BulkResponseItem::Delete(s) => (Action::Delete, s),
        }
    }
}

/// The fields of one bulk item.
#[derive(Debug, Deserialize)]
#[non_exhaustive]
pub struct ItemStatus {
    /// The index (for a data stream, the backing index written to).
    #[serde(rename = "_index")]
    pub index: String,
    /// The document id.
    #[serde(rename = "_id", default)]
    pub id: Option<String>,
    /// The HTTP status of this operation.
    pub status: u16,
    /// `created`, `updated`, `deleted`, `noop` or `not_found`.
    #[serde(default)]
    pub result: Option<ItemResult>,
    /// Present only for a failed operation.
    #[serde(default)]
    pub error: Option<ErrorCause>,
    /// The document version (successful operations).
    #[serde(rename = "_version", default)]
    pub version: Option<u64>,
    /// The sequence number (successful operations).
    #[serde(rename = "_seq_no", default)]
    pub seq_no: Option<u64>,
    /// The primary term (successful operations).
    #[serde(rename = "_primary_term", default)]
    pub primary_term: Option<u64>,
}

/// An item's `result`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemResult {
    /// `created`
    Created,
    /// `updated`
    Updated,
    /// `deleted`
    Deleted,
    /// `noop`
    Noop,
    /// `not_found`
    NotFound,
    /// A value this SDK does not know.
    #[serde(other)]
    Unknown,
}

/// What happened to one operation.
#[derive(Debug)]
pub enum ItemOutcome {
    /// Elasticsearch applied the operation: a 2xx item without `error`, or a `delete` answered
    /// `404` with `result: not_found` and no `error` (the bulk reference lists that item among
    /// "multiple successful operations", with `errors: false`).
    Applied {
        /// The item's `result`.
        result: Option<ItemResult>,
        /// `_version`.
        version: Option<u64>,
        /// `_seq_no`.
        seq_no: Option<u64>,
        /// `_primary_term`.
        primary_term: Option<u64>,
    },
    /// The operation did not complete: the item has an `error`, or a non-2xx status that the
    /// rule above does not cover (then `cause` is `None`).
    Failed {
        /// The item's `error`.
        cause: Option<ErrorCause>,
    },
}

/// One operation's checked outcome.
#[derive(Debug)]
pub struct BulkItem {
    position: usize,
    action: Action,
    index: String,
    id: DocumentId,
    status: u16,
    outcome: ItemOutcome,
}

impl BulkItem {
    /// The 0-based position of the operation in its request.
    pub fn position(&self) -> usize {
        self.position
    }

    /// The action.
    pub fn action(&self) -> Action {
        self.action
    }

    /// The index the item names (the backing index for a data stream).
    pub fn index(&self) -> &str {
        &self.index
    }

    /// The document id (checked equal to the submitted one).
    pub fn id(&self) -> &str {
        self.id.as_str()
    }

    /// The item's HTTP status.
    pub fn status(&self) -> u16 {
        self.status
    }

    /// The outcome.
    pub fn outcome(&self) -> &ItemOutcome {
        &self.outcome
    }

    /// Whether the operation was applied.
    pub fn is_applied(&self) -> bool {
        matches!(self.outcome, ItemOutcome::Applied { .. })
    }

    /// Whether the operation failed.
    pub fn is_failed(&self) -> bool {
        !self.is_applied()
    }

    /// A failed item with status `429` (Elasticsearch could not accept the operation at that
    /// time). This SDK's only built-in resubmission rule: [`BulkReport::resubmit_rejected`].
    pub fn is_rejected(&self) -> bool {
        self.is_failed() && self.status == 429
    }

    /// The `result` of an applied item.
    pub fn result(&self) -> Option<ItemResult> {
        match self.outcome {
            ItemOutcome::Applied { result, .. } => result,
            ItemOutcome::Failed { .. } => None,
        }
    }

    /// The `error` of a failed item.
    pub fn error(&self) -> Option<&ErrorCause> {
        match &self.outcome {
            ItemOutcome::Failed { cause } => cause.as_ref(),
            ItemOutcome::Applied { .. } => None,
        }
    }
}

/// The outcome of every operation of one bulk request, in submission order.
///
/// It is the `Ok` of [`crate::Elasticsearch::bulk`] only when every operation was applied; with
/// at least one failure it comes back inside [`Error::PartialBulk`]. It keeps the submitted
/// operations, so that [`resubmit`](Self::resubmit) can build a request of failed operations
/// only: an applied operation can never be selected.
pub struct BulkReport {
    took: u64,
    items: Vec<BulkItem>,
    operations: Vec<Operation>,
    limits: BulkLimits,
    refresh: Option<Refresh>,
}

impl BulkReport {
    /// `took`, in milliseconds.
    pub fn took(&self) -> u64 {
        self.took
    }

    /// Every item, in submission order.
    pub fn items(&self) -> &[BulkItem] {
        &self.items
    }

    /// The applied items.
    pub fn applied(&self) -> impl Iterator<Item = &BulkItem> {
        self.items.iter().filter(|item| item.is_applied())
    }

    /// The failed items.
    pub fn failed(&self) -> impl Iterator<Item = &BulkItem> {
        self.items.iter().filter(|item| item.is_failed())
    }

    /// Whether every operation was applied.
    pub fn is_success(&self) -> bool {
        self.items.iter().all(BulkItem::is_applied)
    }

    /// A new request holding the failed operations that `select` accepts, in their original
    /// order, with the same limits and `refresh`; `None` when it selects none. `select` is only
    /// ever offered failed items.
    pub fn resubmit(&self, mut select: impl FnMut(&BulkItem) -> bool) -> Option<BulkRequest> {
        let mut request = BulkRequest::with_limits(self.limits);
        request.refresh = self.refresh;
        for item in self.items.iter().filter(|item| item.is_failed()) {
            if select(item) {
                let operation = self.operations[item.position].clone();
                // A subset of an accepted request: within the same limits.
                request.bytes += operation.bytes();
                request.operations.push(operation);
            }
        }
        (!request.is_empty()).then_some(request)
    }

    /// [`resubmit`](Self::resubmit) the rejected (`429`) operations only.
    pub fn resubmit_rejected(&self) -> Option<BulkRequest> {
        self.resubmit(BulkItem::is_rejected)
    }
}

impl fmt::Debug for BulkReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BulkReport")
            .field("took", &self.took)
            .field("items", &self.items)
            .finish_non_exhaustive()
    }
}

/// Matches item `n` with operation `n`, checks the match, classifies every item and checks the
/// `errors` flag against the classification.
pub(crate) fn correlate(request: BulkRequest, response: BulkResponse) -> Result<BulkReport, Error> {
    let BulkRequest {
        operations,
        limits,
        refresh,
        ..
    } = request;
    if response.items.len() != operations.len() {
        return Err(Error::Protocol(Protocol::ItemCount {
            submitted: operations.len(),
            returned: response.items.len(),
        }));
    }
    let mut items = Vec::with_capacity(operations.len());
    for (position, (operation, item)) in operations.iter().zip(response.items).enumerate() {
        let (action, status) = item.into_parts();
        if action != operation.action {
            return Err(Error::Protocol(Protocol::ItemAction { position }));
        }
        if status.id.as_deref() != Some(operation.id.as_str()) {
            return Err(Error::Protocol(Protocol::ItemId { position }));
        }
        let code = status.status;
        let outcome = classify(action, status.error, code, status.result, || {
            (status.version, status.seq_no, status.primary_term)
        });
        items.push(BulkItem {
            position,
            action,
            index: status.index,
            id: operation.id.clone(),
            status: code,
            outcome,
        });
    }
    let report = BulkReport {
        took: response.took,
        items,
        operations,
        limits,
        refresh,
    };
    let failed = !report.is_success();
    if response.errors != failed {
        return Err(Error::Protocol(Protocol::ErrorsFlag));
    }
    if failed {
        return Err(Error::PartialBulk(Box::new(report)));
    }
    Ok(report)
}

fn classify(
    action: Action,
    error: Option<ErrorCause>,
    status: u16,
    result: Option<ItemResult>,
    versions: impl FnOnce() -> (Option<u64>, Option<u64>, Option<u64>),
) -> ItemOutcome {
    if let Some(cause) = error {
        return ItemOutcome::Failed { cause: Some(cause) };
    }
    let applied = (200..300).contains(&status)
        || (action == Action::Delete && status == 404 && result == Some(ItemResult::NotFound));
    if !applied {
        return ItemOutcome::Failed { cause: None };
    }
    let (version, seq_no, primary_term) = versions();
    ItemOutcome::Applied {
        result,
        version,
        seq_no,
        primary_term,
    }
}
