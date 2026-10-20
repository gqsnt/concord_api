use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use concord_core::contracts::{ApiError, ErrorCategory, ItemStream, StatusCode, StreamErrorKind};

use super::sse::{ServerEvent, SseState};

type ConnectFuture =
    Pin<Box<dyn Future<Output = Result<ItemStream<ServerEvent>, ApiError>> + Send + 'static>>;
type Connector = Box<dyn FnMut(Option<String>) -> ConnectFuture + Send + 'static>;

/// An explicitly reconnecting consumer of managed server-sent event streams.
///
/// The connector opens a separate managed operation for each connection. It
/// receives the latest nonempty last-event ID, which the application can put
/// in a `Last-Event-ID` request header. End of body and transport failures are
/// suppressed and followed by a reconnect; other failures are returned once,
/// then this source ends. An HTTP 204 response ends the source immediately.
/// Each connection retains the runtime's independent stream limits.
///
/// The default reconnect delay is three seconds, with no reconnect-count
/// limit. A valid SSE `retry` field replaces the delay, including when its
/// block contains no event. IDs and retry values survive reconnection.
///
/// Dropping a pending [`next`](Self::next) future cancels its connector or
/// delay and drops any current connection. A later call can reconnect using
/// the retained protocol state. No background task is spawned.
pub struct EventSource {
    connector: Connector,
    connection: Option<ItemStream<ServerEvent>>,
    last_failure: Option<ApiError>,
    last_event_id: String,
    reconnect_time: Duration,
    retry: Option<Duration>,
    max_reconnects: Option<u32>,
    consecutive_reconnects: u32,
    connections: u32,
    reconnect_pending: bool,
    done: bool,
}

