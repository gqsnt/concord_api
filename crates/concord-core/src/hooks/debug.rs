use std::fmt;

use super::{
    BeforeSend, CompletionEvent, DebugLevel, PaginationCompletionEvent, ResponseHeadEvent,
};

/// Private body-free presentation dispatcher for built-in debug output.
pub(crate) struct DebugDispatcher {
    level: DebugLevel,
    #[cfg(test)]
    output: Option<std::sync::Arc<std::sync::Mutex<Vec<String>>>>,
}

impl DebugDispatcher {
    pub(crate) const fn new(level: DebugLevel) -> Self {
        Self {
            level,
            #[cfg(test)]
            output: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_output(
        mut self,
        output: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    ) -> Self {
        self.output = Some(output);
        self
    }

    pub(crate) fn request_ready(&self, event: &BeforeSend<'_>) {
        match self.level {
            DebugLevel::None => {}
            DebugLevel::Basic => self.emit(format_args!(
                "request-ready endpoint={} method={} url={} page={} recovery={}",
                event.endpoint,
                event.method,
                event.safe_url,
                event.page_index,
                event.recovery_index
            )),
            DebugLevel::Headers => self.emit(format_args!(
                "request-ready endpoint={} method={} url={} page={} recovery={} headers={:?}",
                event.endpoint,
                event.method,
                event.safe_url,
                event.page_index,
                event.recovery_index,
                event.headers
            )),
        }
    }

    pub(crate) fn response_head(&self, event: &ResponseHeadEvent<'_>) {
        match self.level {
            DebugLevel::None => {}
            DebugLevel::Basic => self.emit(format_args!(
                "response-head endpoint={} method={} url={} status={} page={} recovery={}",
                event.endpoint,
                event.method,
                event.safe_url,
                event.status,
                event.page_index,
                event.recovery_index
            )),
            DebugLevel::Headers => self.emit(format_args!(
                "response-head endpoint={} method={} url={} status={} page={} recovery={} headers={:?}",
                event.endpoint,
                event.method,
                event.safe_url,
                event.status,
                event.page_index,
                event.recovery_index,
                event.headers
            )),
        }
    }

    pub(crate) fn completion(&self, event: &CompletionEvent<'_>) {
        if self.level != DebugLevel::None {
            self.emit(format_args!(
                "completion endpoint={} method={} url={} page={} status={:?} outcome={:?} elapsed={:?}",
                event.endpoint,
                event.method,
                event.safe_url,
                event.page_index,
                event.final_status,
                event.outcome,
                event.elapsed
            ));
        }
    }

    pub(crate) fn pagination_completion(&self, event: &PaginationCompletionEvent<'_>) {
        if self.level != DebugLevel::None {
            self.emit(format_args!(
                "pagination-completion endpoint={} method={} pages={} items={} status={:?} outcome={:?} elapsed={:?}",
                event.endpoint,
                event.method,
                event.pages_completed,
                event.items_collected,
                event.final_status,
                event.outcome,
                event.elapsed
            ));
        }
    }

    fn emit(&self, message: fmt::Arguments<'_>) {
        #[cfg(test)]
        if let Some(output) = &self.output {
            output
                .lock()
                .expect("private debug capture lock must not be poisoned")
                .push(message.to_string());
            return;
        }

        eprintln!("{message}");
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use http::header::{AUTHORIZATION, HeaderValue};
    use http::{HeaderMap, Method, StatusCode};

    use super::*;
    use crate::hooks::{BodyKind, CompletionOutcome};
    use crate::operation::OperationId;
    use crate::redaction::SanitizedHeaders;

    const SECRET: &str = "debug-secret-sentinel-7d4cb6";

    fn operation_id() -> OperationId {
        OperationId::allocate().expect("test operation ID")
    }

    fn headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_static(SECRET));
        headers.insert("x-visible", HeaderValue::from_static("visible"));
        headers
    }

    #[test]
    fn levels_are_body_free_and_sensitive_headers_are_redacted() {
        let method = Method::GET;
        let headers = headers();
        let id = operation_id();
        let before = BeforeSend {
            operation_id: id,
            parent_operation_id: None,
            endpoint: "DebugApi::health::bytes",
            method: &method,
            safe_url: "https://debug.example.test/health",
            page_index: 0,
            recovery_index: 0,
            headers: SanitizedHeaders::new(&headers),
            body_kind: BodyKind::Empty,
            known_body_length: Some(0),
        };
        let response = ResponseHeadEvent {
            operation_id: id,
            parent_operation_id: None,
            endpoint: before.endpoint,
            method: &method,
            safe_url: before.safe_url,
            page_index: 0,
            recovery_index: 0,
            status: StatusCode::OK,
            headers: SanitizedHeaders::new(&headers),
        };
        let completion = CompletionEvent {
            operation_id: id,
            parent_operation_id: None,
            endpoint: before.endpoint,
            method: &method,
            safe_url: before.safe_url,
            page_index: 0,
            recovered_authentication: false,
            elapsed: Duration::from_millis(2),
            final_status: Some(StatusCode::OK),
            outcome: CompletionOutcome::Success,
        };

        for (level, expected_lines) in [
            (DebugLevel::None, 0),
            (DebugLevel::Basic, 3),
            (DebugLevel::Headers, 3),
        ] {
            let output = Arc::new(Mutex::new(Vec::new()));
            let dispatcher = DebugDispatcher::new(level).with_output(Arc::clone(&output));
            dispatcher.request_ready(&before);
            dispatcher.response_head(&response);
            dispatcher.completion(&completion);
            let output = output.lock().expect("debug output lock");
            assert_eq!(output.len(), expected_lines);
            let joined = output.join("\n");
            assert!(!joined.contains(SECRET));
            assert!(!joined.contains("body-sentinel"));
            if level == DebugLevel::Headers {
                assert!(joined.contains("Redacted"));
                assert!(joined.contains("visible"));
            }
        }
    }
}
