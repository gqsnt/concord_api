//! Bulk query operations: submit (`bulkOperationRunQuery`, a **mutation** that creates a job on
//! the shop), status (`bulkOperation(id:)`, the 2026-01+ replacement of the deprecated
//! `currentBulkOperation`) and cancel (`bulkOperationCancel`, a mutation).

use serde::{Deserialize, Serialize};

use super::open_enum;
use crate::graphql::{Operation, OperationKind, UserError, nullable};
use crate::scalars::{BulkOperationId, DateTime, SignedUrl, UnsignedInt64};

/// The fields every bulk operation document selects.
macro_rules! bulk_fields {
    () => {
        "id status errorCode createdAt completedAt objectCount rootObjectCount fileSize url \
         partialDataUrl type"
    };
}

open_enum! {
    /// `BulkOperationStatus`.
    BulkOperationStatus {
        /// Created, not yet running.
        Created = "CREATED",
        /// Running; `objectCount` grows.
        Running = "RUNNING",
        /// Finished; `url` is the result (`null` when the query produced no results).
        Completed = "COMPLETED",
        /// Cancelation initiated.
        Canceling = "CANCELING",
        /// Canceled.
        Canceled = "CANCELED",
        /// Failed; see `errorCode`; `partialDataUrl` may hold partial results.
        Failed = "FAILED",
        /// The result URL expired.
        Expired = "EXPIRED",
    }
}

impl BulkOperationStatus {
    /// Whether the operation will not change any more (completed, canceled, failed, expired).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Canceled | Self::Failed | Self::Expired
        )
    }
}

open_enum! {
    /// `BulkOperationErrorCode`.
    BulkOperationErrorCode {
        /// Missing access scopes.
        AccessDenied = "ACCESS_DENIED",
        /// Server errors during execution; results partial.
        InternalServerError = "INTERNAL_SERVER_ERROR",
        /// Query timeouts during execution; results partial.
        Timeout = "TIMEOUT",
    }
}

open_enum! {
    /// `BulkOperationType`.
    BulkOperationType {
        /// A bulk query.
        Query = "QUERY",
        /// A bulk mutation.
        Mutation = "MUTATION",
    }
}

/// A bulk operation as selected by this SDK.
///
/// `url` and `partial_data_url` are signed, expiring download URLs ([`SignedUrl`]: never
/// printed).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkOperation {
    /// `ID!`
    pub id: BulkOperationId,
    /// `BulkOperationStatus!`
    pub status: BulkOperationStatus,
    /// `BulkOperationErrorCode` (nullable).
    #[serde(deserialize_with = "nullable")]
    pub error_code: Option<BulkOperationErrorCode>,
    /// `DateTime!`
    pub created_at: DateTime,
    /// `DateTime` (nullable).
    #[serde(deserialize_with = "nullable")]
    pub completed_at: Option<DateTime>,
    /// `UnsignedInt64!`: a running count of every object processed.
    pub object_count: UnsignedInt64,
    /// `UnsignedInt64!`: root objects processed.
    pub root_object_count: UnsignedInt64,
    /// `UnsignedInt64` (nullable): bytes of the file at `url`.
    #[serde(deserialize_with = "nullable")]
    pub file_size: Option<UnsignedInt64>,
    /// `URL` (nullable): the complete result, expiring 7 days after completion; `null` when the
    /// query produced no results.
    #[serde(deserialize_with = "nullable")]
    pub url: Option<SignedUrl>,
    /// `URL` (nullable): partial results of a failed operation, `null` when there are none.
    #[serde(deserialize_with = "nullable")]
    pub partial_data_url: Option<SignedUrl>,
    /// `BulkOperationType!`
    #[serde(rename = "type")]
    pub kind: BulkOperationType,
}

/// `bulkOperationRunQuery(query:)`: **creates a job** on the shop. Keep it behind an explicit
/// opt-in for live shops.
#[derive(Clone, Serialize)]
pub struct RunBulkQuery {
    /// The bulk query document (one connection at the root, at most five connections and two
    /// levels of nesting).
    pub query: String,
}

impl std::fmt::Debug for RunBulkQuery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The document embeds the caller's search filter.
        write!(f, "RunBulkQuery({} bytes)", self.query.len())
    }
}

impl Operation for RunBulkQuery {
    const NAME: &'static str = "BulkRunQuery";
    const DOCUMENT: &'static str = concat!(
        "mutation BulkRunQuery($query: String!) { bulkOperationRunQuery(query: $query) { \
         bulkOperation { ",
        bulk_fields!(),
        " } userErrors { field message code } } }",
    );
    const KIND: OperationKind = OperationKind::Mutation;
    type Data = RunBulkQueryData;
}

/// `data` of [`RunBulkQuery`].
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunBulkQueryData {
    /// `BulkOperationRunQueryPayload` (nullable).
    #[serde(deserialize_with = "nullable")]
    pub bulk_operation_run_query: Option<BulkPayload>,
}

/// A bulk mutation's payload.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkPayload {
    /// `BulkOperation` (nullable): `None` when the mutation was refused.
    #[serde(deserialize_with = "nullable")]
    pub bulk_operation: Option<BulkOperation>,
    /// `userErrors` (non-null list): why it was refused.
    pub user_errors: Vec<UserError>,
}

/// `bulkOperation(id:)`: the status of one operation.
#[derive(Clone, Debug, Serialize)]
pub struct BulkOperationQuery {
    /// The operation.
    pub id: BulkOperationId,
}

impl Operation for BulkOperationQuery {
    const NAME: &'static str = "BulkOperationStatus";
    const DOCUMENT: &'static str = concat!(
        "query BulkOperationStatus($id: ID!) { bulkOperation(id: $id) { ",
        bulk_fields!(),
        " } }",
    );
    const KIND: OperationKind = OperationKind::Query;
    type Data = BulkOperationData;
}

/// `data` of [`BulkOperationQuery`].
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkOperationData {
    /// `BulkOperation` (nullable): `None` for an unknown id.
    #[serde(deserialize_with = "nullable")]
    pub bulk_operation: Option<BulkOperation>,
}

/// `bulkOperationCancel(id:)`: starts canceling a running operation (a mutation).
#[derive(Clone, Debug, Serialize)]
pub struct CancelBulkOperation {
    /// The operation.
    pub id: BulkOperationId,
}

impl Operation for CancelBulkOperation {
    const NAME: &'static str = "BulkOperationCancel";
    const DOCUMENT: &'static str = concat!(
        "mutation BulkOperationCancel($id: ID!) { bulkOperationCancel(id: $id) { \
         bulkOperation { ",
        bulk_fields!(),
        " } userErrors { field message } } }",
    );
    const KIND: OperationKind = OperationKind::Mutation;
    type Data = CancelBulkOperationData;
}

/// `data` of [`CancelBulkOperation`].
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelBulkOperationData {
    /// `BulkOperationCancelPayload` (nullable).
    #[serde(deserialize_with = "nullable")]
    pub bulk_operation_cancel: Option<BulkPayload>,
}
