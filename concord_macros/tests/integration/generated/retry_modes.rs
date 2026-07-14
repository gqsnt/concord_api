use bytes::Bytes;
use concord_core::prelude::{CursorPagination, Json, RetryMode, Text};
use concord_macros::api;
use concord_test_support::{MockReply, mock};
use http::StatusCode;

use self::cross_origin_pagination_status_api::CrossOriginPaginationStatusApi;
use self::dynamic_status_api::DynamicStatusApi;
use self::fixed_status_api::FixedStatusApi;
use self::hostless_status_api::HostlessStatusApi;
use self::multiple_status_api::MultipleStatusApi;

api! {
    client FixedStatusApi {
        base "https://example.com"
    }

    GET Ping
        path ["ping"]
        -> Text<String>
}

api! {
    client HostlessStatusApi {
        base "https://example.com"
    }
}

api! {
    client CrossOriginPaginationStatusApi {
        base "https://example.com"
    }

    scope tenant(cursor?: String) {
        host [cursor]
        GET Pages
            paginate CursorPagination<String> { cursor = cursor }
            -> Json<Vec<String>>
    }
}

api! {
    client DynamicStatusApi {
        base "https://example.com"
        var tenant: String
    }

    scope tenant {
        host [vars.tenant]

        GET Ping
            path ["ping"]
            -> Text<String>
    }
}

api! {
    client MultipleStatusApi {
        base "https://example.com"
    }

    scope one {
        host ["one.example.com"]

        GET Ping
            path ["ping"]
            -> Text<String>
    }

    scope two {
        host ["two.example.com"]

        GET Pong
            path ["pong"]
            -> Text<String>
    }
}

#[tokio::test]
async fn generated_fixed_origin_can_install_status_retry() {
    let (transport, handle) = mock()
        .replies([
            MockReply::status(StatusCode::SERVICE_UNAVAILABLE),
            MockReply::ok_text(Bytes::from_static(b"recovered")),
        ])
        .build();
    let retry_mode = RetryMode::status(1, [StatusCode::SERVICE_UNAVAILABLE]).unwrap();
    let api = FixedStatusApi::new_with_safe_reqwest_builder_and_retry_mode(retry_mode, |builder| {
        Ok(transport.configure_reqwest(builder))
    })
    .expect("generated fixed-origin client accepts status retry");

    let value = api.ping().execute().await.expect("status retry succeeds");
    assert_eq!(value, "recovered");
    assert_eq!(handle.recorded().len(), 2);
}

#[tokio::test]
async fn generated_status_mode_never_classifies_authentication_challenges() {
    for challenge in [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN] {
        let (transport, handle) = mock().replies([MockReply::status(challenge)]).build();
        let retry_mode = RetryMode::status(2, [StatusCode::SERVICE_UNAVAILABLE]).unwrap();
        let api =
            FixedStatusApi::new_with_safe_reqwest_builder_and_retry_mode(retry_mode, |builder| {
                Ok(transport.configure_reqwest(builder))
            })
            .expect("fixed-origin status mode");

        api.ping()
            .execute()
            .await
            .expect_err("challenge remains terminal");
        assert_eq!(handle.recorded().len(), 1);
    }
}

#[test]
fn generated_dynamic_and_multi_origin_clients_reject_status_retry() {
    let retry_mode = RetryMode::status(1, [StatusCode::SERVICE_UNAVAILABLE]).unwrap();

    let dynamic = match DynamicStatusApi::new_with_retry_mode(
        "tenant.example.com".into(),
        retry_mode.clone(),
    ) {
        Ok(_) => panic!("dynamic-origin clients cannot install status retry"),
        Err(error) => error,
    };
    assert!(matches!(
        dynamic,
        concord_core::prelude::RetryModeError::NotFixedOrigin
    ));

    let multiple = match MultipleStatusApi::new_with_retry_mode(retry_mode.clone()) {
        Ok(_) => panic!("multi-origin clients cannot install status retry"),
        Err(error) => error,
    };
    assert!(matches!(
        multiple,
        concord_core::prelude::RetryModeError::NotFixedOrigin
    ));

    let cross_origin = match CrossOriginPaginationStatusApi::new_with_retry_mode(retry_mode) {
        Ok(_) => panic!("cross-origin pagination cannot install status retry"),
        Err(error) => error,
    };
    assert!(matches!(
        cross_origin,
        concord_core::prelude::RetryModeError::NotFixedOrigin
    ));

    let hostless = match HostlessStatusApi::new_with_retry_mode(
        RetryMode::status(1, [StatusCode::SERVICE_UNAVAILABLE]).unwrap(),
    ) {
        Ok(_) => panic!("hostless generated clients cannot install status retry"),
        Err(error) => error,
    };
    assert!(matches!(
        hostless,
        concord_core::prelude::RetryModeError::NotFixedOrigin
    ));
}
