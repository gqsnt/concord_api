//! Complete guide recipes, exercised with scripted provider responses.
#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod docker {
    include!("../fixtures/recipes/docker_frames.rs");
}
mod aws {
    include!("../fixtures/recipes/aws_frames.rs");
}
mod multipart_response {
    include!("../fixtures/recipes/multipart_response.rs");
}
mod batch {
    include!("../fixtures/recipes/multipart_batch.rs");
}
mod hosts {
    include!("../fixtures/recipes/several_hosts.rs");
}

use concord::{
    bytes::Bytes, read::StreamError, transport::TransportError, Base, ByteStream, Retry,
};
use concord_testkit::Script;
use futures_util::TryStreamExt;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

fn poll<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(
        futures_util::task::noop_waker_ref(),
    ))
}
fn one_send(script: &Script) {
    assert_eq!(script.requests().len(), 1);
    assert_eq!(script.requests()[0].info().number(), 1);
}
async fn body(script: &Script) -> ByteStream {
    docker::Docker::new("http://docker.test".parse::<Base>().unwrap())
        .with_transport(script.clone())
        .logs("abc")
        .with_retry(Retry::standard())
        .await
        .unwrap()
}
fn docker_wire(channel: u8, data: &[u8]) -> Vec<u8> {
    let mut wire = vec![channel, 0, 0, 0];
    wire.extend_from_slice(&(data.len() as u32).to_be_bytes());
    wire.extend_from_slice(data);
    wire
}
fn aws_wire(kind: &str, payload: &[u8]) -> Vec<u8> {
    let mut headers = Vec::new();
    for (name, value) in [(":message-type", "event"), (":event-type", kind)] {
        headers.push(name.len() as u8);
        headers.extend_from_slice(name.as_bytes());
        headers.push(7);
        headers.extend_from_slice(&(value.len() as u16).to_be_bytes());
        headers.extend_from_slice(value.as_bytes());
    }
    let mut wire = Vec::new();
    wire.extend_from_slice(&((16 + headers.len() + payload.len()) as u32).to_be_bytes());
    wire.extend_from_slice(&(headers.len() as u32).to_be_bytes());
    wire.extend_from_slice(&crc32fast::hash(&wire).to_be_bytes());
    wire.extend_from_slice(&headers);
    wire.extend_from_slice(payload);
    wire.extend_from_slice(&crc32fast::hash(&wire).to_be_bytes());
    wire
}
fn batch_wire(boundary: &str, replies: &[(&str, u16, &str)]) -> Vec<u8> {
    let mut wire = String::new();
    for (id, status, payload) in replies {
        wire.push_str(&format!(
            "--{boundary}\r\nContent-Type: application/http\r\n"
        ));
        if !id.is_empty() {
            wire.push_str(&format!("Content-ID: {id}\r\n"));
        }
        wire.push_str(&format!("\r\nHTTP/1.1 {status} Reply\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{payload}\r\n", payload.len()));
    }
    wire.push_str(&format!("--{boundary}--\r\n"));
    wire.into_bytes()
}

#[test]
fn c1_20_stream_recipe_blocks_match_the_guide_word_for_word() {
    let recipes = include_str!("../../../docs/guide/recipes.md");
    for code in [
        include_str!("../fixtures/recipes/docker_frames.rs"),
        include_str!("../fixtures/recipes/aws_frames.rs"),
        include_str!("../fixtures/recipes/multipart_response.rs"),
        include_str!("../fixtures/recipes/multipart_batch.rs"),
    ] {
        assert!(recipes.contains(&format!("```rust\n{}\n```", code.trim())));
    }
    assert!(recipes.contains(&format!(
        "```rust\n{}\n```",
        include_str!("../fixtures/recipes/several_hosts.rs").trim()
    )));
    assert!(
        include_str!("../../../docs/guide/bodies.md").contains(&format!(
            "```rust\n{}\n```",
            include_str!("../fixtures/recipes/upload_stream.rs").trim()
        ))
    );
}

