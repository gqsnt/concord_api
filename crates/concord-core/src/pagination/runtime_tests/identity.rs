use super::*;

fn forbidden_destination_core(
    executor: &DeterministicExecutor,
    hooks: Arc<PaginationProbe>,
    limiter: Arc<LimiterProbe>,
    debug: Arc<Mutex<Vec<String>>>,
) -> ClientCore {
    let limiter: Arc<dyn RateLimiter> = limiter;
    ClientCore::build_with_development(
        BASE,
        HttpConfigBuilder::new().http1_only(),
        RuntimeConfigBuilder::new()
            .operation_observer(hooks.clone())
            .operation_observer(hooks.clone())
            .operation_observer(hooks)
            .rate_limiter(limiter)
            .debug(DebugLevel::Headers),
        executor,
    )
    .expect("forbidden-destination core")
    .with_debug_output(debug)
}

/// A continuation may not move the logical origin or a rate-limit dimension:
/// the page that proposed it is withheld and the forbidden request is never
/// prepared for dispatch. Observers, errors and debug output stay value-free.
#[tokio::test]
async fn continuations_cannot_change_origin_or_rate_dimensions() {
    const INITIAL: &str = "page-host-initial-sentinel-73190584";
    const SECOND: &str = "page-host-second-sentinel-10827319";

    for (move_host, expected) in [
        (true, PaginationErrorKind::CrossOriginContinuation),
        (false, PaginationErrorKind::ForbiddenContinuationChange),
    ] {
        let executor = DeterministicExecutor::new();
        executor
            .application()
            .queue_expected_response(
                ExpectedRequest::new().native_url(format!(
                    "http://{INITIAL}.pagination-runtime.example.test/v1/host-rate-items?stable=public&cursor={INITIAL}"
                )),
                ScriptedResponse::success(Bytes::from(format!(
                    "first-item-sentinel-a38f|{SECOND}"
                ))),
            )
            .expect("host page response");
        let hooks = Arc::new(PaginationProbe::default());
        let limiter = Arc::new(LimiterProbe::default());
        let debug = Arc::new(Mutex::new(Vec::new()));
        let core = forbidden_destination_core(
            &executor,
            Arc::clone(&hooks),
            Arc::clone(&limiter),
            Arc::clone(&debug),
        );
        let endpoint = ForbiddenDestinationEndpoint {
            host: INITIAL.to_owned(),
            rate_dimension: INITIAL.to_owned(),
            move_host,
            calls: Calls::default(),
        };
        let mut pages: Pages<String> = pages_request(Request::new(core, endpoint), 5);
        let error = pages.next_page().await.expect_err("forbidden continuation");
        assert_eq!(error.pagination_kind(), Some(expected));
        assert_eq!(
            error.stage(),
            crate::error::FailureStage::PaginationDecision
        );
        assert!(pages.next_page().await.expect("terminal").is_none());
        let summary = pages.finish();
        assert_eq!(summary.stop_reason(), StopReason::Failed);
        assert_eq!(summary.pages_received(), 1);
        assert_eq!(summary.pages_delivered(), 0);
        assert_eq!(executor.application().request_count().expect("count"), 1);
        assert_eq!(limiter.acquisition_pages(), [0]);
        let origin = limiter.acquisitions.lock().expect("acquisitions")[0]
            .1
            .clone();
        assert_eq!(
            origin.as_deref(),
            Some(format!("http://{INITIAL}.pagination-runtime.example.test:80").as_str()),
            "the limiter receives the actual protected origin"
        );

        let mut automatic = Vec::new();
        automatic.extend(
            hooks
                .before
                .lock()
                .expect("before")
                .iter()
                .map(|record| record.safe.clone()),
        );
        automatic.extend(
            hooks
                .response
                .lock()
                .expect("response")
                .iter()
                .map(|record| record.5.clone()),
        );
        automatic.extend(
            hooks
                .completion
                .lock()
                .expect("completion")
                .iter()
                .map(|record| record.safe.clone()),
        );
        automatic.push(debug.lock().expect("debug").join("\n"));
        automatic.push(format!("{error} {error:?} {summary:?}"));
        automatic.extend(
            executor
                .application()
                .captures()
                .expect("captures")
                .into_iter()
                .map(|capture| format!("{capture:?}")),
        );
        let automatic = automatic.join("\n");
        for sentinel in [INITIAL, SECOND] {
            assert!(!automatic.contains(sentinel));
        }
        let mut source: Option<&(dyn std::error::Error + 'static)> = error.source();
        while let Some(current) = source {
            for sentinel in [INITIAL, SECOND] {
                assert!(!current.to_string().contains(sentinel));
                assert!(!format!("{current:?}").contains(sentinel));
            }
            source = current.source();
        }
    }
}

#[cfg(feature = "default-tls")]
fn rate_key_core(
    executor: &DeterministicExecutor,
    hooks: Arc<PaginationProbe>,
    limiter: Arc<KeyBoundLimiter>,
    debug: Arc<Mutex<Vec<String>>>,
) -> ClientCore {
    let limiter: Arc<dyn RateLimiter> = limiter;
    ClientCore::build_with_development(
        "https://pagination-runtime.example.test/v1/",
        HttpConfigBuilder::new().http1_only(),
        RuntimeConfigBuilder::new()
            .operation_observer(hooks.clone())
            .operation_observer(hooks.clone())
            .operation_observer(hooks)
            .rate_limiter(limiter)
            .debug(DebugLevel::Headers),
        executor,
    )
    .expect("pagination rate-key development core")
    .with_debug_output(debug)
}

#[cfg(feature = "default-tls")]
fn collect_rate_key(
    core: ClientCore,
    key: String,
    credential: CredentialRef<AccessToken>,
) -> BoxFuture<'static, Result<Prefix<String>, ApiError>> {
    Box::pin(
        collect_prefix_request(
            Request::new(
                core,
                RateKeyEndpoint {
                    key,
                    credential,
                    calls: Calls::default(),
                },
            ),
            1,
        )
        .into_future(),
    )
}

