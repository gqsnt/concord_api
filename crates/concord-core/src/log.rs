//! Feature `tracing`: one `concord.call` event per execution, static facts only.

use std::time::Instant;

use reqwest::StatusCode;

use crate::response::Operation;

/// One execution through [`Decoder::execute`](crate::Decoder::execute), from entry to the
/// response head or the transport failure. Dropped unfinished (cancelled), it emits nothing.
pub(crate) struct Call {
    operation: Option<Operation>,
    /// `None` when no subscriber accepts INFO at all: the clock is then never read.
    started: Option<Instant>,
}

impl Call {
    /// Reads the clock only if some subscriber accepts INFO (`level_enabled!`, a condition every
    /// emitted event must meet). Interest in the event itself — its name, kind, fields, target —
    /// is decided by the event's own callsite in [`end`](Self::end), never by a separate probe.
    pub(crate) fn start(operation: Option<Operation>) -> Self {
        let started = tracing::level_enabled!(tracing::Level::INFO).then(Instant::now);
        Self { operation, started }
    }

    /// Emits the event: `status` if a head arrived, `transport="failed"` otherwise. Every field
    /// is a declared `&'static str` or a number; no URL, header, body or error text. The macro
    /// evaluates the fields (the second clock read included) only if its callsite is enabled.
    pub(crate) fn end(self, status: Option<StatusCode>) {
        let Some(started) = self.started else {
            return;
        };
        let elapsed_ms = || u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let operation = self.operation.map_or("unnamed", |op| op.name());
        let method = self.operation.and_then(|op| op.method()).unwrap_or("");
        let route = self
            .operation
            .and_then(|op| op.meta())
            .map_or("", |meta| meta.route);
        match status {
            Some(status) => tracing::info!(
                name: "concord.call",
                target: "concord",
                operation,
                method,
                route,
                status = u64::from(status.as_u16()),
                elapsed_ms = elapsed_ms(),
                delivery = "answered",
            ),
            None => tracing::info!(
                name: "concord.call",
                target: "concord",
                operation,
                method,
                route,
                transport = "failed",
                elapsed_ms = elapsed_ms(),
                delivery = "unknown",
            ),
        }
    }
}
