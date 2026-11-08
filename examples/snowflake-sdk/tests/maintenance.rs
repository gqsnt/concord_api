//! Maintenance perturbation: pending, a second partition, and a gated completion/cancel race.
mod common;
use common::*;
use concord_example_testserver::{Gate, Script, TestServer};
use concord_sdk_snowflake::*;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

const SUBMIT_PENDING: &[u8] = include_bytes!("../fixtures/maintenance-pending-submit.json");
const POLL_PENDING: &[u8] = include_bytes!("../fixtures/maintenance-pending-poll.json");
const AFTER_CANCEL: &[u8] = include_bytes!("../fixtures/maintenance-pending-after-cancel.json");
const RESULT: &[u8] = include_bytes!("../fixtures/maintenance-complete.json");
const LINKS: &str = include_str!("../fixtures/maintenance-partitions.link");

fn status_url(bytes: &[u8]) -> String {
    serde_json::from_slice::<serde_json::Value>(bytes).unwrap()["statementStatusUrl"]
        .as_str()
        .unwrap()
        .to_owned()
}
fn policy(attempts: usize) -> PollPolicy {
    PollPolicy {
        max_attempts: nz(attempts),
        interval: Duration::from_secs(2),
        max_elapsed: Duration::from_secs(20),
    }
}

#[tokio::test]
async fn pending_second_partition_and_both_cancel_replies_report_observed_completion() {
    for cancel_status in [200, 422] {
        let gate = Gate::new();
        let hold = gate.clone();
        let statuses = Arc::new(AtomicUsize::new(0));
        let seen = statuses.clone();
        let cancel_body = mutate(
            if cancel_status == 200 {
                CANCELLED
            } else {
                NOT_FOUND
            },
            |v| {
                v["statementStatusUrl"] = status_url(POLL_PENDING).into();
            },
        );
        let s = TestServer::start(move |r| {
            if r.path().ends_with("/cancel") {
                return json(cancel_status, &cancel_body).after(hold.clone());
            }
            if r.method == "POST" {
                return json(202, SUBMIT_PENDING);
            }
            if !r.query_all("partition").is_empty() {
                return gzip(GZIP1);
            }
            match seen.fetch_add(1, Ordering::SeqCst) {
                0 => json(202, POLL_PENDING),
                1 | 3 => json(200, RESULT).header("link", LINKS),
                2 => json(202, AFTER_CANCEL),
                _ => panic!("unexpected status request"),
            }
        })
        .await;
        let submission = client(&s).submit(context(), statement()).await.unwrap();
        let query = submission.query.clone();
        let bounded = submission
            .wait(policy(1), &LocalStop::default())
            .await
            .unwrap();
        let PollOutcome::Unresolved {
            reason: PollEnd::Attempts,
            attempts: 1,
            last_pending: Some(last),
        } = bounded
        else {
            panic!("the inserted pending poll must remain unresolved")
        };
        assert_eq!(last.value.statement_handle.as_ref(), Some(query.handle()));
        assert_eq!(
            last.value.statement_status_url.as_deref(),
            Some(status_url(POLL_PENDING).as_str())
        );
        assert_eq!(query.context(), &context());
        assert_eq!(query.handle().as_str(), B);

        let cancelling = query.clone();
        let cancel_task = tokio::spawn(async move {
            cancelling
                .cancel_and_wait(id(CANCEL_ID), policy(3), &LocalStop::default())
                .await
        });
        spin_until(|| s.requests().len() == 3).await;
        // Completion is delivered while the cancellation response is still held on the wire.
        let concurrent = first(query.status().await.unwrap());
        assert_eq!(concurrent.plan().query().handle().as_str(), B);
        assert!(!cancel_task.is_finished());
        tokio::time::pause();
        gate.open();
        spin_until(|| s.requests().len() == 5).await;
        for _ in 0..2000 {
            tokio::task::yield_now().await;
        }
        assert!(!cancel_task.is_finished());
        tokio::time::advance(Duration::from_millis(2001)).await;
        spin_until(|| cancel_task.is_finished()).await;
        let observation = cancel_task.await.unwrap();
        match observation.acknowledgement {
            Ok(reply) => {
                assert_eq!(cancel_status, 200);
                assert_eq!(reply.value.code, "000604");
                assert_eq!(reply.value.statement_handle.as_ref(), Some(query.handle()));
            }
            Err(Error::Server(error)) => {
                assert_eq!(cancel_status, 422);
                assert_eq!(error.document.as_ref().unwrap().code, "000709");
                assert_eq!(
                    error.document.as_ref().unwrap().statement_handle.as_ref(),
                    Some(query.handle())
                );
                assert_eq!(
                    error.meta.operation().unwrap().name(),
                    "SqlApi.statement.cancel"
                );
            }
            other => panic!("unexpected cancel result: {other:?}"),
        }
        let result = complete(observation.outcome.unwrap());
        assert_eq!(result.plan().query().handle(), query.handle());
        assert_eq!(result.plan().query().context(), query.context());
        assert_eq!(
            result
                .response()
                .value
                .result_set_meta_data
                .partition_info
                .len(),
            2
        );
        let mut parts = result.bounded_partitions(nz(1), nz(1));
        let mut rows = Vec::new();
        let mut indexes = Vec::new();
        let fetch = tokio::spawn(async move {
            while let Some(part) = parts.next().await {
                let part = part.unwrap();
                assert_eq!(part.plan().query().handle().as_str(), B);
                assert_eq!(part.plan().query().context(), &context());
                indexes.push(part.index);
                rows.extend(part.rows);
            }
            (indexes, rows, parts.finish())
        });
        spin_until(|| fetch.is_finished()).await;
        let (indexes, rows, report) = fetch.await.unwrap();
        assert_eq!(indexes, [0, 1]);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[2][1].as_deref(), Some("βeta"));
        assert!(report.is_complete());
        assert_eq!(report.missing().count(), 0);
        assert_eq!(report.remaining.progress.admitted, 1);
        let requests = s.requests();
        assert_eq!(requests.len(), 7);
        assert_eq!(requests[1].target, status_url(SUBMIT_PENDING));
        assert_eq!(
            requests[2].target,
            format!("/api/v2/statements/{B}/cancel?requestId={CANCEL_ID}")
        );
        assert_eq!(requests[3].target, status_url(POLL_PENDING));
        assert_eq!(requests[4].target, status_url(POLL_PENDING));
        assert_eq!(requests[5].target, status_url(AFTER_CANCEL));
        assert_eq!(
            requests[6].target,
            LINKS.split('<').nth(2).unwrap().split('>').next().unwrap()
        );
        assert_eq!(requests[6].header("accept-encoding"), Some("gzip"));
        assert_eq!(statuses.load(Ordering::SeqCst), 4);
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.path().ends_with("/cancel"))
                .count(),
            1
        );
        s.shutdown().await;
        tokio::time::resume();
    }
}

