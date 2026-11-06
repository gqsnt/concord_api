//! End-to-end workflows against deterministic loopback state machines; no provider calls.
mod common;
use common::*;
use concord_example_testserver::{Gate, Script, TestServer};
use concord_sdk_snowflake::{
    pagination::{OnError, Options, Outcome},
    *,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

fn policy(attempts: usize) -> PollPolicy {
    PollPolicy {
        max_attempts: nz(attempts),
        interval: Duration::from_secs(2),
        max_elapsed: Duration::from_secs(20),
    }
}
async fn settle() {
    for _ in 0..2000 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn workflow_a_pending_perturbation_reuses_first_then_fetches_two_independent_partitions() {
    let statuses = Arc::new(AtomicUsize::new(0));
    let seen = statuses.clone();
    let second_arrived = Gate::new();
    let gate = second_arrived.clone();
    let s = TestServer::start(move |r| {
        if r.method == "POST" {
            return json(202, PENDING_A);
        }
        match r.query_all("partition").first().map(String::as_str) {
            Some("1") => gzip(GZIP1).after(gate.clone()),
            Some("2") => {
                gate.open();
                gzip(GZIP2)
            }
            None => {
                if seen.fetch_add(1, Ordering::SeqCst) < 2 {
                    json(202, PENDING_A)
                } else {
                    json(200, THREE)
                }
            }
            _ => panic!("unexpected partition"),
        }
    })
    .await;
    let submission = client(&s).submit(context(), statement()).await.unwrap();
    tokio::time::pause();
    let waiter =
        tokio::spawn(async move { submission.wait(policy(4), &LocalStop::default()).await });
    spin_until(|| statuses.load(Ordering::SeqCst) == 1).await;
    settle().await;
    assert_eq!(statuses.load(Ordering::SeqCst), 1);
    tokio::time::advance(Duration::from_millis(2001)).await;
    spin_until(|| statuses.load(Ordering::SeqCst) == 2).await;
    settle().await;
    tokio::time::advance(Duration::from_millis(2001)).await;
    spin_until(|| waiter.is_finished()).await;
    let first = complete(waiter.await.unwrap().unwrap());
    assert_eq!(statuses.load(Ordering::SeqCst), 3);
    assert_eq!(
        first.response().value.result_set_meta_data.partition_info[1].row_count,
        1
    );
    let mut parts = first.bounded_partitions(nz(8), nz(2));
    let first = parts.next().await.unwrap().unwrap();
    assert_eq!(first.index, 0);
    assert_eq!(first.rows[1][1], None);
    assert_eq!(first.plan().query().context(), &context());
    let fetcher = tokio::spawn(async move {
        let mut data = vec![first];
        while let Some(p) = parts.next().await {
            data.push(p.unwrap());
        }
        (data, parts.finish())
    });
    spin_until(|| fetcher.is_finished()).await;
    let (data, report) = fetcher.await.unwrap();
    assert!(second_arrived.is_open());
    assert!(report.is_complete());
    assert_eq!(report.missing().count(), 0);
    assert_eq!(report.remaining.progress.admitted, 2);
    assert_eq!(data.iter().map(|p| p.index).collect::<Vec<_>>(), [0, 1, 2]);
    assert_eq!(data.iter().map(|p| p.rows.len()).sum::<usize>(), 5);
    assert!(data.iter().all(|p| p.plan().query().handle().as_str() == A));
    let requests = s.requests();
    assert_eq!(requests.len(), 6); // POST + three status checks + two later partitions
    assert!(!requests.iter().any(|r| r.query_all("partition") == ["0"]));
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.query_all("partition") == ["1"])
            .count(),
        1
    );
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.query_all("partition") == ["2"])
            .count(),
        1
    );
    s.shutdown().await;
}

