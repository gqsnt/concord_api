use super::common::{
    GateableBodyTransport, GateableHooks, GateableTransport, MockResponse, PhaseGate,
    SafeRecordingDebugSink, TestAuthVars, TestCx, TextEndpoint, assert_events_do_not_contain,
    assert_still_pending, auth_policy, rate_limit_policy, wait_bounded,
};
use crate::support::RedactionSentinels;
use bytes::Bytes;
use concord_core::advanced::AuthPlacement;
use concord_core::prelude::{ApiClient, ApiClientError, DebugLevel};
use http::StatusCode;
use std::sync::Arc;
use tokio::sync::Mutex;

const REDACTION_SENTINELS_PR78: RedactionSentinels = RedactionSentinels::new(
    "RAW_AUTH_SENTINEL_PR78",
    "RESPONSE_BODY_SENTINEL_PR78",
    "RESPONSE_OBSERVER_SENTINEL_PR78",
);

fn sentinels() -> [&'static str; 2] {
    REDACTION_SENTINELS_PR78.auth_body()
}

#[tokio::test]
async fn phase_gate_blocks_and_releases_deterministically() {
    let gate = PhaseGate::new();
    gate.block("phase").await;

    let first = {
        let gate = gate.clone();
        tokio::spawn(async move {
            gate.enter("phase").await;
            gate.enter("after_first").await;
        })
    };
    let second = {
        let gate = gate.clone();
        tokio::spawn(async move {
            gate.enter("phase").await;
            gate.enter("after_second").await;
        })
    };

    gate.wait_for("phase", 2).await;
    assert_still_pending("release has not happened", async {
        gate.wait_for("after_first", 1).await;
    })
    .await;

    gate.release_one("phase").await;
    wait_bounded("first release", async {
        loop {
            let events = gate.events().await;
            if events.iter().any(|event| event.starts_with("after_")) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert_eq!(
        gate.events()
            .await
            .iter()
            .filter(|event| event.starts_with("after_"))
            .count(),
        1
    );

    gate.release_all("phase").await;
    first.await.expect("first task should finish");
    second.await.expect("second task should finish");
    assert_eq!(
        gate.events()
            .await
            .iter()
            .filter(|event| event.as_str() == "phase")
            .count(),
        2
    );
}

#[tokio::test]
async fn phase_gate_release_all_does_not_release_future_entries() {
    let gate = PhaseGate::new();
    gate.block("phase").await;

    let first = {
        let gate = gate.clone();
        tokio::spawn(async move {
            gate.enter("phase").await;
        })
    };

    gate.wait_for("phase", 1).await;
    gate.release_all("phase").await;
    first.await.expect("first task should finish");

    let mut second = {
        let gate = gate.clone();
        tokio::spawn(async move {
            gate.enter("phase").await;
        })
    };

    gate.wait_for("phase", 2).await;
    assert_still_pending("release_all must not pre-release future entrants", async {
        (&mut second).await.expect("second task should finish");
    })
    .await;
    gate.release_one("phase").await;
    second.await.expect("second task should finish");
}

#[tokio::test]
async fn phase_gate_release_one_does_not_release_future_entries() {
    let gate = PhaseGate::new();
    gate.block("phase").await;

    let first = {
        let gate = gate.clone();
        tokio::spawn(async move {
            gate.enter("phase").await;
        })
    };

    gate.wait_for("phase", 1).await;
    gate.release_one("phase").await;
    first.await.expect("first task should finish");

    let mut second = {
        let gate = gate.clone();
        tokio::spawn(async move {
            gate.enter("phase").await;
        })
    };

    gate.wait_for("phase", 2).await;
    assert_still_pending("release_one must not pre-release future entrants", async {
        (&mut second).await.expect("second task should finish");
    })
    .await;
    gate.release_one("phase").await;
    second.await.expect("second task should finish");
}

#[tokio::test]
async fn phase_gate_release_one_called_twice_does_not_release_future_entries() {
    let gate = PhaseGate::new();
    gate.block("phase").await;

    let first = {
        let gate = gate.clone();
        tokio::spawn(async move {
            gate.enter("phase").await;
        })
    };

    gate.wait_for("phase", 1).await;
    gate.release_one("phase").await;
    gate.release_one("phase").await;
    first.await.expect("first task should finish");

    let mut second = {
        let gate = gate.clone();
        tokio::spawn(async move {
            gate.enter("phase").await;
        })
    };

    gate.wait_for("phase", 2).await;
    assert_still_pending(
        "duplicate release_one must not pre-release future entrants",
        async { (&mut second).await.expect("second task should finish") },
    )
    .await;
    gate.release_one("phase").await;
    second.await.expect("second task should finish");
}

#[tokio::test]
async fn phase_gate_release_all_called_twice_does_not_release_future_entries() {
    let gate = PhaseGate::new();
    gate.block("phase").await;

    let first = {
        let gate = gate.clone();
        tokio::spawn(async move {
            gate.enter("phase").await;
        })
    };

    gate.wait_for("phase", 1).await;
    gate.release_all("phase").await;
    gate.release_all("phase").await;
    first.await.expect("first task should finish");

    let mut second = {
        let gate = gate.clone();
        tokio::spawn(async move {
            gate.enter("phase").await;
        })
    };

    gate.wait_for("phase", 2).await;
    assert_still_pending(
        "duplicate release_all must not pre-release future entrants",
        async { (&mut second).await.expect("second task should finish") },
    )
    .await;
    gate.release_one("phase").await;
    second.await.expect("second task should finish");
}

#[tokio::test]
async fn phase_gate_cancelled_waiter_does_not_release_future_entries() {
    let gate = PhaseGate::new();
    gate.block("phase").await;
    let events = Arc::new(Mutex::new(Vec::new()));
    let probe = super::common::DropProbe::new("cancelled_waiter", events.clone());
    let token = probe.token();

    let task = {
        let gate = gate.clone();
        tokio::spawn(async move {
            let _token = token;
            gate.enter("phase").await;
        })
    };

    gate.wait_for("phase", 1).await;
    task.abort();
    probe.wait_for(1).await;

    gate.release_one("phase").await;

    let mut second = {
        let gate = gate.clone();
        tokio::spawn(async move {
            gate.enter("phase").await;
        })
    };

    gate.wait_for("phase", 2).await;
    assert_still_pending(
        "cancelled waiter must not leave a reusable release behind",
        async { (&mut second).await.expect("second task should finish") },
    )
    .await;

    gate.release_one("phase").await;
    second.await.expect("second task should finish");
}

#[tokio::test]
async fn drop_probe_counts_future_drop() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    gate.block("future").await;
    let probe = super::common::DropProbe::new("future", events.clone());
    let token = probe.token();

    let task = {
        let gate = gate.clone();
        tokio::spawn(async move {
            let _token = token;
            gate.enter("future").await;
        })
    };

    gate.wait_for("future", 1).await;
    task.abort();
    probe.wait_for(1).await;
    assert_eq!(probe.count(), 1);
    assert!(events.lock().await.contains(&"drop:future".to_string()));
}

#[tokio::test]
async fn gateable_transport_blocks_send_until_released() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    gate.block("transport_send").await;
    let probe = super::common::DropProbe::new("transport_send_future", events.clone());
    let transport = GateableTransport::new(
        gate.clone(),
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "ok")],
    )
    .with_drop_probe(probe.clone());
    let sent = transport.clone();
    let client: ApiClient<TestCx, GateableTransport> =
        ApiClient::with_transport((), TestAuthVars::default(), transport);

    let mut task = tokio::spawn(async move {
        client
            .request(TextEndpoint::default())
            .execute_decoded_with::<concord_core::prelude::Text<String>>()
            .await
    });

    gate.wait_for("transport_send", 1).await;
    assert_eq!(sent.sent_count().await, 1);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), &mut task)
            .await
            .is_err(),
        "request completed before transport release"
    );
    gate.release_all("transport_send").await;
    let decoded = task
        .await
        .expect("task should join")
        .expect("request should succeed");
    assert_eq!(decoded.value(), "ok");
    assert_eq!(gate.events().await, vec!["transport_send"]);
    probe.wait_for(1).await;
}

