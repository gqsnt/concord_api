use std::fmt;

use super::{
    AttemptStarted, DebugLevel, OperationFinished, PaginationFinished, ResponseHeadObserved,
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

    pub(crate) fn request_ready(&self, event: &AttemptStarted<'_>) {
        match self.level {
            DebugLevel::None => {}
            DebugLevel::Basic => self.emit(format_args!(
                "request-ready endpoint={} method={} target={:?} page={} attempt={} policy-retry={} authentication-recovery={}",
                event.endpoint,
                event.method,
                event.target,
                event.page_index,
                event.attempt.index,
                event.attempt.policy_retry_index,
                event.attempt.authentication_recovery
            )),
            DebugLevel::Headers => self.emit(format_args!(
                "request-ready endpoint={} method={} target={:?} page={} attempt={} policy-retry={} authentication-recovery={}",
                event.endpoint,
                event.method,
                event.target,
                event.page_index,
                event.attempt.index,
                event.attempt.policy_retry_index,
                event.attempt.authentication_recovery
            )),
        }
    }

    pub(crate) fn response_head(&self, event: &ResponseHeadObserved<'_>) {
        match self.level {
            DebugLevel::None => {}
            DebugLevel::Basic => self.emit(format_args!(
                "response-head endpoint={} method={} target={:?} status={} page={} attempt={} policy-retry={} authentication-recovery={}",
                event.endpoint,
                event.method,
                event.target,
                event.status,
                event.page_index,
                event.attempt.index,
                event.attempt.policy_retry_index,
                event.attempt.authentication_recovery
            )),
            DebugLevel::Headers => self.emit(format_args!(
                "response-head endpoint={} method={} target={:?} status={} page={} attempt={} policy-retry={} authentication-recovery={} content-length={:?} server-delay={:?}",
                event.endpoint,
                event.method,
                event.target,
                event.status,
                event.page_index,
                event.attempt.index,
                event.attempt.policy_retry_index,
                event.attempt.authentication_recovery,
                event.content_length,
                event.server_delay
            )),
        }
    }

    pub(crate) fn completion(&self, event: &OperationFinished<'_>) {
        if self.level != DebugLevel::None {
            self.emit(format_args!(
                "completion endpoint={} method={} target={:?} page={} attempts={} policy-retries={} recovered-authentication={} status={:?} outcome={:?} elapsed={:?}",
                event.endpoint,
                event.method,
                event.target,
                event.page_index,
                event.attempts_started,
                event.policy_retries,
                event.recovered_authentication,
                event.final_status,
                event.outcome,
                event.elapsed
            ));
        }
    }

    pub(crate) fn pagination_completion(&self, event: &PaginationFinished<'_>) {
        if self.level != DebugLevel::None {
            self.emit(format_args!(
                "pagination-completion endpoint={} method={} pages={} items={} attempts={} policy-retries={} recovered-authentication={} status={:?} outcome={:?} elapsed={:?}",
                event.endpoint,
                event.method,
                event.pages_completed,
                event.items_collected,
                event.attempts_started,
                event.policy_retries,
                event.recovered_authentication,
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

    use http::{Method, StatusCode};

    use super::*;
    use crate::hooks::{AttemptInfo, BodyKind, CompletionOutcome, RequestTarget};
    use crate::operation::OperationId;
    const SECRET: &str = "debug-secret-sentinel-7d4cb6";

    fn operation_id() -> OperationId {
        OperationId::allocate().expect("test operation ID")
    }

    #[test]
    fn levels_are_body_free_and_request_values_are_absent() {
        let method = Method::GET;
        let id = operation_id();
        let target = RequestTarget::from_url(
            &url::Url::parse("https://debug.example.test/health?first=one&second=two")
                .expect("debug target"),
            2,
        );
        let before = AttemptStarted {
            operation_id: id,
            parent_operation_id: None,
            endpoint: "DebugApi::health::bytes",
            method: &method,
            target,
            page_index: 0,
            attempt: AttemptInfo::INITIAL,
            body_kind: BodyKind::Empty,
            known_body_length: Some(0),
        };
        let response = ResponseHeadObserved {
            operation_id: id,
            parent_operation_id: None,
            endpoint: before.endpoint,
            method: &method,
            target,
            page_index: 0,
            attempt: AttemptInfo::INITIAL,
            status: StatusCode::OK,
            content_length: Some(7),
            server_delay: None,
        };
        let completion = OperationFinished {
            operation_id: id,
            parent_operation_id: None,
            endpoint: before.endpoint,
            method: &method,
            target,
            page_index: 0,
            attempts_started: 1,
            policy_retries: 0,
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
            if level != DebugLevel::None {
                assert!(joined.contains("attempt=0"));
                assert!(joined.contains("policy-retr"));
                assert!(joined.contains("recovered-authentication=false"));
            }
            if level == DebugLevel::Headers {
                assert!(joined.contains("query_occurrence_count: 2"));
                assert!(joined.contains("public_header_occurrence_count: 2"));
                assert!(joined.contains("content-length=Some(7)"));
            }
        }
    }
}
