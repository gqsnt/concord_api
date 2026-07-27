#![forbid(unsafe_code)]

use concord::http::SafeProxy;
use concord_test_support::server::{ResponseScript, ScriptedServer};

fn proxy(server: &ScriptedServer) -> SafeProxy {
    SafeProxy::parse(server.url("")).expect("loopback proxy")
}

// ANCHOR: first_request_loopback
#[tokio::test]
async fn getting_started_request_executes_against_loopback() {
    use concord::http::SafeProxy;
    use concord_examples::getting_started::minimal::{MinimalApi, execute, minimal_api};
    use concord_test_support::server::{ResponseScript, ScriptedServer};

    let server =
        ScriptedServer::start([ResponseScript::status(204, Vec::new())]).expect("health server");
    let proxy = SafeProxy::parse(server.url("")).expect("loopback proxy");
    let client = MinimalApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("minimal client");

    let endpoint = minimal_api::Health::new();
    let plan = client.plan(&endpoint).expect("safe health plan");
    assert_eq!(plan.endpoint_id(), "minimal_api.health");

    execute(&client).await.expect("successful health request");

    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method(), "GET");
    assert!(requests[0].target().ends_with("/v1/health"));
}
// ANCHOR_END: first_request_loopback

#[cfg(all(feature = "oauth2-client", feature = "default-tls"))]
#[tokio::test]
async fn actual_authentication_example_stores_and_applies_endpoint_credentials_over_tls() {
    use concord::SecretString;
    use concord::auth::OAuth2ClientCredentials;
    use concord_examples::authentication::api::AuthenticationApi;
    use concord_examples::authentication::extensions::ExampleProvider;
    use concord_test_support::proxy::ConnectProxy;
    use concord_test_support::tls::TlsServer;

    const ACQUIRED_SENTINEL: &str = "endpoint-token-phase11-8c72";

    let server = TlsServer::start().expect("trusted TLS server");
    server
        .queue_response(200, ACQUIRED_SENTINEL.as_bytes())
        .expect("token response");
    server
        .queue_response(200, Vec::new())
        .expect("custom Authorization response");
    server
        .queue_response(200, Vec::new())
        .expect("application response");
    let tunnel = ConnectProxy::start(server.address()).expect("CONNECT tunnel");
    let proxy = SafeProxy::parse(tunnel.url()).expect("safe CONNECT proxy");
    let certificate = server.certificate_pem();
    let oauth = OAuth2ClientCredentials::builder("https://localhost/oauth/token")
        .client_id("local-client")
        .client_secret("local-client-secret")
        .build()
        .expect("OAuth helper");
    let client = AuthenticationApi::builder()
        .api_key(SecretString::try_new("local-api-key").expect("bounded local API key"))
        .bearer_token(SecretString::try_new("local-bearer").expect("bounded local bearer token"))
        .username(SecretString::try_new("local-user").expect("bounded local username"))
        .password(SecretString::try_new("local-password").expect("bounded local password"))
        .oauth_provider(oauth)
        .custom_provider(ExampleProvider)
        .configure_http(move |http| {
            http.http1_only()
                .proxy(proxy)
                .add_root_certificate_pem(certificate)
        })
        .build()
        .expect("authentication client");

    client
        .login()
        .store_as_acquired()
        .await
        .expect("endpoint credential acquisition");
    assert!(client.credentials().acquired().is_set());
    client
        .token_authorization()
        .await
        .expect("custom Authorization placement");
    client
        .endpoint_backed()
        .await
        .expect("credential application");
    drop(client);
    tokio::task::yield_now().await;

    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].method(), "GET");
    assert!(!requests[0].target().contains(ACQUIRED_SENTINEL));
    assert_eq!(
        requests[1].header("authorization"),
        Some("Token local-api-key")
    );
    assert!(
        requests[2]
            .target()
            .contains(&format!("session={ACQUIRED_SENTINEL}"))
    );
}