#[tokio::test]
async fn immediate_one_empty_and_second_partition_variants_use_public_workflow() {
    for body in [
        ONE,
        include_bytes!("../fixtures/complete-empty.json").as_slice(),
        TWO,
    ] {
        let body = body.to_vec();
        let s = TestServer::start(move |r| {
            if r.method == "POST" {
                json(200, &body)
            } else {
                gzip(GZIP1)
            }
        })
        .await;
        let submission = client(&s).submit(context(), statement()).await.unwrap();
        let first = complete(
            submission
                .wait(policy(1), &LocalStop::default())
                .await
                .unwrap(),
        );
        let count = first.plan().metadata().partition_info.len();
        let expected = first.plan().metadata().num_rows;
        let mut parts = first.bounded_partitions(nz(2), nz(2));
        let mut rows = 0;
        while let Some(part) = parts.next().await {
            rows += part.unwrap().rows.len() as u64;
        }
        assert!(parts.finish().is_complete());
        assert_eq!(rows, expected);
        assert_eq!(s.requests().len(), count); // no status GET for immediate completion
        s.shutdown().await;
    }
}

#[tokio::test]
async fn plan_can_fetch_index_two_before_one_using_original_descriptors() {
    let s = TestServer::start(
        |r| match r.query_all("partition").first().map(String::as_str) {
            None => json(200, THREE),
            Some("2") => gzip(GZIP2),
            Some("1") => json(200, include_bytes!("../fixtures/partition-bare.json")),
            _ => panic!(),
        },
    )
    .await;
    let f = first(
        client(&s)
            .submit(context(), statement())
            .await
            .unwrap()
            .state,
    );
    let plan = f.plan();
    let two = plan.fetch(2).await.unwrap();
    let one = plan.fetch(1).await.unwrap();
    assert_eq!(
        two.rows.len() as u64,
        plan.metadata().partition_info[2].row_count
    );
    assert_eq!(
        one.rows.len() as u64,
        plan.metadata().partition_info[1].row_count
    );
    assert_eq!(plan.metadata(), &f.response().value.result_set_meta_data);
    assert_eq!(s.requests()[1].query_all("partition"), ["2"]);
    assert_eq!(s.requests()[2].query_all("partition"), ["1"]);
    s.shutdown().await;
}

#[tokio::test]
async fn concurrency_window_does_not_admit_third_request_until_a_slot_opens() {
    let four = mutate(THREE, |v| {
        let m = &mut v["resultSetMetaData"];
        m["numRows"] = 6.into();
        let descriptor = m["partitionInfo"][1].clone();
        m["partitionInfo"].as_array_mut().unwrap().push(descriptor);
    });
    let gate = Gate::new();
    let hold = gate.clone();
    let s =
        TestServer::start(
            move |r| match r.query_all("partition").first().map(String::as_str) {
                None => json(200, &four),
                Some("1") => gzip(GZIP1).after(hold.clone()),
                Some("2") => gzip(GZIP2).after(hold.clone()),
                Some("3") => gzip(GZIP1),
                _ => panic!(),
            },
        )
        .await;
    let mut parts = first(
        client(&s)
            .submit(context(), statement())
            .await
            .unwrap()
            .state,
    )
    .bounded_partitions(nz(3), nz(2));
    assert_eq!(parts.next().await.unwrap().unwrap().index, 0);
    let task = tokio::spawn(async move {
        let mut indexes = Vec::new();
        while let Some(part) = parts.next().await {
            indexes.push(part.unwrap().index);
        }
        (indexes, parts.finish())
    });
    spin_until(|| s.requests().len() == 3).await;
    settle().await;
    assert_eq!(s.requests().len(), 3); // POST + only partitions 1 and 2, both held
    assert!(!task.is_finished());
    gate.open();
    spin_until(|| task.is_finished()).await;
    let (indexes, report) = task.await.unwrap();
    assert_eq!(indexes, [1, 2, 3]);
    assert!(report.is_complete());
    assert_eq!(report.remaining.progress.admitted, 3);
    s.shutdown().await;
}