/// Public preparation (including the initial-cursor comparison) enforces the
/// actual rate-key limit before any credential, limiter or transport work.
#[tokio::test]
#[cfg(feature = "default-tls")]
async fn pagination_rate_key_limit_uses_actual_value_and_cuts_off_protected_work() {
    const BOUNDARY: &str = "page-rate-key-boundary-sentinel-4a871f";
    const KEY_NAME: &str = "endpoint.cursor";
    const CANONICAL_UTF8_OVERHEAD: usize = 1 + "utf8".len() + 8;
    let key_limit = 4 + KEY_NAME.len() + 8 + CANONICAL_UTF8_OVERHEAD + BOUNDARY.len();

    let accepted_executor = DeterministicExecutor::new();
    accepted_executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new()
                .native_url(format!(
                    "https://pagination-runtime.example.test/v1/rate-key-items?stable=public&cursor={BOUNDARY}"
                ))
                .header(
                    http::HeaderName::from_static("x-page-cursor"),
                    http::HeaderValue::from_static(BOUNDARY),
                ),
            ScriptedResponse::success(Bytes::from_static(b"|-")),
        )
        .expect("accepted boundary response");
    let accepted_provider_calls = Arc::new(AtomicUsize::new(0));
    let accepted_selector = SlotSelector::provider(
        CountingRateKeyProvider {
            calls: Arc::clone(&accepted_provider_calls),
        },
        Duration::ZERO,
    );
    let accepted_hooks = Arc::new(PaginationProbe::default());
    let accepted_limiter = Arc::new(KeyBoundLimiter {
        acquisitions: AtomicUsize::new(0),
        key_limit,
    });
    let accepted_debug = Arc::new(Mutex::new(Vec::new()));
    let accepted_core = rate_key_core(
        &accepted_executor,
        Arc::clone(&accepted_hooks),
        Arc::clone(&accepted_limiter),
        Arc::clone(&accepted_debug),
    );
    let prefix = collect_rate_key(
        accepted_core,
        BOUNDARY.to_owned(),
        CredentialRef::new(accepted_selector),
    )
    .await
    .expect("actual key at configured boundary");
    assert!(prefix.items().is_empty());
    assert_eq!(accepted_provider_calls.load(Ordering::SeqCst), 1);
    assert_eq!(accepted_limiter.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(
        accepted_executor
            .application()
            .request_count()
            .expect("accepted request count"),
        1
    );
    assert_eq!(accepted_hooks.before_count(), 1);
    assert_eq!(accepted_hooks.response.lock().expect("response").len(), 1);
    assert_eq!(
        accepted_hooks.completion.lock().expect("completion").len(),
        1
    );
    let accepted_automatic = format!(
        "{:?}\n{}\n{:?}",
        accepted_executor
            .application()
            .captures()
            .expect("accepted captures"),
        accepted_debug.lock().expect("accepted debug").join("\n"),
        prefix.summary(),
    );
    assert!(!accepted_automatic.contains(BOUNDARY));

    let excess = format!("{BOUNDARY}x");
    let excess_executor = DeterministicExecutor::new();
    let excess_provider_calls = Arc::new(AtomicUsize::new(0));
    let excess_selector = SlotSelector::provider(
        CountingRateKeyProvider {
            calls: Arc::clone(&excess_provider_calls),
        },
        Duration::ZERO,
    );
    let excess_hooks = Arc::new(PaginationProbe::default());
    let excess_limiter = Arc::new(KeyBoundLimiter {
        acquisitions: AtomicUsize::new(0),
        key_limit,
    });
    let excess_debug = Arc::new(Mutex::new(Vec::new()));
    let excess_core = rate_key_core(
        &excess_executor,
        Arc::clone(&excess_hooks),
        Arc::clone(&excess_limiter),
        Arc::clone(&excess_debug),
    );
    let error = collect_rate_key(
        excess_core,
        excess.clone(),
        CredentialRef::new(excess_selector),
    )
    .await
    .expect_err("first actual key byte above the configured limit");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(
        error.configuration_kind(),
        Some(crate::error::ConfigurationErrorKind::InvalidRateLimit)
    );
    assert_eq!(excess_provider_calls.load(Ordering::SeqCst), 0);
    assert_eq!(excess_limiter.acquisitions.load(Ordering::SeqCst), 0);
    assert_eq!(
        excess_executor
            .application()
            .request_count()
            .expect("excess request count"),
        0
    );
    assert_eq!(excess_hooks.before_count(), 0);
    assert!(excess_hooks.response.lock().expect("response").is_empty());
    assert!(
        excess_hooks
            .completion
            .lock()
            .expect("completion")
            .is_empty()
    );
    let aggregate = excess_hooks.aggregate();
    assert_eq!(aggregate.len(), 1);
    assert_eq!(aggregate[0].pages_completed, 0);
    assert_eq!(aggregate[0].items_collected, 0);
    assert_eq!(aggregate[0].final_status, None);

    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(&error);
    while let Some(current) = source {
        assert!(!current.to_string().contains(&excess));
        assert!(!format!("{current:?}").contains(&excess));
        source = current.source();
    }
    assert!(
        !excess_debug
            .lock()
            .expect("excess debug")
            .join("\n")
            .contains(&excess)
    );
}