impl EventSource {
    /// Creates a lazy source without opening a connection.
    ///
    /// The connector receives `None` until a nonempty event ID is known, and
    /// again after an empty SSE `id` field clears the retained ID. Its future
    /// must own the request values it needs so that it is `Send + 'static`.
    #[must_use]
    pub fn new<F, Fut>(mut connector: F) -> Self
    where
        F: FnMut(Option<String>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<ItemStream<ServerEvent>, ApiError>> + Send + 'static,
    {
        Self {
            connector: Box::new(move |last_event_id| Box::pin(connector(last_event_id))),
            connection: None,
            last_failure: None,
            last_event_id: String::new(),
            reconnect_time: Duration::from_secs(3),
            retry: None,
            max_reconnects: None,
            consecutive_reconnects: 0,
            connections: 0,
            reconnect_pending: false,
            done: false,
        }
    }

    /// Sets the initial last-event ID supplied to the connector.
    ///
    /// An empty value clears the ID and is supplied as `None`.
    #[must_use]
    pub fn with_last_event_id(mut self, last_event_id: impl Into<String>) -> Self {
        self.last_event_id = last_event_id.into();
        self
    }

    /// Sets the reconnect delay used until a valid SSE `retry` replaces it.
    #[must_use]
    pub fn with_reconnect_time(mut self, reconnect_time: Duration) -> Self {
        self.reconnect_time = reconnect_time;
        self
    }

    /// Bounds consecutive reconnect attempts without receiving an event.
    ///
    /// The initial connection does not count. Every event resets the count;
    /// zero allows the initial connection only. Exhaustion returns the last
    /// connection's transport failure once, if it failed, or ends normally
    /// after EOF. The default is unlimited.
    #[must_use]
    pub fn with_max_reconnects(mut self, max_reconnects: u32) -> Self {
        self.max_reconnects = Some(max_reconnects);
        self
    }

    /// Returns the currently retained last-event ID, or an empty string.
    #[must_use]
    pub fn last_event_id(&self) -> &str {
        &self.last_event_id
    }

    /// Returns the currently selected reconnect delay.
    #[must_use]
    pub const fn reconnect_time(&self) -> Duration {
        self.reconnect_time
    }

    /// Returns the number of connector invocations, saturating at `u32::MAX`.
    ///
    /// Failed or cancelled connection attempts are included.
    #[must_use]
    pub const fn connections(&self) -> u32 {
        self.connections
    }

    /// Returns the next event, one terminal error, or the end of this source.
    ///
    /// EOF and transport failures reconnect after the selected delay. The
    /// returned future is `Send`; dropping it releases its pending work and
    /// current connection synchronously. A subsequent call can reconnect.
    #[allow(
        clippy::should_implement_trait,
        reason = "EventSource exposes an async inherent next like ItemStream"
    )]
    pub async fn next(&mut self) -> Option<Result<ServerEvent, ApiError>> {
        if self.done {
            return None;
        }
        let connection = self.connection.take();
        let mut operation = NextOperation {
            source: self,
            connection,
        };

        loop {
            if operation.connection.is_none() {
                if operation.source.reconnect_pending {
                    if operation
                        .source
                        .max_reconnects
                        .is_some_and(|max| operation.source.consecutive_reconnects >= max)
                    {
                        operation.source.done = true;
                        return operation.source.last_failure.take().map(Err);
                    }
                    tokio::time::sleep(operation.source.reconnect_time).await;
                    operation.source.consecutive_reconnects =
                        operation.source.consecutive_reconnects.saturating_add(1);
                }

                let last_event_id = (!operation.source.last_event_id.is_empty())
                    .then(|| operation.source.last_event_id.clone());
                operation.source.connections = operation.source.connections.saturating_add(1);
                operation.source.last_failure = None;
                // If acquisition is cancelled, the next call is a reconnect
                // and must still obey the selected delay and count bound.
                operation.source.reconnect_pending = true;
                match (operation.source.connector)(last_event_id).await {
                    Ok(stream) => {
                        if stream.summary().status() == StatusCode::NO_CONTENT {
                            operation.source.done = true;
                            return None;
                        }
                        operation.source.seed_state(&stream);
                        operation.connection = Some(stream);
                        operation.source.reconnect_pending = false;
                    }
                    Err(error) => {
                        if error.last_response_status() == Some(StatusCode::NO_CONTENT) {
                            operation.source.done = true;
                            return None;
                        }
                        if is_transport_error(&error) {
                            operation.source.last_failure = Some(error);
                            continue;
                        }
                        operation.source.done = true;
                        return Some(Err(error));
                    }
                }
            }

            let result = operation
                .connection
                .as_mut()
                .expect("the connection was acquired before reading")
                .next()
                .await;
            operation.observe_state();
            match result {
                Some(Ok(event)) => {
                    // This fallback also supports application-owned record
                    // decoders producing ServerEvent without SSE extensions.
                    operation.source.last_event_id.clear();
                    operation
                        .source
                        .last_event_id
                        .push_str(event.last_event_id());
                    if let Some(retry) = event.retry() {
                        operation.source.retry = Some(retry);
                        operation.source.reconnect_time = retry;
                    }
                    operation.source.consecutive_reconnects = 0;
                    operation.source.connection = operation.connection.take();
                    return Some(Ok(event));
                }
                Some(Err(error)) => {
                    operation.close_connection();
                    if is_transport_error(&error) {
                        operation.source.last_failure = Some(error);
                        operation.source.reconnect_pending = true;
                        continue;
                    }
                    operation.source.done = true;
                    return Some(Err(error));
                }
                None => {
                    operation.close_connection();
                    operation.source.reconnect_pending = true;
                }
            }
        }
    }

    fn seed_state(&self, stream: &ItemStream<ServerEvent>) {
        if let Some(state) = stream.extension::<SseState>() {
            let mut state = state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.id.clone_from(&self.last_event_id);
            state.retry = self.retry;
        }
    }

    fn observe_state(&mut self, stream: &ItemStream<ServerEvent>) {
        if let Some(state) = stream.extension::<SseState>() {
            let state = state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            self.last_event_id.clone_from(&state.id);
            if let Some(retry) = state.retry {
                self.retry = Some(retry);
                self.reconnect_time = retry;
            }
        }
    }
}

impl fmt::Debug for EventSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EventSource")
            .field("connections", &self.connections)
            .field("connected", &self.connection.is_some())
            .field("done", &self.done)
            .finish_non_exhaustive()
    }
}

// This guard owns the connection while next is pending. Keeping it in the
// handle across an await would leave the native body open on cancellation.
struct NextOperation<'a> {
    source: &'a mut EventSource,
    connection: Option<ItemStream<ServerEvent>>,
}

impl NextOperation<'_> {
    fn observe_state(&mut self) {
        if let Some(connection) = &self.connection {
            self.source.observe_state(connection);
        }
    }

    fn close_connection(&mut self) {
        self.observe_state();
        self.connection = None;
    }
}

impl Drop for NextOperation<'_> {
    fn drop(&mut self) {
        if self.connection.is_some() {
            self.close_connection();
            self.source.reconnect_pending = true;
        }
    }
}

fn is_transport_error(error: &ApiError) -> bool {
    matches!(
        error.category(),
        ErrorCategory::Connect | ErrorCategory::Transport
    ) || (error.category() == ErrorCategory::ResponseBody
        && error
            .stream_summary()
            .and_then(|summary| summary.error_kind())
            == Some(StreamErrorKind::Body))
}