#[tokio::test(start_paused = true)]
async fn c5_10_docker_frames_preserve_channels_binary_bytes_and_every_cut() {
    let wire = [docker_wire(1, b"out\0\xff"), docker_wire(2, b"err\n")].concat();
    for split in 0..=wire.len() {
        let script = Script::new();
        script
            .answer(200)
            .body(wire[..split].to_vec())
            .body(wire[split..].to_vec());
        let mut frames = docker::docker_frames(body(&script).await);
        assert_eq!(
            docker::docker_log(frames.try_next().await.unwrap().unwrap().freeze()).unwrap(),
            (1, Bytes::from_static(b"out\0\xff"))
        );
        assert_eq!(
            docker::docker_log(frames.try_next().await.unwrap().unwrap().freeze())
                .unwrap()
                .0,
            2
        );
        assert!(frames.try_next().await.unwrap().is_none());
        assert!(script.requests()[0]
            .url()
            .as_str()
            .ends_with("/containers/abc/logs?stdout=true&stderr=true&follow=true"));
        one_send(&script);
    }
    let script = Script::new();
    script.answer(200).body(wire);
    let mut logs = Vec::new();
    docker::read_logs(body(&script).await, &mut logs)
        .await
        .unwrap();
    assert_eq!(
        logs,
        [
            (1, Bytes::from_static(b"out\0\xff")),
            (2, Bytes::from_static(b"err\n"))
        ]
    );
    one_send(&script);
}

#[tokio::test(start_paused = true)]
async fn c5_10_aws_frames_keep_the_whole_frame_validate_crcs_and_require_end() {
    let wire = [aws_wire("Records", b"row\n"), aws_wire("End", b"")].concat();
    for split in 0..=wire.len() {
        let script = Script::new();
        script
            .answer(200)
            .header("content-type", "application/vnd.amazon.eventstream")
            .body(wire[..split].to_vec())
            .body(wire[split..].to_vec());
        let s3 = aws::S3::new(
            "https://bucket.s3.us-east-1.amazonaws.com"
                .parse::<Base>()
                .unwrap(),
            "AWS4-HMAC-SHA256 test-signature",
        )
        .with_transport(script.clone());
        let query = concord::Body::new("<SelectObjectContentRequest><Expression>SELECT * FROM S3Object</Expression><ExpressionType>SQL</ExpressionType><InputSerialization><CSV/></InputSerialization><OutputSerialization><CSV/></OutputSerialization></SelectObjectContentRequest>", "application/xml");
        let mut records = Vec::new();
        aws::read_select(s3.select("rows.csv", query).await.unwrap(), &mut records)
            .await
            .unwrap();
        assert_eq!(records, [Bytes::from_static(b"row\n")]);
        assert!(script.requests()[0].headers()["authorization"].is_sensitive());
        one_send(&script);
    }
    for index in [8, wire.len() - 1] {
        let mut damaged = wire.clone();
        damaged[index] ^= 1;
        let script = Script::new();
        script.answer(200).body(damaged);
        assert_eq!(
            aws::read_select(body(&script).await, &mut Vec::new())
                .await
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::InvalidData
        );
        one_send(&script);
    }
    let script = Script::new();
    script.answer(200).body(aws_wire("Records", b"kept"));
    let mut records = Vec::new();
    assert_eq!(
        aws::read_select(body(&script).await, &mut records)
            .await
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::UnexpectedEof
    );
    assert_eq!(records, [Bytes::from_static(b"kept")]);
    one_send(&script);
}

#[tokio::test(start_paused = true)]
async fn c5_10_binary_frame_limits_are_checked_before_waiting_for_payload() {
    for aws in [false, true] {
        let script = Script::new();
        let prefix = if aws {
            ((concord::MIB + 1) as u32).to_be_bytes().to_vec()
        } else {
            vec![1, 0, 0, 0, 0, 1, 0, 1]
        };
        script.answer(200).body(prefix).stall();
        let start = tokio::time::Instant::now();
        let error = if aws {
            aws::aws_frames(body(&script).await)
                .try_next()
                .await
                .unwrap_err()
        } else {
            docker::docker_frames(body(&script).await)
                .try_next()
                .await
                .unwrap_err()
        };
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(start.elapsed(), Duration::ZERO);
        one_send(&script);
    }
    let script = Script::new();
    script
        .answer(200)
        .body(docker_wire(1, &vec![b'x'; 64 * concord::KIB]));
    assert_eq!(
        docker::docker_frames(body(&script).await)
            .try_next()
            .await
            .unwrap()
            .unwrap()
            .len(),
        64 * concord::KIB + 8
    );
    one_send(&script);
}

