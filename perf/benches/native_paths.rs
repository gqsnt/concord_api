use bytes::Bytes;
use concord_core::advanced::{MultipartBody, StreamBody};
use concord_core::prelude::ApiClientError;
use concord_test_support::{MockReply, mock};
use criterion::{Criterion, criterion_group, criterion_main};
use http::{HeaderValue, StatusCode};
use perf::{AuthBenchmarkClient, BenchmarkClient};

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("benchmark runtime")
}

fn bench(c: &mut Criterion) {
    let runtime = runtime();
    let (server, _handle) = mock()
        .repeating(MockReply::ok_text(Bytes::from_static(b"ok")))
        .build();
    let client =
        BenchmarkClient::new_with_safe_reqwest_builder(|builder| server.configure_reqwest(builder))
            .expect("loopback managed client");
    let (download_server, _download_handle) = mock()
        .repeating(
            MockReply::status(StatusCode::OK)
                .with_header(
                    http::header::CONTENT_TYPE,
                    HeaderValue::from_static("application/octet-stream"),
                )
                .with_body(Bytes::from_static(b"stream response")),
        )
        .build();
    let download_client = BenchmarkClient::new_with_safe_reqwest_builder(|builder| {
        download_server.configure_reqwest(builder)
    })
    .expect("streaming loopback client");

    c.bench_function("native_path/streaming_request", |b| {
        b.to_async(&runtime).iter(|| async {
            client
                .upload(StreamBody::from_bytes(Bytes::from_static(
                    b"stream payload",
                )))
                .await
                .expect("streaming upload")
        });
    });

    c.bench_function("native_path/streaming_response", |b| {
        b.to_async(&runtime).iter(|| async {
            let mut response = download_client
                .download()
                .execute_stream()
                .await
                .expect("streaming response");
            while response
                .next_chunk()
                .await
                .expect("response chunk")
                .is_some()
            {}
        });
    });

    c.bench_function("native_path/direct_multipart", |b| {
        b.to_async(&runtime).iter(|| async {
            client
                .multipart_upload(
                    MultipartBody::new().bytes("payload", Bytes::from_static(b"multipart")),
                )
                .await
                .expect("multipart upload")
        });
    });

    c.bench_function("native_path/response_limit", |b| {
        let mut limited = client.clone();
        limited.configure_mut(|config| {
            config.max_response_body_bytes(1);
        });
        b.to_async(&runtime).iter(|| async {
            let error = limited.ping().await.expect_err("bounded response");
            assert!(matches!(
                error,
                ApiClientError::ResponseTooLarge { .. }
                    | ApiClientError::ResponseBodyLimitExceeded { .. }
            ));
        });
    });

    c.bench_function("native_path/authentication_recovery_end_to_end", |b| {
        b.iter(|| {
            let (server, handle) = mock()
                .replies([
                    MockReply::status(StatusCode::UNAUTHORIZED),
                    MockReply::ok_text(Bytes::from_static(b"recovered")),
                ])
                .build();
            let client = AuthBenchmarkClient::new_with_safe_reqwest_builder(
                "benchmark-token".to_string(),
                |builder| server.configure_reqwest(builder),
            )
            .expect("auth benchmark client");
            runtime.block_on(async {
                client
                    .protected()
                    .await
                    .expect("one authentication recovery");
            });
            assert_eq!(handle.wire_request_count(), 2);
        });
    });

    c.bench_function("native_path/retry_after_future_call_end_to_end", |b| {
        b.iter(|| {
            let (server, handle) = mock()
                .replies([
                    MockReply::status(StatusCode::TOO_MANY_REQUESTS)
                        .with_header(http::header::RETRY_AFTER, HeaderValue::from_static("0")),
                    MockReply::ok_text(Bytes::from_static(b"ok")),
                ])
                .build();
            let client = BenchmarkClient::new_with_safe_reqwest_builder(|builder| {
                server.configure_reqwest(builder)
            })
            .expect("cooldown benchmark client");
            runtime.block_on(async {
                let _ = client.ping().await.expect_err("terminal 429");
                client.ping().await.expect("future call after cooldown");
            });
            assert_eq!(handle.wire_request_count(), 2);
        });
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