#[tokio::test]
async fn race_observation_rejects_other_handles_after_an_inserted_pending_poll() {
    for (status, body) in [
        (202, PENDING_A),
        (200, ONE),
        (422, include_bytes!("../fixtures/failed-a.json").as_slice()),
    ] {
        let script = Script::new([
            json(202, SUBMIT_PENDING),
            json(202, POLL_PENDING),
            json(200, CANCELLED),
            json(status, body),
        ]);
        let s = TestServer::start(move |_| script.next()).await;
        let submitted = client(&s).submit(context(), statement()).await.unwrap();
        let query = submitted.query.clone();
        assert!(matches!(
            submitted
                .wait(policy(1), &LocalStop::default())
                .await
                .unwrap(),
            PollOutcome::Unresolved { .. }
        ));
        let observed = query
            .cancel_and_wait(id(CANCEL_ID), policy(1), &LocalStop::default())
            .await;
        assert!(observed.acknowledgement.is_ok());
        let Err(Error::Protocol {
            kind: ProtocolError::HandleMismatch,
            meta: Some(meta),
        }) = observed.outcome
        else {
            panic!("a different statement must never determine this race")
        };
        assert_eq!(meta.status().as_u16(), status);
        assert_eq!(meta.operation().unwrap().name(), "SqlApi.statement.status");
        assert_eq!(query.handle().as_str(), B);
        assert_eq!(s.requests().len(), 4);
        s.shutdown().await;
    }
}