#[tokio::test]
async fn budget_and_caller_stop_report_missing_work_without_remote_cancellation() {
    for early_stop in [false, true] {
        let s = TestServer::start(|r| {
            if r.method == "POST" {
                json(200, THREE)
            } else {
                gzip(GZIP1)
            }
        })
        .await;
        let mut parts = first(
            client(&s)
                .submit(context(), statement())
                .await
                .unwrap()
                .state,
        )
        .bounded_partitions(nz(1), nz(2));
        parts.next().await.unwrap().unwrap();
        if !early_stop {
            assert_eq!(parts.next().await.unwrap().unwrap().index, 1);
            assert!(parts.next().await.is_none());
        }
        let report = parts.finish();
        assert!(!report.is_complete());
        if early_stop {
            assert_eq!(report.missing().collect::<Vec<_>>(), [1, 2]);
        } else {
            assert_eq!(report.remaining.outcome, Outcome::Bounded);
            assert_eq!(report.missing().collect::<Vec<_>>(), [2]);
        }
        assert!(!s.requests().iter().any(|r| r.path().ends_with("/cancel")));
        s.shutdown().await;
    }
    let s = TestServer::start(|_| json(200, THREE)).await;
    let parts = first(
        client(&s)
            .submit(context(), statement())
            .await
            .unwrap()
            .state,
    )
    .bounded_partitions(nz(4), nz(2));
    let report = parts.finish();
    assert_eq!(report.missing().collect::<Vec<_>>(), [0, 1, 2]);
    assert_eq!(report.unconsumed_first.unwrap().rows.len(), 2);
    s.shutdown().await;
}

#[tokio::test]
async fn failed_partition_is_keyed_and_continue_keeps_other_successes() {
    let s = TestServer::start(
        |r| match r.query_all("partition").first().map(String::as_str) {
            None => json(200, THREE),
            Some("1") => json(503, include_bytes!("../fixtures/rate-limit.json")),
            Some("2") => gzip(GZIP2),
            _ => panic!(),
        },
    )
    .await;
    let mut parts = first(
        client(&s)
            .submit(context(), statement())
            .await
            .unwrap()
            .state,
    )
    .partitions(
        Options::new(nz(2).into())
            .window(nz(2))
            .on_error(OnError::Continue),
    );
    let mut successes = Vec::new();
    let mut failures = Vec::new();
    while let Some(part) = parts.next().await {
        match part {
            Ok(p) => successes.push(p.index),
            Err(f) => failures.push(f),
        }
    }
    assert_eq!(successes, [0, 2]);
    assert_eq!(failures[0].index, 1);
    assert_eq!(
        failures[0].error.response_meta().unwrap().status().as_u16(),
        503
    );
    let report = parts.finish();
    assert!(!report.is_complete());
    assert_eq!(report.missing().collect::<Vec<_>>(), [1]);
    assert_eq!(report.remaining.failed().copied().collect::<Vec<_>>(), [1]);
    s.shutdown().await;
}