#[cfg(all(feature = "json", feature = "multipart"))]
#[tokio::test]
async fn actual_body_example_round_trips_the_custom_codec_locally() {
    use concord_examples::request_response::bodies::BodyApi;

    let server = ScriptedServer::start([ResponseScript::ok(b"local response".to_vec())
        .with_header("content-type", "application/vnd.concord.example")])
    .expect("body server");
    let proxy = proxy(&server);
    let client = BodyApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("body client");

    let value = client
        .custom_round_trip("local request".to_owned())
        .await
        .expect("custom codec response");
    assert_eq!(value, "local response");
    assert_eq!(server.request_count(), 1);
    assert_eq!(server.requests()[0].body(), b"local request");
}

#[tokio::test]
async fn actual_specification_example_runs_generated_and_runtime_endpoints_locally() {
    use concord::Bytes;
    use concord::endpoint::Request;
    use concord::endpoint::{EndpointId, RuntimeEndpoint};
    use concord::http::Method;
    use concord_examples::request_response::specifications::{
        ExampleBufferedSpec, ExampleRequestSpec, GenericSpecificationApi,
    };

    let server = ScriptedServer::start([
        ResponseScript::ok(b"generated".to_vec()),
        ResponseScript::ok(b"runtime".to_vec()),
    ])
    .expect("specification server");
    let proxy = proxy(&server);
    let client = GenericSpecificationApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("specification client");

    let generated = client
        .buffered(Bytes::from_static(b"generated request"))
        .await
        .expect("generated specification request");
    assert_eq!(generated, b"generated".len());

    let endpoint = RuntimeEndpoint::builder(
        EndpointId::new("examples.specifications.runtime").expect("runtime ID"),
        Method::POST,
    )
    .path_segment("runtime")
    .body_spec::<ExampleRequestSpec>(Bytes::from_static(b"runtime request"))
    .response::<ExampleBufferedSpec>()
    .build()
    .expect("runtime endpoint");
    let request: Request<_> = client.request(endpoint);
    let runtime = request.await.expect("runtime specification request");
    assert_eq!(runtime, b"runtime".len());

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].body(), b"generated request");
    assert_eq!(requests[1].body(), b"runtime request");
}

#[cfg(feature = "json")]
#[tokio::test]
async fn actual_pagination_example_binds_each_next_offset_sequentially() {
    use std::num::NonZeroU32;

    use concord::PageLimit;
    use concord_examples::pagination::PaginationApi;

    let server = ScriptedServer::start([
        ResponseScript::ok(br#"[1,2]"#.to_vec()).with_header("content-type", "application/json"),
        ResponseScript::ok(br#"[3]"#.to_vec()).with_header("content-type", "application/json"),
    ])
    .expect("pagination server");
    let proxy = proxy(&server);
    let client = PaginationApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("pagination client");

    let values = client
        .offset(0, NonZeroU32::new(2).expect("positive page size"))
        .paginate(PageLimit::Take(3))
        .await
        .expect("offset pagination");
    assert_eq!(values, [1, 2, 3]);

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].target().contains("offset=0"));
    assert!(requests[0].target().contains("limit=2"));
    assert!(requests[1].target().contains("offset=2"));
    assert!(requests[1].target().contains("limit=2"));
}