#[tokio::test]
async fn mismatched_cancel_acknowledgement_does_not_mask_correlated_completion() {
    let script = Script::new([
        json(202, SUBMIT_PENDING),
        json(200, &mutate(CANCELLED, |v| v["statementHandle"] = A.into())),
        json(200, RESULT).header("link", LINKS),
    ]);
    let s = TestServer::start(move |_| script.next()).await;
    let query = client(&s)
        .submit(context(), statement())
        .await
        .unwrap()
        .query;
    let observed = query
        .cancel_and_wait(id(CANCEL_ID), policy(1), &LocalStop::default())
        .await;
    assert!(matches!(
        observed.acknowledgement,
        Err(Error::Protocol {
            kind: ProtocolError::HandleMismatch,
            ..
        })
    ));
    assert_eq!(
        complete(observed.outcome.unwrap()).plan().query().handle(),
        query.handle()
    );
    assert_eq!(s.requests()[2].target, status_url(SUBMIT_PENDING));
    assert_eq!(s.requests().len(), 3);
    s.shutdown().await;
}

#[tokio::test]
async fn invalid_status_links_are_explicit_protocol_errors_before_another_request() {
    for target in [
        "https://different-account.invalid/api/v2/statements/anything".to_owned(),
        format!("/api/v2/statements/{A}"),
        format!("/api/v2/statements/{B}/cancel"),
        format!("/api/v2/statements/{B}?partition=1"),
        format!("/api/v2/statements/{B}?partition=0&partition=0"),
        format!("/api/v2/statements/{B}#fragment"),
        String::new(),
    ] {
        let body = mutate(SUBMIT_PENDING, |v| v["statementStatusUrl"] = target.into());
        let s = TestServer::start(move |_| json(202, &body)).await;
        let error = client(&s).submit(context(), statement()).await.unwrap_err();
        assert!(matches!(
            error,
            Error::Protocol {
                kind: ProtocolError::InvalidLink,
                ..
            }
        ));
        assert_eq!(error.response_meta().unwrap().status().as_u16(), 202);
        assert_eq!(s.requests().len(), 1);
        s.shutdown().await;
    }
}

#[tokio::test]
async fn malformed_or_uncorrelated_partition_links_are_not_an_empty_success() {
    let headers = [
        "malformed Link".to_owned(),
        format!("<https://different-account.invalid/api/v2/statements/{B}?partition=1>; rel=next"),
        format!("</api/v2/statements/{A}?partition=1>; rel=next"),
        format!("</api/v2/statements/{B}?partition=2>; rel=next"),
        format!("</api/v2/statements/{B}?partition=1&partition=1>; rel=next"),
        format!(
            "</api/v2/statements/{B}?partition=1>; rel=next, </api/v2/statements/{B}?partition=1&opaque=other>; rel=next"
        ),
        format!("</api/v2/statements/{B}?partition=1>; rel=next; title=\"unterminated"),
        format!("</api/v2/statements/{B}?partition=1>; rel=next; anchor=\"/different-context\""),
    ];
    // One server and one client for every case: each submission is answered with the next
    // header over the same kept-alive connection, so no case depends on binding, connecting to
    // and tearing down a server of its own (eight in a row, alongside the other tests' servers).
    let script = Script::new(
        headers
            .iter()
            .map(|header| json(200, RESULT).header("link", header)),
    );
    let s = TestServer::start(move |_| script.next()).await;
    let snowflake = client(&s);
    for case in 0..headers.len() {
        let error = snowflake.submit(context(), statement()).await.unwrap_err();
        assert!(matches!(
            error,
            Error::Protocol {
                kind: ProtocolError::InvalidLink,
                ..
            }
        ));
        assert_eq!(error.response_meta().unwrap().status().as_u16(), 200);
        // Exactly one request for this case: the submission, nothing after it.
        assert_eq!(s.requests().len(), case + 1);
    }
    s.shutdown().await;
}