#[tokio::test]
async fn bounded_polling_uses_virtual_time_and_never_assumes_remote_cancel() {
    let s = TestServer::start(|_| json(202, PENDING_A)).await;
    let submission = client(&s).submit(context(), statement()).await.unwrap();
    let query = submission.query.clone();
    tokio::time::pause();
    let task = tokio::spawn(async move { submission.wait(policy(2), &LocalStop::default()).await });
    spin_until(|| s.requests().len() == 2).await;
    settle().await;
    tokio::time::advance(Duration::from_secs(1)).await;
    settle().await;
    assert_eq!(s.requests().len(), 2);
    tokio::time::advance(Duration::from_millis(1001)).await;
    spin_until(|| task.is_finished()).await;
    match task.await.unwrap().unwrap() {
        PollOutcome::Unresolved {
            reason: PollEnd::Attempts,
            attempts: 2,
            last_pending: Some(p),
        } => assert_eq!(p.value.statement_handle.as_ref(), Some(query.handle())),
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(s.requests().len(), 3);
    assert!(s.requests().iter().all(|r| !r.path().ends_with("/cancel")));
    s.shutdown().await;
}

#[tokio::test]
async fn local_stop_and_deadline_interrupt_stalled_poll_without_remote_cancel() {
    for local_stop in [true, false] {
        let gate = Gate::new();
        let hold = gate.clone();
        let s = TestServer::start(move |r| {
            if r.method == "POST" {
                json(202, PENDING_A)
            } else {
                json(200, ONE).after(hold.clone())
            }
        })
        .await;
        let submission = client(&s).submit(context(), statement()).await.unwrap();
        let query = submission.query.clone();
        let stop = LocalStop::default();
        let task_stop = stop.clone();
        tokio::time::pause();
        let task = tokio::spawn(async move { submission.wait(policy(10), &task_stop).await });
        spin_until(|| s.requests().len() == 2).await;
        if local_stop {
            stop.stop();
        } else {
            tokio::time::advance(Duration::from_millis(20001)).await;
        }
        spin_until(|| task.is_finished()).await;
        match task.await.unwrap().unwrap() {
            PollOutcome::Unresolved {
                reason,
                attempts: 1,
                ..
            } => assert_eq!(
                reason,
                if local_stop {
                    PollEnd::LocalStop
                } else {
                    PollEnd::Deadline
                }
            ),
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(query.handle().as_str(), A);
        assert!(!s.requests().iter().any(|r| r.path().ends_with("/cancel")));
        gate.open();
        // The same durable handle can still observe completion after local stop.
        let check = tokio::spawn(async move { query.status().await });
        spin_until(|| check.is_finished()).await;
        assert!(matches!(
            check.await.unwrap().unwrap(),
            StatementState::Complete(_)
        ));
        s.shutdown().await;
        tokio::time::resume();
    }
}

#[tokio::test]
async fn workflow_b_explicit_cancel_then_observe_correlated_terminal_cancellation() {
    let script = Script::new([
        json(202, PENDING_B),
        json(200, CANCELLED),
        json(202, PENDING_B),
        json(422, CANCELLED),
    ]);
    let s = TestServer::start(move |_| script.next()).await;
    let query = client(&s)
        .submit(context(), Statement::new("select synthetic_long_running"))
        .await
        .unwrap()
        .query;
    tokio::time::pause();
    let task = tokio::spawn(async move {
        query
            .cancel_and_wait(id(CANCEL_ID), policy(3), &LocalStop::default())
            .await
    });
    spin_until(|| s.requests().len() == 3).await;
    settle().await;
    tokio::time::advance(Duration::from_millis(2001)).await;
    spin_until(|| task.is_finished()).await;
    let observation = task.await.unwrap();
    assert_eq!(observation.acknowledgement.unwrap().value.code, "000604");
    let PollOutcome::Terminal(e) = observation.outcome.unwrap() else {
        panic!("not terminal")
    };
    assert!(e.is_cancelled());
    assert_eq!(
        e.document
            .as_ref()
            .unwrap()
            .statement_handle
            .as_ref()
            .unwrap()
            .as_str(),
        B
    );
    let r = &s.requests()[1];
    assert_eq!(r.method, "POST");
    assert_eq!(
        r.target,
        format!("/api/v2/statements/{B}/cancel?requestId={CANCEL_ID}")
    );
    s.shutdown().await;
}

#[tokio::test]
async fn cancellation_loses_completion_race_and_reports_success_instead() {
    let cancelled = Arc::new(AtomicUsize::new(0));
    let flag = cancelled.clone();
    let s = TestServer::start(move |r| {
        if r.path().ends_with("/cancel") {
            flag.fetch_add(1, Ordering::SeqCst);
            return json(422, NOT_FOUND);
        }
        if r.method == "POST" {
            json(202, PENDING_B)
        } else {
            assert_eq!(flag.load(Ordering::SeqCst), 1);
            json(200, COMPLETE_B)
        }
    })
    .await;
    let query = client(&s)
        .submit(context(), Statement::new("select synthetic_race"))
        .await
        .unwrap()
        .query;
    let observation = query
        .cancel_and_wait(id(CANCEL_ID), policy(3), &LocalStop::default())
        .await;
    let Err(Error::Server(e)) = observation.acknowledgement else {
        panic!("expected rejected cancel")
    };
    assert_eq!(e.document.unwrap().code, "000709");
    let first = complete(observation.outcome.unwrap());
    assert_eq!(first.plan().query().handle().as_str(), B);
    assert_eq!(first.response().value.data.len(), 2);
    assert_eq!(cancelled.load(Ordering::SeqCst), 1);
    s.shutdown().await;
}

#[tokio::test]
async fn cancellation_acknowledgement_with_unresolved_poll_is_not_terminal() {
    let s = TestServer::start(|r| {
        if r.path().ends_with("/cancel") {
            json(200, CANCELLED)
        } else {
            json(202, PENDING_B)
        }
    })
    .await;
    let query = client(&s)
        .submit(context(), statement())
        .await
        .unwrap()
        .query;
    let observation = query
        .cancel_and_wait(id(CANCEL_ID), policy(1), &LocalStop::default())
        .await;
    assert!(observation.acknowledgement.is_ok());
    assert!(matches!(
        observation.outcome.unwrap(),
        PollOutcome::Unresolved {
            reason: PollEnd::Attempts,
            ..
        }
    ));
    s.shutdown().await;
}

#[tokio::test]
async fn terminal_failure_and_timeout_are_distinct_from_success_and_transient_errors() {
    for (status, body, cancelled) in [
        (
            422,
            include_bytes!("../fixtures/failed-a.json").as_slice(),
            false,
        ),
        (
            408,
            include_bytes!("../fixtures/timeout-a.json").as_slice(),
            true,
        ),
    ] {
        let script = Script::new([json(202, PENDING_A), json(status, body)]);
        let s = TestServer::start(move |_| script.next()).await;
        let result = client(&s)
            .submit(context(), statement())
            .await
            .unwrap()
            .wait(policy(2), &LocalStop::default())
            .await
            .unwrap();
        let PollOutcome::Terminal(e) = result else {
            panic!("not terminal")
        };
        assert_eq!(e.is_cancelled(), cancelled);
        s.shutdown().await;
    }
    let script = Script::new([
        json(202, PENDING_A),
        json(429, include_bytes!("../fixtures/rate-limit.json")),
    ]);
    let s = TestServer::start(move |_| script.next()).await;
    assert!(matches!(
        client(&s)
            .submit(context(), statement())
            .await
            .unwrap()
            .wait(policy(2), &LocalStop::default())
            .await,
        Err(Error::Server(_))
    ));
    assert_eq!(s.requests().len(), 2); // no implicit retry
    s.shutdown().await;
}

#[tokio::test]
async fn dropped_next_future_preserves_owned_partition_request_for_resume() {
    let gate = Gate::new();
    let hold = gate.clone();
    let s = TestServer::start(move |r| {
        if r.method == "POST" {
            json(200, TWO)
        } else {
            gzip(GZIP1).after(hold.clone())
        }
    })
    .await;
    let mut parts = first(
        client(&s)
            .submit(context(), statement())
            .await
            .unwrap()
            .state,
    )
    .bounded_partitions(nz(2), nz(1));
    parts.next().await.unwrap().unwrap();
    tokio::time::pause();
    let task = tokio::spawn(async move {
        assert!(
            tokio::time::timeout(Duration::from_secs(1), parts.next())
                .await
                .is_err()
        );
        parts
    });
    spin_until(|| s.requests().len() == 2).await;
    tokio::time::advance(Duration::from_millis(1001)).await;
    spin_until(|| task.is_finished()).await;
    let mut parts = task.await.unwrap();
    gate.open();
    let resumed = tokio::spawn(async move {
        assert_eq!(parts.next().await.unwrap().unwrap().index, 1);
        assert!(parts.next().await.is_none());
        parts.finish()
    });
    spin_until(|| resumed.is_finished()).await;
    assert!(resumed.await.unwrap().is_complete());
    assert_eq!(s.requests().len(), 2); // same in-flight request, no refetch
    s.shutdown().await;
}
