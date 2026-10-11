//! Write reconciliation (§23.6).
//!
//! The write helper retains the application's durable command identifier. On
//! an error whose aggregate `dispatch_knowledge()` is not `NotDispatched`, it
//! returns an outcome-unknown classification together with that identifier
//! for explicit reconciliation. It never generates a new command ID and never
//! resubmits automatically: even an observed 2xx followed by a body/decoder
//! failure can have committed the write.
//!
//! Concord's own errors are value-free, but that is no guarantee for
//! application wrappers. The types here carry the command identifier and the
//! payload, so they implement `Debug`/`Display` by hand and print neither; a
//! derived formatter would print both (`tests/application.rs` checks this
//! with sentinels against these exact formatters).

// ANCHOR: reconciliation
use std::fmt;

use concord::{ApiError, CallOptions, DispatchKnowledge};

use super::execute::execute;
use crate::contracts::catalog_v3::{Item, UpdateInput, catalog_api};

/// A durable application command identifier (for example persisted before
/// the first attempt). Formatting never prints the value.
#[derive(Clone, Eq, PartialEq)]
pub struct CommandId(String);

impl CommandId {
    /// Wraps an application-generated durable identifier.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Explicitly exposes the identifier for reconciliation or storage.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for CommandId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CommandId(<redacted>)")
    }
}

/// One durable title change: the command identifier and its payload.
pub struct TitleChange {
    /// Target item.
    pub item_id: String,
    /// Durable command identifier sent as `Idempotency-Key`.
    pub command_id: CommandId,
    /// New title (application data; not formatted automatically).
    pub title: String,
}

impl fmt::Debug for TitleChange {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TitleChange")
            .field("item_id", &"<redacted>")
            .field("command_id", &self.command_id)
            .field("title", &"<redacted>")
            .finish()
    }
}

/// Why a write did not return a confirmed item.
pub enum WriteFailure {
    /// Concord settled that the application request was not dispatched. The
    /// same command may be submitted again later, with the same identifier.
    NotDispatched {
        /// The retained durable identifier.
        command_id: CommandId,
        /// The value-free library error.
        error: ApiError,
    },
    /// The request may have reached the provider (possibly dispatched or a
    /// response was observed). Reconcile using the retained identifier; do not
    /// resubmit under a new one.
    OutcomeUnknown {
        /// The retained durable identifier.
        command_id: CommandId,
        /// The value-free library error.
        error: ApiError,
    },
}

impl WriteFailure {
    /// Returns the retained durable command identifier.
    #[must_use]
    pub fn command_id(&self) -> &CommandId {
        match self {
            Self::NotDispatched { command_id, .. } | Self::OutcomeUnknown { command_id, .. } => {
                command_id
            }
        }
    }

    /// Returns the underlying value-free library error.
    #[must_use]
    pub fn error(&self) -> &ApiError {
        match self {
            Self::NotDispatched { error, .. } | Self::OutcomeUnknown { error, .. } => error,
        }
    }

    /// Whether explicit reconciliation is required before any new attempt.
    #[must_use]
    pub fn needs_reconciliation(&self) -> bool {
        matches!(self, Self::OutcomeUnknown { .. })
    }

    fn classification(&self) -> &'static str {
        match self {
            Self::NotDispatched { .. } => "NotDispatched",
            Self::OutcomeUnknown { .. } => "OutcomeUnknown",
        }
    }
}

impl fmt::Debug for WriteFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let error = self.error();
        formatter
            .debug_struct("WriteFailure")
            .field("classification", &self.classification())
            .field("command_id", self.command_id())
            .field("dispatch", &error.dispatch_knowledge())
            .field("stage", &error.failure_stage())
            .field("status", &error.last_response_status())
            .finish()
    }
}

impl fmt::Display for WriteFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let error = self.error();
        write!(
            formatter,
            "title change {} ({:?}, stage {:?}); command identifier retained for reconciliation",
            self.classification(),
            error.dispatch_knowledge(),
            error.failure_stage(),
        )
    }
}

impl std::error::Error for WriteFailure {}

/// Classifies a failed write by the journal's aggregate dispatch knowledge.
#[must_use]
pub fn classify(command_id: CommandId, error: ApiError) -> WriteFailure {
    match error.dispatch_knowledge() {
        DispatchKnowledge::NotDispatched => WriteFailure::NotDispatched { command_id, error },
        _ => WriteFailure::OutcomeUnknown { command_id, error },
    }
}

/// Submits one title change exactly once under the caller's limits.
///
/// # Errors
///
/// A [`WriteFailure`] that keeps the durable command identifier.
pub async fn change_title(
    scope: &catalog_api::regional::Regional,
    change: TitleChange,
    limits: &CallOptions,
) -> Result<Item, WriteFailure> {
    let TitleChange {
        item_id,
        command_id,
        title,
    } = change;
    let request = scope.update_item(item_id, command_id.expose(), UpdateInput { title });
    execute(request, limits)
        .await
        .map_err(|error| classify(command_id, error))
}
// ANCHOR_END: reconciliation