#[tokio::test]
async fn later_partition_links_update_shared_plans_without_rewriting_opaque_queries() {
    let next = format!("/api/v2/statements/{A}?opaque=one%2f+%2B&partition=1");
    let last = format!("/api/v2/statements/{A}?partition=2&opaque=original");
    let updated = format!("/api/v2/statements/{A}?opaque=two%2F%2b+&partition=2&opaque=keep");
    let first_links = format!("<{next}>; rel=next, <{last}>; rel=last");
    let second_links = format!("<{updated}>; rel=next");
    let s =
        TestServer::start(
            move |r| match r.query_all("partition").first().map(String::as_str) {
                None => json(200, THREE).header("link", &first_links),
                Some("1") => gzip(GZIP1).header("link", &second_links),
                Some("2") => gzip(GZIP2),
                _ => panic!("unexpected partition"),
            },
        )
        .await;
    let result = first(
        client(&s)
            .submit(context(), statement())
            .await
            .unwrap()
            .state,
    );
    let plan = result.plan();
    let shared = plan.clone();
    assert_eq!(plan.fetch(1).await.unwrap().rows.len(), 1);
    assert_eq!(shared.fetch(2).await.unwrap().rows.len(), 2);
    assert_eq!(s.requests()[1].target, next);
    assert_eq!(s.requests()[2].target, updated);
    let debug = format!("{shared:?}");
    assert!(!debug.contains("opaque="));
    assert_eq!(s.requests().len(), 3);
    s.shutdown().await;
}

#[tokio::test]
async fn absolute_status_link_and_error_resume_link_keep_the_provider_error_typed() {
    let statuses = Arc::new(AtomicUsize::new(0));
    let seen = statuses.clone();
    let resume = "?opaque=resume%2f%2B+&requestId=00000000-0000-4000-8000-000000000307";
    let failure = mutate(NOT_FOUND, |v| v["statementStatusUrl"] = resume.into());
    let expected_body = failure.clone();
    let s = TestServer::start(move |r| {
        if r.method == "POST" {
            let body = mutate(SUBMIT_PENDING, |v| {
                v["statementStatusUrl"] = format!(
                    "http://{}{}",
                    r.header("host").unwrap(),
                    status_url(SUBMIT_PENDING)
                )
                .into();
            });
            return json(202, &body);
        }
        if seen.fetch_add(1, Ordering::SeqCst) == 0 {
            json(503, &failure)
        } else {
            json(200, RESULT)
        }
    })
    .await;
    let query = client(&s)
        .submit(context(), statement())
        .await
        .unwrap()
        .query;
    let Err(Error::Server(error)) = query.status().await else {
        panic!("the provider failure must remain typed")
    };
    assert_eq!(error.meta.status().as_u16(), 503);
    assert_eq!(error.document.as_ref().unwrap().code, "000709");
    assert_eq!(error.body.as_ref(), expected_body);
    assert_eq!(
        first(query.clone().status().await.unwrap())
            .plan()
            .query()
            .handle(),
        query.handle()
    );
    assert_eq!(s.requests()[1].target, status_url(SUBMIT_PENDING));
    assert_eq!(
        s.requests()[2].target,
        format!("/api/v2/statements/{B}{resume}")
    );
    assert_eq!(s.requests().len(), 3);
    s.shutdown().await;
}

#[tokio::test]
async fn invalid_link_on_a_later_partition_is_a_keyed_failure_with_missing_work() {
    let bad_link = format!("</api/v2/statements/{B}?partition=2>; rel=next");
    let s = TestServer::start(move |r| {
        if r.method == "POST" {
            json(200, THREE)
        } else {
            gzip(GZIP1).header("link", &bad_link)
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
    assert_eq!(parts.next().await.unwrap().unwrap().index, 0);
    let failure = parts.next().await.unwrap().unwrap_err();
    assert_eq!(failure.index, 1);
    assert!(matches!(
        failure.error,
        Error::Protocol {
            kind: ProtocolError::InvalidLink,
            ..
        }
    ));
    assert!(parts.next().await.is_none());
    let report = parts.finish();
    assert!(!report.is_complete());
    assert_eq!(report.remaining.outcome, pagination::Outcome::Failed);
    assert_eq!(report.missing().collect::<Vec<_>>(), [1, 2]);
    assert_eq!(s.requests().len(), 2);
    s.shutdown().await;
}
