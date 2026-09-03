use super::*;

#[tokio::test]
async fn selected_limiter_receives_actual_page_hosts_while_automatic_surfaces_stay_redacted() {
    const INITIAL: &str = "page-host-initial-sentinel-73190584";
    const SECOND: &str = "page-host-second-sentinel-10827319";
    const THIRD: &str = "page-host-third-sentinel-46205731";

    let executor = DeterministicExecutor::new();
    for (host, cursor, response) in [
        (
            INITIAL,
            INITIAL,
            format!("first-item-sentinel-a38f|{SECOND}"),
        ),
        (SECOND, SECOND, format!("second-item-sentinel-b49e|{THIRD}")),
    ] {
        executor
            .application()
            .queue_expected_response(
                ExpectedRequest::new()
                    .native_url(format!(
                        "http://{host}.pagination-runtime.example.test/v1/host-rate-items?stable=public&cursor={cursor}"
                    ))
                    .header(
                        http::HeaderName::from_static("x-page-cursor"),
                        http::HeaderValue::from_str(cursor).expect("cursor header"),
                    ),
                ScriptedResponse::success(Bytes::from(response)),
            )
            .expect("host page response");
    }

    let hooks = Arc::new(PaginationProbe::default());
    let limiter = Arc::new(HostLimiterProbe::default());
    let configured_limiter: Arc<dyn RateLimiter> = limiter.clone();
    let debug = Arc::new(Mutex::new(Vec::new()));
    let core = ClientCore::build_with_development(
        "http://pagination-runtime.example.test/v1/",
        HttpConfigBuilder::new().http1_only(),
        RuntimeConfigBuilder::new()
            .request_gate(hooks.clone())
            .response_gate(hooks.clone())
            .completion_observer(hooks.clone())
            .rate_limiter(configured_limiter)
            .debug(DebugLevel::Headers),
        &executor,
    )
    .expect("host pagination core")
    .with_debug_output(Arc::clone(&debug));

    let error = pagination_error(
        paginate_with(
            Request::new(
                core,
                HostRateEndpoint {
                    host: INITIAL.to_owned(),
                },
            ),
            PageLimit::CompleteWithin(NonZeroU32::new(2).expect("completion limit")),
        )
        .await,
    );
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::CompletionLimitExceeded)
    );

    let acquisitions = limiter
        .acquisitions
        .lock()
        .expect("host limiter acquisitions");
    let observations = limiter
        .observations
        .lock()
        .expect("host limiter observations");
    assert_eq!(observations.as_slice(), acquisitions.as_slice());
    assert_eq!(
        acquisitions
            .iter()
            .map(|context| context.0)
            .collect::<Vec<_>>(),
        [0, 1]
    );
    let initial_host = format!("{INITIAL}.pagination-runtime.example.test");
    let second_host = format!("{SECOND}.pagination-runtime.example.test");
    let canonical = |value: &str| {
        crate::rate_limit::encode_canonical(
            &value.to_owned(),
            crate::rate_limit::MAX_RATE_LIMIT_KEY_BYTES,
        )
        .expect("canonical test value")
        .into_vec()
    };
    assert_eq!(acquisitions[0].2.as_deref(), Some(initial_host.as_str()));
    assert_eq!(acquisitions[1].2.as_deref(), Some(second_host.as_str()));
    assert_ne!(acquisitions[0].2, acquisitions[1].2);
    assert_eq!(
        acquisitions[0].3,
        [
            ("host".to_owned(), canonical(&initial_host)),
            ("endpoint.host".to_owned(), canonical(INITIAL)),
        ]
    );
    assert_eq!(
        acquisitions[1].3,
        [
            ("host".to_owned(), canonical(&second_host)),
            ("endpoint.host".to_owned(), canonical(SECOND)),
        ]
    );

    let mut automatic = Vec::new();
    automatic.extend(acquisitions.iter().map(|context| context.1.clone()));
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
    automatic.push(format!("{error} {error:?}"));
    automatic.extend(
        executor
            .application()
            .captures()
            .expect("captures")
            .into_iter()
            .map(|capture| format!("{capture:?}")),
    );
    let automatic = automatic.join("\n");
    for sentinel in [INITIAL, SECOND, THIRD] {
        assert!(!automatic.contains(sentinel));
    }
    let mut source: Option<&(dyn std::error::Error + 'static)> = error.source();
    while let Some(current) = source {
        for sentinel in [INITIAL, SECOND, THIRD] {
            assert!(!current.to_string().contains(sentinel));
            assert!(!format!("{current:?}").contains(sentinel));
        }
        source = current.source();
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
            .request_gate(hooks.clone())
            .response_gate(hooks.clone())
            .completion_observer(hooks)
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
) -> BoxFuture<'static, Result<Vec<String>, ApiError>> {
    paginate_with(
        Request::new(
            core,
            RateKeyEndpoint {
                key: key.clone(),
                credential,
            },
        ),
        PageLimit::Prefix(1),
    )
}

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
    let items = collect_rate_key(
        accepted_core,
        BOUNDARY.to_owned(),
        CredentialRef::new(accepted_selector),
    )
    .await
    .expect("actual key at configured boundary");
    assert!(items.is_empty());
    assert_eq!(accepted_provider_calls.load(Ordering::SeqCst), 1);
    assert_eq!(accepted_limiter.acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(
        accepted_executor
            .application()
            .request_count()
            .expect("accepted request count"),
        1
    );
    assert_eq!(accepted_hooks.before.lock().expect("before").len(), 1);
    assert_eq!(accepted_hooks.response.lock().expect("response").len(), 1);
    assert!(
        accepted_hooks
            .completion
            .lock()
            .expect("completion")
            .is_empty()
    );
    let accepted_hook_safe = {
        let before = accepted_hooks.before.lock().expect("accepted before");
        let response = accepted_hooks.response.lock().expect("accepted response");
        let completion = accepted_hooks
            .completion
            .lock()
            .expect("accepted completion");
        format!(
            "{}\n{}\n{}",
            before
                .iter()
                .map(|record| record.safe.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            response
                .iter()
                .map(|record| record.5.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            completion
                .iter()
                .map(|record| record.safe.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
        )
    };
    let accepted_automatic = format!(
        "{:?}\n{}\n{accepted_hook_safe}",
        accepted_executor
            .application()
            .captures()
            .expect("accepted captures"),
        accepted_debug.lock().expect("accepted debug").join("\n")
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
    assert!(excess_hooks.before.lock().expect("before").is_empty());
    assert!(excess_hooks.response.lock().expect("response").is_empty());
    assert!(
        excess_hooks
            .completion
            .lock()
            .expect("completion")
            .is_empty()
    );
    let aggregate = excess_hooks.aggregate.lock().expect("aggregate");
    assert_eq!(aggregate.len(), 1);
    assert_eq!(aggregate[0].pages_completed, 0);
    assert_eq!(aggregate[0].items_collected, 0);
    assert_eq!(aggregate[0].final_status, None);
    drop(aggregate);

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