#[tokio::test]
async fn gateable_body_blocks_reads_and_counts_chunks() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    gate.block("body_chunk").await;
    let probe = super::common::DropProbe::new("body_stream", events.clone());
    let transport = GateableBodyTransport::new(
        gate.clone(),
        events,
        vec![Bytes::from_static(b"ab"), Bytes::from_static(b"cd")],
    )
    .with_drop_probe(probe.clone());
    let body = transport.clone();
    let client: ApiClient<TestCx, GateableBodyTransport> =
        ApiClient::with_transport((), TestAuthVars::default(), transport);

    let mut task = tokio::spawn(async move {
        client
            .request(TextEndpoint::default())
            .execute_decoded_with::<concord_core::prelude::Text<String>>()
            .await
    });

    gate.wait_for("body_chunk", 1).await;
    assert_eq!(body.read_count(), 0);
    gate.release_one("body_chunk").await;
    gate.wait_for("body_chunk", 2).await;
    assert_eq!(body.read_count(), 1);
    assert_still_pending(
        "second chunk must remain blocked until its release",
        async {
            let _ = (&mut task).await.expect("task should join");
        },
    )
    .await;
    gate.release_one("body_chunk").await;
    let decoded = task
        .await
        .expect("task should join")
        .expect("request should succeed");
    assert_eq!(decoded.value(), "abcd");
    assert_eq!(body.read_count(), 2);
    assert_eq!(gate.events().await, vec!["body_chunk", "body_chunk"]);
    probe.wait_for(1).await;
}

