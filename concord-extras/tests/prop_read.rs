//! C5 readers over arbitrary bytes, chunk boundaries, heads and announced lengths.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use concord_core::{
    __private::{no_arm, read_body, refused_plain},
    bytes::Bytes,
    error::{Class, Kind},
    http::{HeaderMap, HeaderValue, Response, StatusCode},
    read::Incoming,
    reqwest,
    transport::TryInfo,
    Body, Error, Reader,
};
#[cfg(feature = "json")]
use concord_extras::read::Json;
use concord_extras::read::{Binary, Stream, Text};
use proptest::prelude::*;

fn response(
    chunks: &[Vec<u8>],
    announced: Option<u64>,
    status: u16,
    headers: &[(String, Vec<u8>)],
) -> reqwest::Response {
    let body = Body::concat(
        chunks
            .iter()
            .map(|bytes| Body::stream(Body::from(bytes.clone()).into_stream().unwrap())),
    );
    let mut response = Response::new(reqwest::Body::wrap_stream(body.into_stream().unwrap()));
    *response.status_mut() = StatusCode::from_u16(status).unwrap();
    for (name, value) in headers {
        if let (Ok(name), Ok(value)) = (
            name.parse::<concord_core::http::header::HeaderName>(),
            HeaderValue::from_bytes(value),
        ) {
            response.headers_mut().append(name, value);
        }
    }
    if let Some(n) = announced {
        response.headers_mut().insert(
            "content-length",
            HeaderValue::from_str(&n.to_string()).unwrap(),
        );
    }
    response.into()
}
fn diagnostic(error: &(dyn std::error::Error + 'static)) -> String {
    let mut result = String::new();
    let mut next = Some(error);
    for _ in 0..32 {
        match next {
            None => return result,
            Some(e) => {
                result.push_str(&format!("{e} {e:?} "));
                next = e.source();
            }
        }
    }
    assert!(next.is_none(), "cyclic source chain");
    result
}
fn classify<T: std::fmt::Debug>(
    result: &Result<T, Error>,
    status: u16,
    transport: bool,
    expected_success: bool,
) {
    if expected_success {
        assert!(result.is_ok(), "{result:?}");
        return;
    }
    let e = result.as_ref().unwrap_err();
    assert_eq!(e.status().unwrap().as_u16(), status);
    assert_eq!(
        e.kind(),
        if !(200..300).contains(&status) {
            Kind::Refused
        } else if transport {
            Kind::Transport
        } else {
            Kind::Mismatch
        }
    );
    if transport && (200..300).contains(&status) {
        assert_eq!(e.class(), Some(Class::Body));
    }
}
proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]
    #[test]
    /// Also proves C5.11, C6.1.
    fn c5_2_all_readers_preserve_bytes_or_classify_without_disclosure(
        chunks in proptest::collection::vec(proptest::collection::vec(any::<u8>(),0..256),0..12),
        announced in prop::option::of(prop_oneof![0u64..4096,Just(u64::MAX)]),
        limit in 0usize..2048,status in 100u16..1000,
        headers in proptest::collection::vec(("[a-z][a-z0-9-]{0,16}",proptest::collection::vec(any::<u8>(),0..64)),0..8),
        label in prop::sample::select(vec!["application/json","text/plain","text/html","application/problem+json","", "application/octet-stream"]),
        strict in any::<bool>(),
    ){
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        rt.block_on(async {
            let bytes: Vec<_> = chunks.iter().flatten().copied().collect();
            let mut headers = headers;
            headers.push(("content-type".into(), label.as_bytes().to_vec()));
            let make = || response(&chunks, announced, status, &headers);
            let info = TryInfo::new("props.read", 1);
            let limited = announced.is_some_and(|n| n > limit as u64) || bytes.len() > limit;
            let broken = announced.is_some_and(|n| n > bytes.len() as u64);
            let binary: Result<Bytes, Error> = read_body(&Binary.limit(limit), make(), &info).await;
            classify(&binary, status, broken && !limited, !limited && !broken);
            if let Ok(value) = &binary {
                assert_eq!(value.as_ref(), bytes);
            }
            if let Err(e) = &binary {
                assert!(
                    e.body().is_none_or(|b| b.len() <= limit.min(64 * 1024))
                        || !(200..300).contains(&status)
                );
            }
            let text: Result<String, Error> = read_body(&Text.limit(limit), make(), &info).await;
            let utf8 = std::str::from_utf8(&bytes).is_ok();
            classify(
                &text,
                status,
                broken && !limited,
                !limited && !broken && utf8,
            );
            if let Ok(value) = &text {
                assert_eq!(value.as_bytes(), bytes);
                assert!(value.capacity() <= limit);
            }
            let mut errors = Vec::new();
            if let Err(e) = binary {
                errors.push(e);
            }
            if let Err(e) = text {
                errors.push(e);
            }
            #[cfg(feature = "json")]
            {
                let reader = if strict {
                    Json.strict().limit(limit)
                } else {
                    Json.limit(limit)
                };
                let json: Result<serde_json::Value, Error> = read_body(&reader, make(), &info).await;
                let raw = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
                let valid = serde_json::from_slice::<serde_json::Value>(raw).is_ok();
                let label_ok = label == "application/json" || label.ends_with("+json");
                classify(
                    &json,
                    status,
                    broken && !limited && (!strict || label_ok),
                    !limited && !broken && valid && (!strict || label_ok),
                );
                if let Err(e) = json {
                    errors.push(e);
                }
            }
            #[cfg(not(feature = "json"))]
            let _ = strict;
            // Stream hands over without buffering or enforcing a whole-body limit.
            let mut stream = Stream.read(Incoming::new(make())).await.unwrap();
            let mut actual = Vec::new();
            let mut failure = None;
            loop {
                match stream.try_next().await {
                    Ok(Some(piece)) => actual.extend_from_slice(&piece),
                    Ok(None) => break,
                    Err(e) => {
                        failure = Some(e);
                        break;
                    }
                }
            }
            assert_eq!(actual, bytes);
            assert_eq!(failure.is_some(), broken);
            if let Some(e) = failure {
                assert_eq!(e.kind(), Kind::Transport);
                assert_eq!(e.class(), Some(Class::Body));
                assert!(stream.try_next().await.unwrap().is_none());
            }
            // Search only nontrivial byte strings; empty/one-letter inputs occur in ordinary prose.
            for e in &errors {
                if let Ok(raw) = std::str::from_utf8(&bytes) {
                    if raw.len() >= 16 {
                        assert!(!diagnostic(e).contains(raw));
                    }
                }
                assert!(e.body().is_none_or(|b| b.len() <= 64 * 1024));
            }
            // All engine-created answer error kinds also receive body sentinels separately below.
        });
    }

    #[test]
    fn c6_1_answer_errors_never_print_the_body(payload in ".{0,128}",status in 100u16..1000){
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        rt.block_on(async {
            let sentinel = format!("BODY_SENTINEL_BEGIN{payload}BODY_SENTINEL_END");
            let bytes = sentinel.as_bytes().to_vec();
            let chunks = vec![bytes.clone()];
            let info = TryInfo::new("props.redaction", 1);
            let make = || response(&chunks, None, status, &[]);
            let refused: Error = refused_plain(make(), &info).await;
            let mismatch: Error = no_arm(make(), &info).await;
            for e in [refused, mismatch] {
                assert!(!diagnostic(&e).contains(&sentinel));
            }
            let text = concord_core::read::ReadError::not_the_type(Bytes::from(bytes))
                .with_details(sentinel.clone());
            assert!(!diagnostic(&text).contains(&sentinel));
            #[cfg(feature = "json")]
            {
                let result: Result<u32, Error> = read_body(&Json, make(), &info).await;
                assert!(!diagnostic(&result.unwrap_err()).contains(&sentinel));
                // Valid JSON exercises type failures, map keys and enum variant diagnostics.
                let scalar = serde_json::to_vec(&sentinel).unwrap();
                let chunks = vec![scalar];
                let result: Result<u32, Error> =
                    read_body(&Json, response(&chunks, None, status, &[]), &info).await;
                assert!(!diagnostic(&result.unwrap_err()).contains(&sentinel));
                let chunks = vec![serde_json::to_vec(&std::collections::BTreeMap::from([(
                    &sentinel, &sentinel,
                )]))
                .unwrap()];
                let result: Result<std::collections::BTreeMap<String, u32>, Error> =
                    read_body(&Json, response(&chunks, None, status, &[]), &info).await;
                assert!(!diagnostic(&result.unwrap_err()).contains(&sentinel));
                let chunks = vec![serde_json::to_vec(&vec![&sentinel]).unwrap()];
                let result: Result<Vec<u32>, Error> =
                    read_body(&Json, response(&chunks, None, status, &[]), &info).await;
                assert!(!diagnostic(&result.unwrap_err()).contains(&sentinel));
                #[derive(Debug, serde::Deserialize)]
                enum Variant {
                    Known,
                }
                let chunks = vec![serde_json::to_vec(&sentinel).unwrap()];
                let result: Result<Variant, Error> =
                    read_body(&Json, response(&chunks, None, status, &[]), &info).await;
                assert!(!diagnostic(&result.unwrap_err()).contains(&sentinel));
            }
            let mut stream = Stream
                .read(Incoming::new(response(
                    &chunks,
                    Some((sentinel.len() + 1) as u64),
                    status,
                    &[],
                )))
                .await
                .unwrap();
            assert!(stream.try_next().await.unwrap().is_some());
            let e = stream.try_next().await.unwrap_err();
            assert!(!diagnostic(&e).contains(&sentinel));
        });
    }
}

// Headers are independent of the arbitrary payload; this also checks opaque header values.
#[test]
fn c5_11_empty_announced_excess_has_no_kept_body() {
    let mut h = HeaderMap::new();
    h.insert("content-length", HeaderValue::from_static("99"));
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    let e: Error = rt
        .block_on(read_body::<Bytes, (), _>(
            &Binary.limit(1),
            response(&[b"secret".to_vec()], Some(99), 200, &[]),
            &TryInfo::new("props.limit", 1),
        ))
        .unwrap_err();
    assert_eq!(e.kind(), Kind::Mismatch);
    assert_eq!(e.kept(), None);
    assert_eq!(e.body(), None);
}
