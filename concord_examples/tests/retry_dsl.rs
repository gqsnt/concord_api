use concord_core::prelude::*;
use concord_macros::api;
use concord_test_support::*;

#[tokio::test]
async fn retry_profile_retries_status_then_endpoint_can_turn_it_off() {
    api! {
        client ApiDslRetryProfile {
            scheme: https,
            host: "example.com",
            retry {
                profile read {
                    attempts 2
                    methods [GET, HEAD]
                    on status[503]
                    backoff none
                }
                default read
            }
        }

        GET Ping
        -> Json<()>
        {
        }

        GET NoRetry
        -> Json<()>
        {
            retry off
        }
    }

    use api_dsl_retry_profile::*;

    let (transport, h) = mock()
        .replies([
            MockReply::status(http::StatusCode::SERVICE_UNAVAILABLE),
            MockReply::ok_json(json_bytes(&())),
            MockReply::status(http::StatusCode::SERVICE_UNAVAILABLE),
        ])
        .build();
    let api = ApiDslRetryProfile::new_with_transport(transport);

    api.request(endpoints::Ping::new()).execute().await.unwrap();
    let err = api
        .request(endpoints::NoRetry::new())
        .execute()
        .await
        .expect_err("retry off should return the first status error");

    match err {
        ApiClientError::HttpStatus { status, .. } => {
            assert_eq!(status, http::StatusCode::SERVICE_UNAVAILABLE);
        }
        other => panic!("unexpected error: {other:?}"),
    }

    let reqs = h.recorded();
    assert_eq!(reqs.len(), 3);
    assert_eq!(reqs[0].meta.attempt, 0);
    assert_eq!(reqs[1].meta.attempt, 1);
    assert_eq!(reqs[2].meta.attempt, 0);
    h.finish();
}

#[tokio::test]
async fn retry_scope_profile_applies_to_child_endpoints() {
    api! {
        client ApiDslRetryScope {
            scheme: https,
            host: "example.com",
            retry {
                profile base {
                    attempts 2
                    methods [GET]
                    backoff none
                }
                profile read extends base {
                    on status[503]
                }
            }
        }

        scope service {
            path["api"]
            retry read

            GET Flaky
            -> Json<()>
            {
            }

            GET NoRetry
            -> Json<()>
            {
                retry off
            }
        }
    }

    use api_dsl_retry_scope::*;

    let (transport, h) = mock()
        .replies([
            MockReply::status(http::StatusCode::SERVICE_UNAVAILABLE),
            MockReply::ok_json(json_bytes(&())),
            MockReply::status(http::StatusCode::SERVICE_UNAVAILABLE),
        ])
        .build();
    let api = ApiDslRetryScope::new_with_transport(transport);

    api.request(endpoints::Flaky::new())
        .execute()
        .await
        .unwrap();
    let err = api
        .request(endpoints::NoRetry::new())
        .execute()
        .await
        .expect_err("endpoint retry off should override scope retry");

    match err {
        ApiClientError::HttpStatus { status, .. } => {
            assert_eq!(status, http::StatusCode::SERVICE_UNAVAILABLE);
        }
        other => panic!("unexpected error: {other:?}"),
    }

    let reqs = h.recorded();
    assert_eq!(reqs.len(), 3);
    assert_eq!(reqs[0].meta.attempt, 0);
    assert_eq!(reqs[1].meta.attempt, 1);
    assert_eq!(reqs[2].meta.attempt, 0);
    h.finish();
}

#[tokio::test]
async fn retry_patch_honors_retry_after_status() {
    api! {
        client ApiDslRetryPatch {
            scheme: https,
            host: "example.com",
        }

        GET Limited
        -> Json<()>
        {
            retry {
                attempts 2
                methods [GET]
                on status[429]
                retry_after honor
                backoff none
            }
        }
    }

    use api_dsl_retry_patch::*;

    let throttled = MockReply::status(http::StatusCode::TOO_MANY_REQUESTS).with_header(
        http::header::RETRY_AFTER,
        http::HeaderValue::from_static("0"),
    );
    let (transport, h) = mock()
        .replies([throttled, MockReply::ok_json(json_bytes(&()))])
        .build();
    let api = ApiDslRetryPatch::new_with_transport(transport);

    api.request(endpoints::Limited::new())
        .execute()
        .await
        .unwrap();

    let reqs = h.recorded();
    assert_eq!(reqs.len(), 2);
    assert_eq!(reqs[0].meta.attempt, 0);
    assert_eq!(reqs[1].meta.attempt, 1);
    h.finish();
}

#[tokio::test]
async fn retry_post_requires_declared_idempotency_header() {
    api! {
        client ApiDslRetryPost {
            scheme: https,
            host: "example.com",
            retry {
                profile write {
                    attempts 2
                    methods [POST]
                    on status[503]
                    idempotency header("Idempotency-Key")
                    backoff none
                }
            }
        }

        POST Create
        -> Json<()>
        {
            retry write
            headers {
                "Idempotency-Key" as idempotency_key: String
            }
        }

        POST UnsafeCreate
        -> Json<()>
        {
            retry write
        }
    }

    use api_dsl_retry_post::*;

    let (transport, h) = mock()
        .replies([
            MockReply::status(http::StatusCode::SERVICE_UNAVAILABLE),
            MockReply::ok_json(json_bytes(&())),
            MockReply::status(http::StatusCode::SERVICE_UNAVAILABLE),
        ])
        .build();
    let api = ApiDslRetryPost::new_with_transport(transport);

    api.request(endpoints::Create::new("create-1".to_string()))
        .execute()
        .await
        .unwrap();
    let err = api
        .request(endpoints::UnsafeCreate::new())
        .execute()
        .await
        .expect_err("POST without idempotency header should not retry");

    match err {
        ApiClientError::HttpStatus { status, .. } => {
            assert_eq!(status, http::StatusCode::SERVICE_UNAVAILABLE);
        }
        other => panic!("unexpected error: {other:?}"),
    }

    let reqs = h.recorded();
    assert_eq!(reqs.len(), 3);
    assert_request(&reqs[0]).header("idempotency-key", "create-1");
    assert_request(&reqs[1]).header("idempotency-key", "create-1");
    assert_request(&reqs[2]).header_absent("idempotency-key");
    h.finish();
}