#[cfg(feature = "json")]
#[tokio::test]
async fn post_pagination_repeats_one_stable_json_body_for_every_page() {
    use std::num::NonZeroU32;

    use concord::PageLimit;
    use concord_examples::pagination::PaginationApi;
    use concord_examples::pagination::search::SearchFilter;

    let server = ScriptedServer::start([
        ResponseScript::ok(br#"[1,2]"#.to_vec()).with_header("content-type", "application/json"),
        ResponseScript::ok(br#"[3]"#.to_vec()).with_header("content-type", "application/json"),
    ])
    .expect("search pagination server");
    let proxy = proxy(&server);
    let client = PaginationApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("pagination client");
    let filter = SearchFilter {
        query: "rust".to_owned(),
        include_archived: false,
    };

    let values = client
        .search(0, NonZeroU32::new(2).expect("positive page size"), filter)
        .paginate(PageLimit::Take(2))
        .await
        .expect("POST search pagination");
    assert_eq!(values, [1, 2, 3]);

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    let expected_body = br#"{"query":"rust","include_archived":false}"#;
    for request in &requests {
        assert_eq!(request.method(), "POST");
        assert_eq!(request.header("content-type"), Some("application/json"));
        assert_eq!(request.body(), expected_body);
        assert!(request.target().contains("/search"));
        assert!(request.target().contains("limit=2"));
    }
    assert!(requests[0].target().contains("offset=0"));
    assert!(requests[1].target().contains("offset=2"));
}

#[cfg(feature = "rate-limit")]
#[tokio::test]
async fn actual_rate_limit_example_honors_an_explicit_downstream_limiter() {
    use std::sync::Arc;

    use concord::rate_limit::RateLimiter;
    use concord_examples::rate_limiting::custom_limiter::CountingLimiter;
    use concord_examples::rate_limiting::lexical::RateLimitApi;

    let server = ScriptedServer::start([ResponseScript::ok(b"limited response".to_vec())])
        .expect("rate-limit server");
    let proxy = proxy(&server);
    let limiter = CountingLimiter::new();
    let configured: Arc<dyn RateLimiter> = Arc::new(limiter.clone());
    let client = RateLimitApi::builder()
        .set_tenant("tenant-a".to_owned())
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(configured))
        .build()
        .expect("rate-limit client");

    let value = client
        .region("eu".to_owned())
        .item("record-42".to_owned())
        .await
        .expect("rate-limited request");
    assert_eq!(value, concord::Bytes::from_static(b"limited response"));
    assert_eq!(limiter.acquisitions(), 1);
    assert_eq!(server.request_count(), 1);
}

#[cfg(feature = "rate-limit")]
#[tokio::test]
async fn local_state_capacity_is_recoverable_without_evicting_an_existing_bucket() {
    use std::sync::Arc;

    use concord::rate_limit::{LocalRateLimiter, RateLimiter};
    use concord_examples::rate_limiting::partitioning::{
        ResourceStateApi, is_recoverable_state_capacity,
    };

    let server = ScriptedServer::start([
        ResponseScript::ok(b"first".to_vec()),
        ResponseScript::ok(b"retained".to_vec()),
    ])
    .expect("bounded local-state server");
    let proxy = proxy(&server);
    let limiter: Arc<dyn RateLimiter> = Arc::new(
        LocalRateLimiter::new()
            .quota_capacity(1)
            .expect("positive quota-state capacity"),
    );
    let client = ResourceStateApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(limiter))
        .build()
        .expect("bounded local-state client");

    assert_eq!(
        client
            .channel("first-key".to_owned())
            .await
            .expect("first bucket"),
        concord::Bytes::from_static(b"first")
    );
    let error = client
        .channel("new-high-cardinality-key".to_owned())
        .await
        .expect_err("new bucket must fail while bounded state is full");
    assert!(is_recoverable_state_capacity(&error));
    assert_eq!(
        server.request_count(),
        1,
        "capacity failure must happen before transport"
    );

    assert_eq!(
        client
            .channel("first-key".to_owned())
            .await
            .expect("existing bucket remains operational"),
        concord::Bytes::from_static(b"retained")
    );
    assert_eq!(server.request_count(), 2);
}