#[tokio::test(start_paused = true)]
async fn c5_10_frames_refuse_truncated_prefixes_payloads_and_invalid_channels() {
    for wire in [vec![1, 0, 0], vec![1, 0, 0, 0, 0, 0, 0, 4, b'x']] {
        let script = Script::new();
        script.answer(200).body(wire);
        let mut frames = docker::docker_frames(body(&script).await);
        assert!(frames.try_next().await.is_err());
        assert!(frames.try_next().await.unwrap().is_none());
        one_send(&script);
    }
    for wire in [docker_wire(4, b"x"), vec![1, 9, 0, 0, 0, 0, 0, 0]] {
        let script = Script::new();
        script.answer(200).body(wire);
        let frame = docker::docker_frames(body(&script).await)
            .try_next()
            .await
            .unwrap()
            .unwrap();
        assert!(docker::docker_log(frame.freeze()).is_err());
        one_send(&script);
    }
}

#[tokio::test(start_paused = true)]
async fn c5_10_frame_pull_cancellation_keeps_buffered_prefix_and_body_error_context() {
    let wire = docker_wire(2, b"hello");
    let script = Script::new();
    script
        .answer(200)
        .header("x-request-id", "one")
        .body(wire[..9].to_vec())
        .pause(Duration::from_secs(2))
        .body(wire[9..].to_vec())
        .cut();
    let mut frames = docker::docker_frames(body(&script).await);
    let mut pull = Box::pin(frames.try_next());
    assert!(poll(pull.as_mut()).is_pending());
    drop(pull);
    assert_eq!(
        docker::docker_log(frames.try_next().await.unwrap().unwrap().freeze())
            .unwrap()
            .1,
        "hello"
    );
    let error = frames.try_next().await.unwrap_err();
    let original = error
        .get_ref()
        .unwrap()
        .downcast_ref::<StreamError>()
        .unwrap();
    assert_eq!(original.status().unwrap().as_u16(), 200);
    assert_eq!(original.headers().unwrap().get("x-request-id"), Some("one"));
    assert_eq!(original.tries(), 1);
    assert_eq!(
        frames.get_ref().get_ref().headers().get("x-request-id"),
        Some("one")
    );
    assert!(frames.try_next().await.unwrap().is_none());
    one_send(&script);
}

#[tokio::test(start_paused = true)]
async fn c7_6_codec_body_clocks_survive_and_idle_time_includes_cancelled_pulls() {
    for total in [false, true] {
        let script = Script::new();
        script
            .answer(200)
            .body(docker_wire(1, b"x")[..8].to_vec())
            .stall();
        let client = docker::Docker::new("http://docker.test".parse::<Base>().unwrap())
            .with_transport(script.clone());
        let call = client.logs("abc").with_idle_timeout(Duration::from_secs(2));
        let call = if total {
            call.with_timeout(Duration::from_secs(1))
        } else {
            call
        };
        let start = tokio::time::Instant::now();
        let mut frames = docker::docker_frames(call.await.unwrap());
        let mut pull = Box::pin(frames.try_next());
        assert!(poll(pull.as_mut()).is_pending());
        drop(pull);
        tokio::time::advance(Duration::from_secs(3)).await;
        let error = frames.try_next().await.unwrap_err();
        assert!(error
            .get_ref()
            .unwrap()
            .downcast_ref::<StreamError>()
            .unwrap()
            .is_timeout());
        assert_eq!(start.elapsed(), Duration::from_secs(3));
        one_send(&script);
    }
}