#[tokio::test]
async fn counting_rate_limiter_records_lifecycle_completion() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = super::common::MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "ok")],
    );
    let limiter = Arc::new(
        super::common::CountingRateLimiter::new(events.clone()).with_gate(PhaseGate::new()),
    );
    let mut client = super::common::client(TestAuthVars::default(), transport);
    client.configure(|cfg| {
        cfg.rate_limiter(limiter.clone());
    });

    let decoded = client
        .request(TextEndpoint {
            policy: rate_limit_policy(),
            ..TextEndpoint::default()
        })
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await
        .expect("request should succeed");

    assert_eq!(decoded.value(), "ok");
    assert_eq!(
        limiter
            .acquire_started
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    assert_eq!(
        limiter
            .acquire_completed
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    assert_eq!(
        limiter
            .permit_created
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    assert_eq!(
        limiter
            .response_observed
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    assert_eq!(
        limiter
            .response_lifecycle_completed
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    let events = events.lock().await.clone();
    assert!(events.contains(&"rate_acquire_started".to_string()));
    assert!(events.contains(&"rate_permit_created".to_string()));
    assert!(events.contains(&"rate_lifecycle_completed".to_string()));
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("rate_response:"))
    );

    let fail_events = Arc::new(Mutex::new(Vec::new()));
    let fail_gate = PhaseGate::new();
    let fail_transport = GateableTransport::new(
        fail_gate.clone(),
        fail_events.clone(),
        vec![MockResponse::text(StatusCode::OK, "unexpected")],
    );
    let sent = fail_transport.clone();
    let failing_limiter = Arc::new(super::common::CountingRateLimiter::new(fail_events).failing());
    let mut client: ApiClient<TestCx, GateableTransport> =
        ApiClient::with_transport((), TestAuthVars::default(), fail_transport);
    client.configure(|cfg| {
        cfg.rate_limiter(failing_limiter.clone());
    });

    let err = client
        .request(TextEndpoint {
            policy: rate_limit_policy(),
            ..TextEndpoint::default()
        })
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await
        .expect_err("acquire failure should stop before transport");
    assert!(matches!(err, ApiClientError::RateLimit { .. }));
    assert_eq!(
        err.rate_limit_error().map(|err| err.kind()),
        Some(concord_core::advanced::RateLimitErrorKind::AcquireFailed)
    );
    assert_eq!(sent.sent_count().await, 0);
    assert_eq!(
        failing_limiter
            .acquire_started
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
}

#[tokio::test]
async fn gateable_hooks_block_pre_send_before_transport() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let gate = PhaseGate::new();
    gate.block("hook_pre_send").await;
    let transport = GateableTransport::new(
        gate.clone(),
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "ok")],
    );
    let sent = transport.clone();
    let mut client: ApiClient<TestCx, GateableTransport> =
        ApiClient::with_transport((), TestAuthVars::default(), transport);
    client.configure(|cfg| {
        cfg.runtime_hooks(Arc::new(GateableHooks::new(gate.clone(), events.clone())));
    });

    let task = tokio::spawn(async move {
        client
            .request(TextEndpoint::default())
            .execute_decoded_with::<concord_core::prelude::Text<String>>()
            .await
    });

    gate.wait_for("hook_pre_send", 1).await;
    assert_eq!(sent.sent_count().await, 0);
    gate.release_all("hook_pre_send").await;
    let decoded = task
        .await
        .expect("task should join")
        .expect("request should succeed");
    assert_eq!(decoded.value(), "ok");
    let events = events.lock().await.clone();
    let hook_pos = events
        .iter()
        .position(|event| event == "hook_pre_send_started")
        .expect("hook event");
    let transport_pos = events
        .iter()
        .position(|event| event == "transport_send_start")
        .expect("transport event");
    assert!(hook_pos < transport_pos);
}

#[tokio::test]
async fn harness_observer_surfaces_remain_body_auth_free() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = super::common::MockTransport::new(
        events.clone(),
        vec![MockResponse::text(
            StatusCode::OK,
            REDACTION_SENTINELS_PR78.body,
        )],
    );
    let mut client = super::common::client(
        TestAuthVars {
            token: Some(REDACTION_SENTINELS_PR78.auth.to_string()),
            identity: "auth",
        },
        transport,
    );
    client.configure(|cfg| {
        cfg.rate_limiter(Arc::new(super::common::ObservationRateLimiter::new(
            events.clone(),
        )))
        .runtime_hooks(Arc::new(super::common::ObservationRuntimeHooks::new(
            events.clone(),
        )))
        .debug_level(DebugLevel::VV)
        .debug_sink(Arc::new(SafeRecordingDebugSink::new(events.clone())));
    });

    let mut policy = auth_policy(AuthPlacement::Bearer);
    policy.rate_limit = rate_limit_policy().rate_limit;

    let decoded = client
        .request(TextEndpoint {
            policy,
            ..TextEndpoint::default()
        })
        .execute_decoded_with::<concord_core::prelude::Text<String>>()
        .await
        .expect("request should succeed");

    assert_eq!(decoded.value(), REDACTION_SENTINELS_PR78.body);
    assert_events_do_not_contain(&events, &sentinels()).await;
}

#[tokio::test]
async fn harness_waits_fail_fast_instead_of_hanging() {
    let gate = PhaseGate::new();
    let err = gate
        .try_wait_for("never_happens", 1)
        .await
        .expect_err("missing phase should return bounded timeout");
    assert_eq!(err.label, "never_happens");
}
