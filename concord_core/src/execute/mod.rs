//! Private deterministic proof that generated and prepared endpoints share one typed executor.

use std::fmt;

use crate::__private::RequestHead;

const STATUS_BODY_LIMIT: usize = 8;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct OperationId(u64);

#[derive(Debug)]
struct OperationIdAllocator {
    next: u64,
}

impl OperationIdAllocator {
    fn new(next: u64) -> Self {
        Self { next }
    }

    fn allocate(&mut self) -> Result<OperationId, ExecutionError> {
        let allocated = OperationId(self.next);
        self.next = self.next.checked_add(1).ok_or(ExecutionError {
            operation_id: None,
            kind: ExecutionErrorKind::ArithmeticOverflow,
        })?;
        Ok(allocated)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Deadline {
    start: u64,
    expires_at: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BodyMetadata {
    Empty,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AuthenticationBoundary {
    NoCredentials,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LimiterMetadata {
    rule: &'static str,
    cost: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct HookConfiguration {
    before_send: bool,
    response_head: bool,
    completion: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResponseMode {
    TypedBuffered,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResponseHandlerMetadata {
    WidgetCount,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct OperationMetadata {
    endpoint: &'static str,
    method: &'static str,
    logical_url: String,
    query: Vec<(String, String)>,
    headers: Vec<(String, String)>,
    scope_snapshot: Vec<(&'static str, String)>,
    operation_id: OperationId,
    deadline: Deadline,
    body: BodyMetadata,
    authentication: AuthenticationBoundary,
    limiter: LimiterMetadata,
    hooks: HookConfiguration,
    response_mode: ResponseMode,
    response_handler: ResponseHandlerMetadata,
}

impl OperationMetadata {
    fn from_generated(head: &RequestHead, operation_id: OperationId, deadline: Deadline) -> Self {
        Self {
            endpoint: head.endpoint(),
            method: head.method(),
            logical_url: head.logical_url().to_owned(),
            query: head.query().to_vec(),
            headers: head.headers().to_vec(),
            scope_snapshot: head.scope_snapshot().to_vec(),
            operation_id,
            deadline,
            body: BodyMetadata::Empty,
            authentication: AuthenticationBoundary::NoCredentials,
            limiter: proof_limiter(),
            hooks: proof_hooks(),
            response_mode: ResponseMode::TypedBuffered,
            response_handler: ResponseHandlerMetadata::WidgetCount,
        }
    }
}

fn proof_limiter() -> LimiterMetadata {
    LimiterMetadata {
        rule: "executor-proof",
        cost: 1,
    }
}

fn proof_hooks() -> HookConfiguration {
    HookConfiguration {
        before_send: true,
        response_head: true,
        completion: true,
    }
}

trait TypedResponseHandler {
    type Output;

    fn handle_success(&self, response: &SuccessfulResponse) -> Result<Self::Output, HandlerError>;
}

struct Operation<H> {
    metadata: OperationMetadata,
    handler: H,
}

impl<H> Operation<H> {
    fn from_generated(
        head: &RequestHead,
        operation_id: OperationId,
        deadline: Deadline,
        handler: H,
    ) -> Self {
        Self {
            metadata: OperationMetadata::from_generated(head, operation_id, deadline),
            handler,
        }
    }
}

struct PreparedEndpoint<H> {
    endpoint: &'static str,
    method: &'static str,
    logical_url: String,
    query: Vec<(String, String)>,
    headers: Vec<(String, String)>,
    scope_snapshot: Vec<(&'static str, String)>,
    operation_id: OperationId,
    deadline: Deadline,
    body: BodyMetadata,
    authentication: AuthenticationBoundary,
    limiter: LimiterMetadata,
    hooks: HookConfiguration,
    response_mode: ResponseMode,
    response_handler: ResponseHandlerMetadata,
    handler: H,
}

impl<H> PreparedEndpoint<H> {
    fn into_operation(self) -> Operation<H> {
        Operation {
            metadata: OperationMetadata {
                endpoint: self.endpoint,
                method: self.method,
                logical_url: self.logical_url,
                query: self.query,
                headers: self.headers,
                scope_snapshot: self.scope_snapshot,
                operation_id: self.operation_id,
                deadline: self.deadline,
                body: self.body,
                authentication: self.authentication,
                limiter: self.limiter,
                hooks: self.hooks,
                response_mode: self.response_mode,
                response_handler: self.response_handler,
            },
            handler: self.handler,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SuccessfulResponse {
    status: u16,
    widget_count: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct HandlerError;

#[derive(Clone, Debug, Eq, PartialEq)]
enum ScriptedBody {
    Complete(Vec<u8>),
    KnownTooLarge,
    UnknownChunks(Vec<Vec<u8>>),
    ReadFailure,
    Deadline,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ScriptedResult {
    Success(SuccessfulResponse),
    Status { status: u16, body: ScriptedBody },
    TransportFailure,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ScriptedExchange {
    result: ScriptedResult,
    advance_before_transport: u64,
    advance_after_response_head: u64,
    advance_after_handler: u64,
    final_auth_valid: bool,
}

impl ScriptedExchange {
    fn success(widget_count: u32) -> Self {
        Self {
            result: ScriptedResult::Success(SuccessfulResponse {
                status: 200,
                widget_count,
            }),
            advance_before_transport: 0,
            advance_after_response_head: 0,
            advance_after_handler: 0,
            final_auth_valid: true,
        }
    }

    fn status(status: u16, body: ScriptedBody) -> Self {
        Self {
            result: ScriptedResult::Status { status, body },
            advance_before_transport: 0,
            advance_after_response_head: 0,
            advance_after_handler: 0,
            final_auth_valid: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CompletionCategory {
    Success,
    Status,
    OperationDeadline,
    Handler,
    Transport,
    AuthenticationFinalization,
    ArithmeticOverflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TracePhase {
    OperationStart,
    BeforeSendHook,
    CredentialPreflight,
    LimiterReservation,
    FinalCredentialValidation,
    TransportEntry,
    ResponseHeadHook,
    TypedResponseHandling,
    StatusHandling,
    CompletionHook,
    OperationCompletion,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TraceEvent {
    operation_id: OperationId,
    phase: TracePhase,
    method: &'static str,
    logical_url: String,
    status: Option<u16>,
    completion: Option<CompletionCategory>,
    body_capture: Option<StatusBodyState>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StatusBodyState {
    Complete,
    OmittedKnownTooLarge,
    OmittedUnknownTooLarge,
    OmittedReadFailure,
    OmittedDeadline,
}

#[derive(Clone, Eq, PartialEq)]
struct StatusBodyCapture {
    state: StatusBodyState,
    complete_body: Option<Vec<u8>>,
}

impl fmt::Debug for StatusBodyCapture {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StatusBodyCapture")
            .field("state", &self.state)
            .field(
                "complete_length",
                &self.complete_body.as_ref().map(Vec::len),
            )
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ExecutionErrorKind {
    Status {
        status: u16,
        body: StatusBodyCapture,
    },
    OperationDeadline {
        status: Option<u16>,
    },
    Handler,
    Transport,
    AuthenticationFinalization,
    ArithmeticOverflow,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ExecutionError {
    operation_id: Option<OperationId>,
    kind: ExecutionErrorKind,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ExecutionCounters {
    shared_entries: usize,
    handler_calls: usize,
    limiter_reservations: usize,
    final_auth_boundaries: usize,
    transport_entries: usize,
    completion_events: usize,
    body_factory_calls: usize,
    body_bytes_constructed: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ExecutionContext {
    now: u64,
    trace: Vec<TraceEvent>,
    counters: ExecutionCounters,
}

impl ExecutionContext {
    fn new(now: u64) -> Self {
        Self {
            now,
            trace: Vec::new(),
            counters: ExecutionCounters::default(),
        }
    }

    fn record(
        &mut self,
        metadata: &OperationMetadata,
        phase: TracePhase,
        status: Option<u16>,
        completion: Option<CompletionCategory>,
        body_capture: Option<StatusBodyState>,
    ) {
        self.trace.push(TraceEvent {
            operation_id: metadata.operation_id,
            phase,
            method: metadata.method,
            logical_url: metadata.logical_url.clone(),
            status,
            completion,
            body_capture,
        });
    }

    fn advance(&mut self, amount: u64, operation_id: OperationId) -> Result<(), ExecutionError> {
        self.now = self.now.checked_add(amount).ok_or(ExecutionError {
            operation_id: Some(operation_id),
            kind: ExecutionErrorKind::ArithmeticOverflow,
        })?;
        Ok(())
    }
}

fn deadline_expired(context: &ExecutionContext, deadline: Deadline) -> bool {
    context.now >= deadline.expires_at
}

fn category_for_error(error: &ExecutionErrorKind) -> CompletionCategory {
    match error {
        ExecutionErrorKind::Status { .. } => CompletionCategory::Status,
        ExecutionErrorKind::OperationDeadline { .. } => CompletionCategory::OperationDeadline,
        ExecutionErrorKind::Handler => CompletionCategory::Handler,
        ExecutionErrorKind::Transport => CompletionCategory::Transport,
        ExecutionErrorKind::AuthenticationFinalization => {
            CompletionCategory::AuthenticationFinalization
        }
        ExecutionErrorKind::ArithmeticOverflow => CompletionCategory::ArithmeticOverflow,
    }
}

fn complete(
    context: &mut ExecutionContext,
    metadata: &OperationMetadata,
    category: CompletionCategory,
    status: Option<u16>,
    body_capture: Option<StatusBodyState>,
) {
    context.record(
        metadata,
        TracePhase::CompletionHook,
        status,
        Some(category),
        body_capture,
    );
    context.record(
        metadata,
        TracePhase::OperationCompletion,
        status,
        Some(category),
        body_capture,
    );
    context.counters.completion_events += 1;
}

fn fail<T>(
    context: &mut ExecutionContext,
    metadata: &OperationMetadata,
    kind: ExecutionErrorKind,
    status: Option<u16>,
    body_capture: Option<StatusBodyState>,
) -> Result<T, ExecutionError> {
    complete(
        context,
        metadata,
        category_for_error(&kind),
        status,
        body_capture,
    );
    Err(ExecutionError {
        operation_id: Some(metadata.operation_id),
        kind,
    })
}

fn fail_if_expired<T>(
    context: &mut ExecutionContext,
    metadata: &OperationMetadata,
    status: Option<u16>,
) -> Option<Result<T, ExecutionError>> {
    deadline_expired(context, metadata.deadline).then(|| {
        fail(
            context,
            metadata,
            ExecutionErrorKind::OperationDeadline { status },
            status,
            None,
        )
    })
}

fn capture_status_body(body: ScriptedBody) -> StatusBodyCapture {
    match body {
        ScriptedBody::Complete(bytes) if bytes.len() <= STATUS_BODY_LIMIT => StatusBodyCapture {
            state: StatusBodyState::Complete,
            complete_body: Some(bytes),
        },
        ScriptedBody::Complete(_) => StatusBodyCapture {
            state: StatusBodyState::OmittedUnknownTooLarge,
            complete_body: None,
        },
        ScriptedBody::UnknownChunks(chunks) => {
            let mut complete_body = Vec::new();
            for chunk in chunks {
                let Some(next_length) = complete_body.len().checked_add(chunk.len()) else {
                    return StatusBodyCapture {
                        state: StatusBodyState::OmittedUnknownTooLarge,
                        complete_body: None,
                    };
                };
                if next_length > STATUS_BODY_LIMIT {
                    return StatusBodyCapture {
                        state: StatusBodyState::OmittedUnknownTooLarge,
                        complete_body: None,
                    };
                }
                complete_body.extend_from_slice(&chunk);
            }
            StatusBodyCapture {
                state: StatusBodyState::Complete,
                complete_body: Some(complete_body),
            }
        }
        ScriptedBody::KnownTooLarge => StatusBodyCapture {
            state: StatusBodyState::OmittedKnownTooLarge,
            complete_body: None,
        },
        ScriptedBody::ReadFailure => StatusBodyCapture {
            state: StatusBodyState::OmittedReadFailure,
            complete_body: None,
        },
        ScriptedBody::Deadline => StatusBodyCapture {
            state: StatusBodyState::OmittedDeadline,
            complete_body: None,
        },
    }
}

fn execute_typed<H>(
    operation: Operation<H>,
    exchange: ScriptedExchange,
    context: &mut ExecutionContext,
) -> Result<H::Output, ExecutionError>
where
    H: TypedResponseHandler,
{
    let Operation { metadata, handler } = operation;
    context.counters.shared_entries += 1;
    context.record(&metadata, TracePhase::OperationStart, None, None, None);

    if let Some(result) = fail_if_expired(context, &metadata, None) {
        return result;
    }
    context.record(&metadata, TracePhase::BeforeSendHook, None, None, None);

    if let Some(result) = fail_if_expired(context, &metadata, None) {
        return result;
    }
    context.record(&metadata, TracePhase::CredentialPreflight, None, None, None);

    if let Some(result) = fail_if_expired(context, &metadata, None) {
        return result;
    }
    context.counters.limiter_reservations += 1;
    context.record(&metadata, TracePhase::LimiterReservation, None, None, None);

    if let Some(result) = fail_if_expired(context, &metadata, None) {
        return result;
    }
    context.counters.final_auth_boundaries += 1;
    context.record(
        &metadata,
        TracePhase::FinalCredentialValidation,
        None,
        None,
        None,
    );
    if !exchange.final_auth_valid {
        return fail(
            context,
            &metadata,
            ExecutionErrorKind::AuthenticationFinalization,
            None,
            None,
        );
    }

    if let Err(error) = context.advance(exchange.advance_before_transport, metadata.operation_id) {
        return fail(context, &metadata, error.kind, None, None);
    }
    if let Some(result) = fail_if_expired(context, &metadata, None) {
        return result;
    }
    context.counters.transport_entries += 1;
    context.record(&metadata, TracePhase::TransportEntry, None, None, None);

    let result = match exchange.result {
        ScriptedResult::TransportFailure => {
            return fail(
                context,
                &metadata,
                ExecutionErrorKind::Transport,
                None,
                None,
            );
        }
        result => result,
    };
    let status = match &result {
        ScriptedResult::Success(response) => response.status,
        ScriptedResult::Status { status, .. } => *status,
        ScriptedResult::TransportFailure => unreachable!("handled before response head"),
    };
    context.record(
        &metadata,
        TracePhase::ResponseHeadHook,
        Some(status),
        None,
        None,
    );
    if let Err(error) = context.advance(exchange.advance_after_response_head, metadata.operation_id)
    {
        return fail(context, &metadata, error.kind, Some(status), None);
    }
    if deadline_expired(context, metadata.deadline) {
        if matches!(result, ScriptedResult::Status { .. }) {
            let capture = StatusBodyCapture {
                state: StatusBodyState::OmittedDeadline,
                complete_body: None,
            };
            context.record(
                &metadata,
                TracePhase::StatusHandling,
                Some(status),
                None,
                Some(capture.state),
            );
            return fail(
                context,
                &metadata,
                ExecutionErrorKind::Status {
                    status,
                    body: capture.clone(),
                },
                Some(status),
                Some(capture.state),
            );
        }
        return fail(
            context,
            &metadata,
            ExecutionErrorKind::OperationDeadline {
                status: Some(status),
            },
            Some(status),
            None,
        );
    }

    match result {
        ScriptedResult::Status { status, body } => {
            let capture = capture_status_body(body);
            context.record(
                &metadata,
                TracePhase::StatusHandling,
                Some(status),
                None,
                Some(capture.state),
            );
            fail(
                context,
                &metadata,
                ExecutionErrorKind::Status {
                    status,
                    body: capture.clone(),
                },
                Some(status),
                Some(capture.state),
            )
        }
        ScriptedResult::Success(response) => {
            context.record(
                &metadata,
                TracePhase::TypedResponseHandling,
                Some(response.status),
                None,
                None,
            );
            context.counters.handler_calls += 1;
            let output = match handler.handle_success(&response) {
                Ok(output) => output,
                Err(HandlerError) => {
                    return fail(
                        context,
                        &metadata,
                        ExecutionErrorKind::Handler,
                        Some(response.status),
                        None,
                    );
                }
            };
            if let Err(error) =
                context.advance(exchange.advance_after_handler, metadata.operation_id)
            {
                return fail(context, &metadata, error.kind, Some(response.status), None);
            }
            if let Some(result) = fail_if_expired(context, &metadata, Some(response.status)) {
                return result;
            }
            complete(
                context,
                &metadata,
                CompletionCategory::Success,
                Some(response.status),
                None,
            );
            Ok(output)
        }
        ScriptedResult::TransportFailure => unreachable!("handled before response head"),
    }
}

#[cfg(test)]
mod tests;
