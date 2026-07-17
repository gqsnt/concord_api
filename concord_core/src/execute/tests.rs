use concord_macros::api;

use super::*;

api! {
    client ExecutorProofApi {
        base "https://executor.example.test/v1";
        scope accounts(
            account: String,
        ) {
            path ["accounts", scope.account, "widgets"];
            GET ById(
                widget: String,
            ) -> NoContent {
                path [ep.widget];
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WidgetCount(u32);

#[derive(Clone, Copy, Debug, Default)]
struct WidgetCountHandler {
    fail: bool,
}

impl TypedResponseHandler for WidgetCountHandler {
    type Output = WidgetCount;

    fn handle_success(&self, response: &SuccessfulResponse) -> Result<Self::Output, HandlerError> {
        if self.fail {
            Err(HandlerError)
        } else {
            Ok(WidgetCount(response.widget_count))
        }
    }
}

fn deadline() -> Deadline {
    Deadline {
        start: 0,
        expires_at: 10,
    }
}

fn generated_endpoint() -> executor_proof_api::endpoints::accounts::ById {
    ExecutorProofApi::new()
        .accounts("account-alpha".to_owned())
        .by_id("widget-omega".to_owned())
}

fn generated_operation(
    operation_id: OperationId,
    handler: WidgetCountHandler,
) -> Operation<WidgetCountHandler> {
    let endpoint = generated_endpoint();
    let base = crate::__private::Url::parse("https://executor.example.test/v1")
        .expect("executor proof base");
    let builder = crate::__private::HeadBuilder::new(
        &base,
        crate::__private::EndpointSpec::request_meta(&endpoint),
        &crate::__private::RequestOptions::new(),
        crate::__private::DebugLevel::None,
    );
    let (head, _) =
        crate::__private::EndpointSpec::prepare_head_and_body_metadata(&endpoint, builder)
            .expect("generated proof head");
    Operation::from_generated(&head, operation_id, deadline(), handler)
}

fn prepared_endpoint(
    operation_id: OperationId,
    handler: WidgetCountHandler,
) -> PreparedEndpoint<WidgetCountHandler> {
    PreparedEndpoint {
        endpoint: "ExecutorProofApi::accounts::ById",
        method: "GET",
        logical_url: "https://executor.example.test/v1/accounts/account-alpha/widgets/widget-omega"
            .to_owned(),
        query: Vec::new(),
        headers: Vec::new(),
        scope_snapshot: Vec::new(),
        operation_id,
        deadline: deadline(),
        body: BodyMetadata::Empty,
        authentication: AuthenticationBoundary::NoCredentials,
        limiter: proof_limiter(),
        hooks: proof_hooks(),
        response_mode: ResponseMode::TypedBuffered,
        response_handler: ResponseHandlerMetadata::WidgetCount,
        handler,
    }
}

fn run_generated(
    operation_id: OperationId,
    exchange: ScriptedExchange,
) -> (
    Result<WidgetCount, ExecutionError>,
    OperationMetadata,
    ExecutionContext,
) {
    let operation = generated_operation(operation_id, WidgetCountHandler::default());
    let metadata = operation.metadata.clone();
    let mut context = ExecutionContext::new(metadata.deadline.start);
    let result = execute_typed(operation, exchange, &mut context);
    (result, metadata, context)
}

fn run_prepared(
    operation_id: OperationId,
    exchange: ScriptedExchange,
) -> (
    Result<WidgetCount, ExecutionError>,
    OperationMetadata,
    ExecutionContext,
) {
    let operation = prepared_endpoint(operation_id, WidgetCountHandler::default()).into_operation();
    let metadata = operation.metadata.clone();
    let mut context = ExecutionContext::new(metadata.deadline.start);
    let result = execute_typed(operation, exchange, &mut context);
    (result, metadata, context)
}

fn run_pair(
    exchange: ScriptedExchange,
) -> (
    Result<WidgetCount, ExecutionError>,
    Result<WidgetCount, ExecutionError>,
    OperationMetadata,
    OperationMetadata,
    ExecutionContext,
    ExecutionContext,
) {
    let operation_id = OperationId(41);
    let (generated_result, generated_metadata, generated_context) =
        run_generated(operation_id, exchange.clone());
    let (prepared_result, prepared_metadata, prepared_context) =
        run_prepared(operation_id, exchange);
    (
        generated_result,
        prepared_result,
        generated_metadata,
        prepared_metadata,
        generated_context,
        prepared_context,
    )
}

fn assert_pair_equal(exchange: ScriptedExchange) {
    let (
        generated_result,
        prepared_result,
        generated_metadata,
        prepared_metadata,
        generated_context,
        prepared_context,
    ) = run_pair(exchange);
    assert_eq!(generated_result, prepared_result);
    assert_eq!(generated_metadata, prepared_metadata);
    assert_eq!(generated_context, prepared_context);
}

fn status_capture(error: &ExecutionError) -> &StatusBodyCapture {
    match &error.kind {
        ExecutionErrorKind::Status { body, .. } => body,
        other => panic!("expected status error, got {other:?}"),
    }
}

mod executor_parity {
    use super::*;

    #[test]
    fn executor_generated_and_prepared_use_one_shared_function() {
        let (_, _, _, _, generated, prepared) = run_pair(ScriptedExchange::success(7));
        assert_eq!(generated.counters.shared_entries, 1);
        assert_eq!(prepared.counters.shared_entries, 1);
    }

    #[test]
    fn executor_generated_and_prepared_have_equal_operation_metadata() {
        let (_, _, generated, prepared, _, _) = run_pair(ScriptedExchange::success(7));
        assert_eq!(generated, prepared);
        assert_eq!(generated.endpoint, "ExecutorProofApi::accounts::ById");
        assert_eq!(generated.method, "GET");
        assert_eq!(generated.query, Vec::<(String, String)>::new());
        assert_eq!(generated.headers, Vec::<(String, String)>::new());
        assert!(generated.scope_snapshot.is_empty());
        assert_eq!(generated.operation_id, OperationId(41));
        assert_eq!(generated.deadline, deadline());
        assert_eq!(generated.body, BodyMetadata::Empty);
        assert_eq!(
            generated.authentication,
            AuthenticationBoundary::NoCredentials
        );
        assert_eq!(generated.limiter, proof_limiter());
        assert_eq!(generated.hooks, proof_hooks());
        assert_eq!(generated.response_mode, ResponseMode::TypedBuffered);
        assert_eq!(
            generated.response_handler,
            ResponseHandlerMetadata::WidgetCount
        );
    }

    #[test]
    fn executor_generated_and_prepared_have_equal_boundary_trace() {
        let (_, _, _, _, generated, prepared) = run_pair(ScriptedExchange::success(7));
        assert_eq!(generated.trace, prepared.trace);
        assert_eq!(
            generated
                .trace
                .iter()
                .map(|event| event.phase)
                .collect::<Vec<_>>(),
            vec![
                TracePhase::OperationStart,
                TracePhase::BeforeSendHook,
                TracePhase::CredentialPreflight,
                TracePhase::LimiterReservation,
                TracePhase::FinalCredentialValidation,
                TracePhase::TransportEntry,
                TracePhase::ResponseHeadHook,
                TracePhase::TypedResponseHandling,
                TracePhase::CompletionHook,
                TracePhase::OperationCompletion,
            ]
        );
    }
}

mod executor_typed {
    use super::*;

    #[test]
    fn executor_generated_and_prepared_return_same_typed_success() {
        let (generated, prepared, _, _, generated_context, prepared_context) =
            run_pair(ScriptedExchange::success(37));
        assert_eq!(generated, Ok(WidgetCount(37)));
        assert_eq!(prepared, Ok(WidgetCount(37)));
        assert_eq!(generated_context.trace, prepared_context.trace);
    }

    #[test]
    fn executor_handler_output_is_compile_time_typed() {
        let (result, _, _) = run_generated(OperationId(5), ScriptedExchange::success(19));
        let value: WidgetCount = result.expect("typed success");
        assert_eq!(value, WidgetCount(19));
    }

    #[test]
    fn executor_success_handler_runs_once() {
        let (_, _, _, _, generated, prepared) = run_pair(ScriptedExchange::success(7));
        assert_eq!(generated.counters.handler_calls, 1);
        assert_eq!(prepared.counters.handler_calls, 1);
    }

    #[test]
    fn executor_handler_and_transport_errors_are_structured() {
        let operation = generated_operation(OperationId(8), WidgetCountHandler { fail: true });
        let mut context = ExecutionContext::new(0);
        let handler_error = execute_typed(operation, ScriptedExchange::success(1), &mut context)
            .expect_err("handler must fail");
        assert_eq!(handler_error.kind, ExecutionErrorKind::Handler);

        let mut exchange = ScriptedExchange::success(1);
        exchange.result = ScriptedResult::TransportFailure;
        let (transport_error, _, _) = run_generated(OperationId(9), exchange);
        assert_eq!(
            transport_error.expect_err("transport must fail").kind,
            ExecutionErrorKind::Transport
        );
    }
}

mod executor_status {
    use super::*;

    #[test]
    fn executor_generated_and_prepared_return_same_status_error() {
        let exchange = ScriptedExchange::status(429, ScriptedBody::Complete(b"slow".to_vec()));
        let (generated, prepared, _, _, generated_context, prepared_context) = run_pair(exchange);
        assert_eq!(generated, prepared);
        assert_eq!(generated_context.trace, prepared_context.trace);
        let error = generated.expect_err("429 must be terminal");
        assert_eq!(error.operation_id, Some(OperationId(41)));
        match error.kind {
            ExecutionErrorKind::Status { status, body } => {
                assert_eq!(status, 429);
                assert_eq!(body.state, StatusBodyState::Complete);
                assert_eq!(body.complete_body, Some(b"slow".to_vec()));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn executor_status_body_capture_is_bounded_and_complete_only() {
        let complete = run_generated(
            OperationId(1),
            ScriptedExchange::status(400, ScriptedBody::Complete(b"12345678".to_vec())),
        )
        .0
        .expect_err("status error");
        assert_eq!(status_capture(&complete).state, StatusBodyState::Complete);
        assert_eq!(
            status_capture(&complete).complete_body.as_deref(),
            Some(b"12345678".as_slice())
        );

        let known = run_generated(
            OperationId(2),
            ScriptedExchange::status(400, ScriptedBody::KnownTooLarge),
        )
        .0
        .expect_err("status error");
        assert_eq!(
            status_capture(&known).state,
            StatusBodyState::OmittedKnownTooLarge
        );
        assert!(status_capture(&known).complete_body.is_none());

        let unknown = run_generated(
            OperationId(3),
            ScriptedExchange::status(
                400,
                ScriptedBody::UnknownChunks(vec![b"1234".to_vec(), b"56789".to_vec()]),
            ),
        )
        .0
        .expect_err("status error");
        assert_eq!(
            status_capture(&unknown).state,
            StatusBodyState::OmittedUnknownTooLarge
        );
        assert!(status_capture(&unknown).complete_body.is_none());

        for (body, expected) in [
            (
                ScriptedBody::ReadFailure,
                StatusBodyState::OmittedReadFailure,
            ),
            (ScriptedBody::Deadline, StatusBodyState::OmittedDeadline),
        ] {
            let error = run_generated(OperationId(4), ScriptedExchange::status(503, body))
                .0
                .expect_err("status error");
            assert_eq!(status_capture(&error).state, expected);
            assert!(status_capture(&error).complete_body.is_none());
        }
    }

    #[test]
    fn executor_status_error_exposes_no_body_prefix() {
        let error = run_generated(
            OperationId(4),
            ScriptedExchange::status(
                500,
                ScriptedBody::UnknownChunks(vec![b"prefix".to_vec(), b"overflow".to_vec()]),
            ),
        )
        .0
        .expect_err("status error");
        assert!(status_capture(&error).complete_body.is_none());
        let formatted = format!("{error:?}");
        assert!(!formatted.contains("prefix"));
        assert!(!formatted.contains("overflow"));
    }

    #[test]
    fn executor_status_path_does_not_call_success_handler() {
        let (_, _, _, _, generated, prepared) = run_pair(ScriptedExchange::status(
            404,
            ScriptedBody::Complete(b"missing".to_vec()),
        ));
        assert_eq!(generated.counters.handler_calls, 0);
        assert_eq!(prepared.counters.handler_calls, 0);
    }

    #[test]
    fn executor_status_deadline_preserves_known_status() {
        let mut exchange =
            ScriptedExchange::status(503, ScriptedBody::Complete(b"unavailable".to_vec()));
        exchange.advance_after_response_head = 10;
        let (generated, prepared, _, _, generated_context, prepared_context) = run_pair(exchange);
        assert_eq!(generated, prepared);
        assert_eq!(generated_context.trace, prepared_context.trace);
        let error = generated.expect_err("status remains terminal");
        match error.kind {
            ExecutionErrorKind::Status { status, body } => {
                assert_eq!(status, 503);
                assert_eq!(body.state, StatusBodyState::OmittedDeadline);
                assert!(body.complete_body.is_none());
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}

mod executor_deadline {
    use super::*;

    #[test]
    fn executor_generated_and_prepared_share_deadline_boundary() {
        let mut exchange = ScriptedExchange::success(7);
        exchange.advance_after_handler = 10;
        assert_pair_equal(exchange);
    }

    #[test]
    fn executor_expired_before_transport_sends_nothing() {
        let mut exchange = ScriptedExchange::success(7);
        exchange.advance_before_transport = 10;
        let (generated_result, prepared_result, _, _, generated, prepared) = run_pair(exchange);
        assert!(matches!(
            generated_result,
            Err(ExecutionError {
                kind: ExecutionErrorKind::OperationDeadline { status: None },
                ..
            })
        ));
        assert_eq!(generated_result, prepared_result);
        assert_eq!(generated.counters.transport_entries, 0);
        assert_eq!(prepared.counters.transport_entries, 0);
    }

    #[test]
    fn executor_expired_at_completion_cannot_return_success() {
        let mut exchange = ScriptedExchange::success(7);
        exchange.advance_after_handler = 10;
        let (generated_result, prepared_result, _, _, generated, prepared) = run_pair(exchange);
        assert!(matches!(
            generated_result,
            Err(ExecutionError {
                kind: ExecutionErrorKind::OperationDeadline { status: Some(200) },
                ..
            })
        ));
        assert_eq!(generated_result, prepared_result);
        assert_eq!(generated.counters.handler_calls, 1);
        assert_eq!(prepared.counters.handler_calls, 1);
    }

    #[test]
    fn executor_records_exactly_one_completion() {
        for exchange in [
            ScriptedExchange::success(7),
            ScriptedExchange::status(500, ScriptedBody::Complete(b"failure".to_vec())),
        ] {
            let (_, _, _, _, generated, prepared) = run_pair(exchange);
            assert_eq!(generated.counters.completion_events, 1);
            assert_eq!(prepared.counters.completion_events, 1);
            assert_eq!(
                generated
                    .trace
                    .iter()
                    .filter(|event| event.phase == TracePhase::OperationCompletion)
                    .count(),
                1
            );
        }
    }
}

mod executor_boundaries {
    use super::*;

    #[test]
    fn executor_final_auth_failure_after_limiter_prevents_transport() {
        let mut exchange = ScriptedExchange::success(7);
        exchange.final_auth_valid = false;
        let (generated_result, prepared_result, _, _, generated, prepared) = run_pair(exchange);
        assert_eq!(generated_result, prepared_result);
        assert!(matches!(
            generated_result,
            Err(ExecutionError {
                kind: ExecutionErrorKind::AuthenticationFinalization,
                ..
            })
        ));
        assert_eq!(generated.counters.limiter_reservations, 1);
        assert_eq!(generated.counters.final_auth_boundaries, 1);
        assert_eq!(generated.counters.transport_entries, 0);
        assert_eq!(generated.counters, prepared.counters);
    }

    #[test]
    fn executor_all_events_share_operation_id() {
        let (_, _, _, _, generated, prepared) = run_pair(ScriptedExchange::success(7));
        assert!(
            generated
                .trace
                .iter()
                .all(|event| event.operation_id == OperationId(41))
        );
        assert!(
            prepared
                .trace
                .iter()
                .all(|event| event.operation_id == OperationId(41))
        );

        let mut allocator = OperationIdAllocator::new(100);
        assert_ne!(
            allocator.allocate().expect("first operation ID"),
            allocator.allocate().expect("second operation ID")
        );
    }

    #[test]
    fn executor_generated_and_prepared_share_hook_points() {
        let (_, _, _, _, generated, prepared) = run_pair(ScriptedExchange::success(7));
        let hook_phases = |context: &ExecutionContext| {
            context
                .trace
                .iter()
                .filter_map(|event| match event.phase {
                    TracePhase::BeforeSendHook
                    | TracePhase::ResponseHeadHook
                    | TracePhase::CompletionHook => Some(event.phase),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(hook_phases(&generated), hook_phases(&prepared));
        assert_eq!(
            hook_phases(&generated),
            vec![
                TracePhase::BeforeSendHook,
                TracePhase::ResponseHeadHook,
                TracePhase::CompletionHook,
            ]
        );
    }

    #[test]
    fn executor_operation_id_overflow_fails_before_boundaries() {
        let mut allocator = OperationIdAllocator::new(u64::MAX);
        let error = allocator.allocate().expect_err("allocation must overflow");
        assert_eq!(error.operation_id, None);
        assert_eq!(error.kind, ExecutionErrorKind::ArithmeticOverflow);
    }
}

mod executor_architecture {
    use super::*;

    const EXECUTOR_SOURCE: &str = include_str!("mod.rs");

    #[test]
    fn executor_has_no_runtime_any_or_boxed_output() {
        let forbidden = ["std::any", "Any", "TypeId", "Box<dyn", "downcast"];
        for token in forbidden {
            assert!(
                !EXECUTOR_SOURCE.contains(token),
                "private executor contains forbidden runtime-erasure token {token}"
            );
        }
    }

    #[test]
    fn executor_prepared_and_generated_sources_have_no_separate_loops() {
        assert_eq!(EXECUTOR_SOURCE.matches("fn execute_typed<").count(), 1);
        assert!(!EXECUTOR_SOURCE.contains("loop {"));
        assert!(!EXECUTOR_SOURCE.contains("while "));
        assert_eq!(EXECUTOR_SOURCE.matches("shared_entries += 1").count(), 1);
    }

    #[test]
    fn executor_bodyless_operation_has_empty_body_metadata() {
        let generated = generated_operation(OperationId(1), WidgetCountHandler::default());
        let prepared =
            prepared_endpoint(OperationId(1), WidgetCountHandler::default()).into_operation();
        assert_eq!(generated.metadata.body, BodyMetadata::Empty);
        assert_eq!(generated.metadata.body, prepared.metadata.body);

        let (_, _, _, _, generated_context, prepared_context) =
            run_pair(ScriptedExchange::success(7));
        assert_eq!(generated_context.counters.body_factory_calls, 0);
        assert_eq!(generated_context.counters.body_bytes_constructed, 0);
        assert_eq!(generated_context.counters, prepared_context.counters);
    }
}

#[test]
fn generated_executor_real_macro_supplies_prepared_head() {
    let endpoint = generated_endpoint();
    let base = crate::__private::Url::parse("https://executor.example.test/v1")
        .expect("executor proof base");
    let builder = crate::__private::HeadBuilder::new(
        &base,
        crate::__private::EndpointSpec::request_meta(&endpoint),
        &crate::__private::RequestOptions::new(),
        crate::__private::DebugLevel::None,
    );
    let (head, _) =
        crate::__private::EndpointSpec::prepare_head_and_body_metadata(&endpoint, builder)
            .expect("generated proof head");
    assert_eq!(head.endpoint(), "ExecutorProofApi::accounts::ById");
    assert_eq!(head.method(), &http::Method::GET);
    assert_eq!(
        head.logical_url().as_str(),
        "https://executor.example.test/v1/accounts/account-alpha/widgets/widget-omega"
    );
}