#[tokio::test(start_paused = true)]
async fn c5_10_gmail_batch_correlates_out_of_order_replies_at_every_cut() {
    let wire = batch_wire(
        "reply_d",
        &[
            ("<response-two>", 404, r#"{"error":{"code":404}}"#),
            ("<response-one>", 200, r#"{"id":"one"}"#),
        ],
    );
    for split in 0..=wire.len() {
        let script = Script::new();
        script
            .answer(200)
            .header("content-type", "multipart/mixed; boundary=\"reply_d\"")
            .body(wire[..split].to_vec())
            .body(wire[split..].to_vec());
        let api = batch::Gmail::new("Bearer token").with_transport(script.clone());
        let mut replies = Vec::new();
        batch::gmail_batch(&api, &mut replies).await.unwrap();
        assert_eq!(
            replies[0]
                .0
                .as_deref()
                .unwrap()
                .trim_matches(['<', '>'])
                .strip_prefix("response-"),
            Some("two")
        );
        assert_eq!(replies[0].1.status().as_u16(), 404);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(replies[1].1.value()).unwrap()["id"],
            "one"
        );
        let requests = script.requests();
        assert!(requests[0].headers()["authorization"].is_sensitive());
        assert!(std::str::from_utf8(requests[0].body().unwrap())
            .unwrap()
            .contains("GET /gmail/v1/users/me/messages/one HTTP/1.1"));
        assert_eq!(
            requests[0].url().as_str(),
            "https://gmail.googleapis.com/batch/gmail/v1"
        );
        one_send(&script);
    }
}

#[tokio::test(start_paused = true)]
async fn c5_10_odata_batch_keeps_order_and_inner_failures_with_one_outer_post() {
    let wire = batch_wire(
        "b",
        &[
            ("", 200, r#"{"value":[]}"#),
            ("", 403, r#"{"error":{"code":"denied"}}"#),
        ],
    );
    let script = Script::new();
    let mut answer = script
        .answer(200)
        .header("content-type", "multipart/mixed; boundary=b");
    for byte in wire {
        answer = answer.body(vec![byte]);
    }
    drop(answer);
    let api = batch::SharePoint::new(
        "https://tenant.sharepoint.com/sites/demo/"
            .parse::<Base>()
            .unwrap(),
        "Bearer token",
    )
    .with_transport(script.clone());
    let mut replies = Vec::new();
    batch::sharepoint_batch(&api, &mut replies).await.unwrap();
    assert_eq!(
        replies
            .iter()
            .map(|reply| reply.1.status().as_u16())
            .collect::<Vec<_>>(),
        [200, 403]
    );
    assert!(replies.iter().all(|reply| reply.0.is_none()));
    assert_eq!(
        script.requests()[0].url().as_str(),
        "https://tenant.sharepoint.com/sites/demo/_api/$batch"
    );
    one_send(&script);
}

#[tokio::test(start_paused = true)]
async fn c5_10_batch_preserves_completed_replies_after_later_mime_or_http_failure_and_drop() {
    for mode in 0..3 {
        let first = batch_wire("b", &[("one", 200, "{}")]);
        let first = &first[..first.len() - "--b--\r\n".len()];
        let script = Script::new();
        let answer = script
            .answer(200)
            .header("content-type", "multipart/mixed; boundary=b")
            .body([first, b"--b\r\nContent-Type: application/http\r\n\r\n"].concat());
        if mode == 0 {
            answer
                .pause(Duration::from_secs(1))
                .body("HTTP/1.1 200 OK\r\n\r\n")
                .cut();
        } else if mode == 1 {
            answer.body("invalid HTTP\r\n--b--\r\n");
        } else {
            answer.stall();
        }
        let mut replies = Vec::new();
        let mut read = Box::pin(batch::read_batch(body(&script).await, &mut replies, 4096));
        if mode == 2 {
            assert!(poll(read.as_mut()).is_pending());
            drop(read);
        } else {
            assert!(read.await.is_err());
        }
        assert_eq!(replies.len(), 1);
        assert_eq!(replies[0].1.status().as_u16(), 200);
        tokio::time::advance(Duration::from_secs(60)).await;
        one_send(&script);
    }
}

#[tokio::test(start_paused = true)]
async fn c5_10_multipart_related_binary_nested_parts_and_truncated_close() {
    let script = Script::new();
    script
        .answer(200)
        .header("content-type", "multipart/related; boundary=b")
        .body(b"--b\r\nContent-Type: application/octet-stream\r\n\r\n\0\xff\r\n--b--\r\n".to_vec());
    let mut parser = multipart_response::parts(body(&script).await, 4096).unwrap();
    assert_eq!(
        parser
            .next_field()
            .await
            .unwrap()
            .unwrap()
            .bytes()
            .await
            .unwrap(),
        b"\0\xff".as_slice()
    );
    assert!(parser.next_field().await.unwrap().is_none());
    one_send(&script);
    let inner = batch_wire("inner", &[("one", 201, "{}")]);
    let script = Script::new();
    script
        .answer(200)
        .header("content-type", "multipart/mixed; boundary=outer")
        .body(format!(
            "--outer\r\nContent-Type: multipart/mixed; boundary=inner\r\n\r\n{}\r\n--outer--\r\n",
            String::from_utf8(inner).unwrap()
        ));
    let mut outer = multipart_response::parts(body(&script).await, 4096).unwrap();
    let part = outer.next_field().await.unwrap().unwrap();
    let label = part.headers()["content-type"].to_str().unwrap().to_owned();
    let bytes = part.bytes().await.unwrap();
    let nested =
        concord::read::Incoming::new(concord::reply_with(200, &[("content-type", &label)], bytes))
            .stream();
    let mut replies = Vec::new();
    batch::read_batch(nested, &mut replies, 1024).await.unwrap();
    assert_eq!(replies[0].1.status().as_u16(), 201);
    one_send(&script);
    let script = Script::new();
    script
        .answer(200)
        .header("content-type", "multipart/mixed; boundary=b")
        .body("--b\r\nContent-Type: application/http\r\n\r\nunfinished");
    let mut parser = multipart_response::parts(body(&script).await, 1024).unwrap();
    assert!(parser
        .next_field()
        .await
        .unwrap()
        .unwrap()
        .bytes()
        .await
        .is_err());
    one_send(&script);
}

#[tokio::test(start_paused = true)]
async fn c5_10_multipart_whole_stream_and_field_limits_reject_excess_bytes() {
    for (whole, size) in [(32, 64), (1024 * 1024, 256 * concord::KIB + 1)] {
        let script = Script::new();
        script
            .answer(200)
            .header("content-type", "multipart/mixed; boundary=b")
            .body(format!(
                "--b\r\nContent-Type: application/octet-stream\r\n\r\n{}\r\n--b--\r\n",
                "x".repeat(size)
            ));
        let mut parser = multipart_response::parts(body(&script).await, whole).unwrap();
        let result = match parser.next_field().await {
            Ok(Some(part)) => part.bytes().await.map(|_| ()),
            Err(e) => Err(e),
            Ok(None) => panic!("missing part"),
        };
        assert!(matches!(
            result,
            Err(multer::Error::StreamSizeExceeded { .. } | multer::Error::FieldSizeExceeded { .. })
        ));
        one_send(&script);
    }
    for label in [
        "text/plain",
        "multipart/mixed",
        "multipart/mixed; boundary=\"\"",
    ] {
        let script = Script::new();
        script.answer(200).header("content-type", label);
        assert!(multipart_response::parts(body(&script).await, 1024).is_err());
        one_send(&script);
    }
    assert!(multer::parse_boundary("multipart/mixed; boundary=b").is_err());
}

#[tokio::test(start_paused = true)]
async fn c5_10_multer_refuses_a_boundary_prefix_line_that_is_not_a_delimiter() {
    let script = Script::new();
    script
        .answer(200)
        .header("content-type", "multipart/related; boundary=b")
        .body("--b\r\nContent-Type: application/octet-stream\r\n\r\ndata\r\n--bX\r\n--b--\r\n");
    let mut parser = multipart_response::parts(body(&script).await, 1024).unwrap();
    let part = parser.next_field().await.unwrap().unwrap();
    assert_eq!(part.bytes().await.unwrap(), "data");
    assert!(parser.next_field().await.is_err());
    one_send(&script);
}

#[tokio::test(start_paused = true)]
async fn c5_10_gmail_missing_or_duplicate_ids_fail_without_discarding_replies() {
    for ids in [
        vec!["<response-one>"],
        vec!["<response-one>", "<response-one>"],
    ] {
        let rows: Vec<_> = ids.iter().map(|id| (*id, 200, "{}")).collect();
        let script = Script::new();
        script
            .answer(200)
            .header("content-type", "multipart/mixed; boundary=b")
            .body(batch_wire("b", &rows));
        let api = batch::Gmail::new("Bearer token").with_transport(script.clone());
        let mut replies = Vec::new();
        assert!(batch::gmail_batch(&api, &mut replies).await.is_err());
        assert_eq!(replies.len(), ids.len());
        one_send(&script);
    }
}

concord::api! {
    client ElasticsearchWrites {
        base "https://es-one.test";
        POST add() { path "books/_doc"; body concord::json(serde_json::json!({"title":"rust"})); }
    }
}

#[tokio::test(start_paused = true)]
async fn c4_1_an_unmarked_elasticsearch_document_write_gets_one_send() {
    let script = Script::new();
    script.fail(TransportError::connect());
    assert!(ElasticsearchWrites::new()
        .with_transport(script.clone())
        .add()
        .with_retry(Retry::standard())
        .await
        .is_err());
    one_send(&script);
}

#[test]
fn c5_10_batch_http_keeps_headers_handles_bodyless_responses_and_refuses_transfer_coding() {
    use concord::http::Method;
    let reply = batch::http_response(
        Bytes::from_static(
            b"HTTP/1.1 200 OK\r\nX-Id: one\r\nX-Id: two\r\nContent-Length: 2\r\n\r\n{}",
        ),
        &Method::GET,
    )
    .unwrap();
    assert_eq!(reply.headers().as_map().get_all("x-id").iter().count(), 2);
    for (method, status) in [(Method::HEAD, 200), (Method::GET, 304)] {
        assert!(batch::http_response(
            Bytes::from(format!(
                "HTTP/1.1 {status} Reply\r\nContent-Length: 999\r\n\r\n"
            )),
            &method
        )
        .unwrap()
        .value()
        .is_empty());
    }
    for wire in [
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n",
        "HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nx",
    ] {
        assert!(batch::http_response(Bytes::from(wire.to_owned()), &Method::GET).is_err());
    }
}

fn host_clients(script: &Script) -> (Vec<hosts::Algolia>, Vec<hosts::Elasticsearch>) {
    let algolia = [
        ("https://app-dsn.algolia.net", "key-a"),
        ("https://app-1.algolianet.com", "key-b"),
    ]
    .map(|(origin, key)| {
        hosts::Algolia::new(origin.parse::<Base>().unwrap(), "app", key)
            .with_transport(script.clone())
            .with_retry(Retry::standard().tries(4))
    })
    .to_vec();
    let es = [
        ("https://es-one.test", "ApiKey one"),
        ("https://es-two.test", "ApiKey two"),
    ]
    .map(|(origin, key)| {
        hosts::Elasticsearch::new(origin.parse::<Base>().unwrap(), key)
            .with_transport(script.clone())
            .with_retry(Retry::standard().tries(4))
    })
    .to_vec();
    (algolia, es)
}

#[tokio::test(start_paused = true)]
async fn c4_1_failover_is_two_calls_with_separate_origins_credentials_and_no_engine_retry() {
    for provider in [0, 1] {
        let script = Script::new();
        script.fail(TransportError::connect());
        script.answer(200).body("{}");
        let (algolia, es) = host_clients(&script);
        if provider == 0 {
            hosts::algolia(&algolia, Duration::from_secs(5))
                .await
                .unwrap();
        } else {
            hosts::elasticsearch(&es, Duration::from_secs(5))
                .await
                .unwrap();
        }
        let sent = script.requests();
        assert_eq!(sent.len(), 2);
        assert_eq!(
            sent.iter().map(|r| r.info().number()).collect::<Vec<_>>(),
            [1, 1]
        );
        let header = if provider == 0 {
            "x-algolia-api-key"
        } else {
            "authorization"
        };
        assert_ne!(sent[0].headers()[header], sent[1].headers()[header]);
        assert!(sent.iter().all(|r| r.headers()[header].is_sensitive()));
        assert_ne!(sent[0].url(), sent[1].url());
    }
}

#[tokio::test(start_paused = true)]
async fn c7_6_failover_has_one_deadline_and_drop_starts_no_later_call() {
    for provider in [0, 1] {
        let script = Script::new();
        script
            .fail(TransportError::connect())
            .after(Duration::from_secs(2));
        script.answer(200).after(Duration::from_secs(2)).body("{}");
        let (algolia, es) = host_clients(&script);
        let result = if provider == 0 {
            hosts::algolia(&algolia, Duration::from_secs(3)).await
        } else {
            hosts::elasticsearch(&es, Duration::from_secs(3)).await
        };
        assert!(result.unwrap_err().is::<tokio::time::error::Elapsed>());
        assert_eq!(script.requests().len(), 2);
    }
    let script = Script::new();
    script.never();
    let (algolia, _) = host_clients(&script);
    let mut future = Box::pin(hosts::algolia(&algolia, Duration::from_secs(3)));
    assert!(poll(future.as_mut()).is_pending());
    drop(future);
    tokio::time::advance(Duration::from_secs(10)).await;
    one_send(&script);
}

#[tokio::test(start_paused = true)]
async fn c8_6_another_origin_is_refused_and_authentication_failures_do_not_fail_over() {
    use concord::page::Call;
    let script = Script::new();
    let (algolia, es) = host_clients(&script);
    assert!(algolia[0]
        .search()
        .at_url("https://app-1.algolianet.com/1/indexes/books/query")
        .await
        .is_err());
    assert!(es[0]
        .search()
        .at_url("https://es-two.test/books/_search")
        .await
        .is_err());
    assert_eq!(script.requests().len(), 0);
    for provider in [0, 1] {
        let script = Script::new();
        script.answer(401).body("{}");
        let (algolia, es) = host_clients(&script);
        let result = if provider == 0 {
            hosts::algolia(&algolia, Duration::from_secs(3)).await
        } else {
            hosts::elasticsearch(&es, Duration::from_secs(3)).await
        };
        assert!(result.is_err());
        one_send(&script);
    }
}

#[tokio::test(start_paused = true)]
async fn c4_1_independent_engine_policies_restart_attempt_numbers_on_another_host() {
    let script = Script::new();
    script.fail(TransportError::connect());
    script.fail(TransportError::connect());
    script.answer(200).body("{}");
    let (_, es) = host_clients(&script);
    assert!(es[0]
        .search()
        .with_retry(Retry::standard().tries(2))
        .await
        .is_err());
    es[1]
        .search()
        .with_retry(Retry::standard().tries(2))
        .await
        .unwrap();
    assert_eq!(
        script
            .requests()
            .iter()
            .map(|r| r.info().number())
            .collect::<Vec<_>>(),
        [1, 2, 1]
    );
}

concord::api! {
    client GitHubArrays(authorization: concord::Secret) {
        base "https://api.github.com";
        headers { "authorization" = authorization; "user-agent" = "concord-array-test"; }
        GET list() -> Vec<serde_json::Value> { path "user/repos"; read concord::Json.limit(128); }
    }
}
concord::api! {
    client DockerArrays {
        base "http://docker.test";
        GET list() -> Vec<serde_json::Value> { path "containers/json"; read concord::Json.limit(128); }
    }
}

#[tokio::test(start_paused = true)]
async fn c5_11_json_arrays_read_whole_at_every_cut_for_two_providers() {
    let wire = r#"[{"name":"é,]\"[","nested":[1,{"ok":true}]},null,1e3,"x",false]"#.as_bytes();
    for provider in [0, 1] {
        for split in 0..=wire.len() {
            let script = Script::new();
            script
                .answer(200)
                .body(wire[..split].to_vec())
                .body(wire[split..].to_vec());
            let result = if provider == 0 {
                GitHubArrays::new("Bearer token")
                    .with_transport(script.clone())
                    .list()
                    .await
            } else {
                DockerArrays::new()
                    .with_transport(script.clone())
                    .list()
                    .await
            };
            let values = result.unwrap();
            assert_eq!(values.len(), 5);
            assert_eq!(values[0]["name"], "é,]\"[");
            one_send(&script);
        }
    }
}

#[tokio::test(start_paused = true)]
async fn c5_11_json_arrays_enforce_the_whole_body_bound_and_reject_invalid_documents() {
    for wire in [
        "[1,]",
        "[1",
        "[{}]secret",
        "[1 2]",
        "{}",
        "[[}",
        "[\"unterminated",
        "[NaN]",
    ] {
        let script = Script::new();
        script.answer(200).body(wire);
        let error = DockerArrays::new()
            .with_transport(script.clone())
            .list()
            .await
            .unwrap_err();
        assert!(!format!("{error:?} {error}").contains("secret"));
        one_send(&script);
    }
    for wire in [
        format!("[\"{}\"]", "x".repeat(129)),
        format!("[{}0]", "0,".repeat(70)),
    ] {
        let script = Script::new();
        script.answer(200).body(wire);
        assert!(DockerArrays::new()
            .with_transport(script.clone())
            .list()
            .await
            .is_err());
        one_send(&script);
    }
    let script = Script::new();
    script.answer(200).body("[{\"Id\":\"partial\"},").stall();
    let api = DockerArrays::new().with_transport(script.clone());
    let mut call = Box::pin(api.list().send());
    assert!(poll(call.as_mut()).is_pending());
    drop(call);
    tokio::time::advance(Duration::from_secs(60)).await;
    one_send(&script);
}

#[tokio::test(start_paused = true)]
async fn c7_6_multipart_body_errors_keep_the_source_and_both_clocks_without_retry() {
    for total in [false, true] {
        let script = Script::new();
        script
            .answer(200)
            .header("content-type", "multipart/mixed; boundary=b")
            .header("x-request-id", "one")
            .body("--b\r\nContent-Type: application/http\r\n\r\nHTTP/1.1 200 OK\r\n\r\npartial")
            .stall();
        let api = docker::Docker::new("http://docker.test".parse::<Base>().unwrap())
            .with_transport(script.clone());
        let call = api
            .logs("abc")
            .with_idle_timeout(Duration::from_secs(2))
            .with_retry(Retry::standard());
        let call = if total {
            call.with_timeout(Duration::from_secs(1))
        } else {
            call
        };
        let start = tokio::time::Instant::now();
        let mut replies = Vec::new();
        let error = batch::read_batch(call.await.unwrap(), &mut replies, 4096)
            .await
            .unwrap_err();
        let mut source: &(dyn std::error::Error + 'static) = error.as_ref();
        loop {
            if let Some(stream) = source.downcast_ref::<StreamError>() {
                assert!(stream.is_timeout());
                assert_eq!(stream.headers().unwrap().get("x-request-id"), Some("one"));
                break;
            }
            source = source.source().unwrap();
        }
        assert_eq!(
            start.elapsed(),
            Duration::from_secs(if total { 1 } else { 2 })
        );
        one_send(&script);
    }
}

mod upload {
    use super::*;
    type BoxError = Box<dyn std::error::Error + Send + Sync>;
    concord::api! {
        client Files {
            base "https://files.test";
            PUT upload(name: String, file: concord::Body) { path "files/{name}"; body file; }
        }
    }
    include!("../fixtures/recipes/upload_stream.rs");

    #[tokio::test(start_paused = true)]
    async fn c3_2_upload_loop_reopens_the_file_and_sends_the_entire_body_twice() {
        let path =
            std::env::temp_dir().join(format!("concord-upload-{}.bin", uuid::Uuid::new_v4()));
        let contents: Vec<u8> = (0..131072).map(|n| (n % 256) as u8).collect();
        tokio::fs::write(&path, &contents).await.unwrap();
        let script = Script::new();
        script.fail(TransportError::connect()).read_upload();
        script.answer(204).read_upload();
        let files = Files::new()
            .with_transport(script.clone())
            .with_retry(Retry::standard().tries(5));
        upload(&files, "large.bin", &path).await.unwrap();
        tokio::fs::remove_file(path).await.unwrap();
        let sent = script.requests();
        assert_eq!(sent.len(), 2);
        assert_eq!(
            sent.iter().map(|r| r.info().number()).collect::<Vec<_>>(),
            [1, 1]
        );
        for request in sent {
            assert_eq!(request.method(), "PUT");
            assert_eq!(request.url().as_str(), "https://files.test/files/large.bin");
            assert_eq!(request.upload().bytes(), contents);
            assert_eq!(
                request.upload().state(),
                &concord_testkit::script::UploadState::Ended
            );
            assert!(request.upload().pieces() > 1);
        }
    }
}