#[cfg(feature = "rate-limit")]
#[tokio::test(start_paused = true)]
async fn independently_built_clients_can_share_one_local_limiter_object() {
    use std::sync::Arc;

    use concord::rate_limit::{LocalRateLimiter, RateLimiter};
    use concord_examples::rate_limiting::partitioning::SharedStateApi;

    let server = ScriptedServer::start([ResponseScript::ok(b"first".to_vec())])
        .expect("shared local-state server");
    let limiter: Arc<dyn RateLimiter> = Arc::new(LocalRateLimiter::new());
    let first = SharedStateApi::builder()
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime({
            let limiter = Arc::clone(&limiter);
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("first shared-limiter client");
    let second = SharedStateApi::builder()
        .configure_http({
            let proxy = proxy(&server);
            move |http| http.proxy(proxy)
        })
        .configure_runtime(move |runtime| runtime.rate_limiter(limiter))
        .build()
        .expect("second shared-limiter client");

    assert_eq!(
        first.ping().await.expect("first shared bucket request"),
        concord::Bytes::from_static(b"first")
    );
    let repeated = tokio::spawn(async move { second.ping().await });
    tokio::task::yield_now().await;
    assert!(
        !repeated.is_finished(),
        "the second build must observe quota consumed through the shared limiter"
    );
    assert_eq!(server.request_count(), 1);
    repeated.abort();
    let _ = repeated.await;
}

#[cfg(all(feature = "records", feature = "csv", feature = "ndjson"))]
mod record_boundaries {
    use std::pin::Pin;

    use concord::records::{RecordDecoder, RecordError, RecordStream};
    use concord_examples::request_response::records::RecordsApi;
    use futures_core::Stream;

    use super::{ResponseScript, ScriptedServer, proxy};

    async fn collect<D>(mut stream: RecordStream<D>) -> Result<Vec<D::Record>, RecordError>
    where
        D: RecordDecoder,
    {
        let mut values = Vec::new();
        while let Some(value) =
            std::future::poll_fn(|context| Pin::new(&mut stream).poll_next(context)).await
        {
            values.push(value?);
        }
        Ok(values)
    }

    fn one_byte(input: &[u8]) -> Vec<Vec<u8>> {
        input.iter().map(|byte| vec![*byte]).collect()
    }

    fn response(content_type: &str, chunks: Vec<Vec<u8>>) -> ResponseScript {
        ResponseScript::Chunked {
            status: 200,
            headers: vec![("content-type".to_owned(), content_type.to_owned())],
            chunks,
        }
    }

    #[tokio::test]
    async fn actual_csv_and_ndjson_declarations_survive_one_byte_and_arbitrary_chunks() {
        let csv = b"name,count\nalpha,1\nbeta,2\n";
        let configured_csv = b"name;count\nconfigured;3\n";
        let ndjson = b"1\n20\r\n3";
        let server = ScriptedServer::start([
            response("text/csv", one_byte(csv)),
            response(
                "text/csv; charset=utf-8",
                vec![
                    b"na".to_vec(),
                    b"me,count\nalpha".to_vec(),
                    b",1\nb".to_vec(),
                    b"eta,2\n".to_vec(),
                ],
            ),
            response("text/csv", one_byte(configured_csv)),
            response("application/x-ndjson", one_byte(ndjson)),
            response(
                "application/ndjson",
                vec![
                    b"1\n2".to_vec(),
                    b"0\r".to_vec(),
                    b"\n".to_vec(),
                    b"3".to_vec(),
                ],
            ),
        ])
        .expect("record server");
        let proxy = proxy(&server);
        let client = RecordsApi::builder()
            .configure_http(move |http| http.proxy(proxy))
            .build()
            .expect("records client");

        for expected in [
            vec![("alpha".to_owned(), 1_u32), ("beta".to_owned(), 2)],
            vec![("alpha".to_owned(), 1_u32), ("beta".to_owned(), 2)],
        ] {
            let values = collect(client.csv_rows().await.expect("CSV stream opens"))
                .await
                .expect("CSV stream decodes");
            assert_eq!(values, expected);
        }

        let configured = collect(
            client
                .configured_csv_rows()
                .await
                .expect("configured CSV stream opens"),
        )
        .await
        .expect("configured CSV stream decodes");
        assert_eq!(configured, [("configured".to_owned(), 3)]);

        for _ in 0..2 {
            let values = collect(client.nd_json_values().await.expect("NDJSON stream opens"))
                .await
                .expect("NDJSON stream decodes");
            assert_eq!(values, [1, 20, 3]);
        }

        assert_eq!(server.request_count(), 5);
    }
}
