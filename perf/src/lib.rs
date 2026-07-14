use concord_core::advanced::{OctetStream, StreamBody};
use concord_core::prelude::Text;
use concord_macros::api;

api! {
    client BenchmarkClient {
        base "https://benchmark.invalid"

        policies {
            rate_limit benchmark {
                bucket host by [host] {
                    100000 / 1s
                }
            }
        }

        default {
            rate_limit benchmark
        }
    }

    GET Ping
        as ping
        path ["ping"]
        -> Text<String>

    GET Download
        as download
        path ["download"]
        -> Stream<OctetStream>

    POST Upload(body: Stream<OctetStream>)
        as upload
        path ["upload"]
        -> Text<String>

    POST MultipartUpload(body: Multipart<()>)
        as multipart_upload
        path ["multipart"]
        -> Text<String>
}

pub use self::benchmark_client::BenchmarkClient;

api! {
    client AuthBenchmarkClient {
        base "https://benchmark.invalid"
        secret token: String
        credential session = bearer(secret.token)
    }

    GET Protected
        as protected
        path ["protected"]
        auth bearer session
        -> Text<String>
}

pub use self::auth_benchmark_client::AuthBenchmarkClient;

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use concord_core::prelude::{ApiClientError, RetryMode, StatusRetryConfig};
    use concord_test_support::{ScriptedReply, deterministic_mock};

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("performance test runtime")
    }

    #[test]
    fn generated_visible_execution_is_deterministic() {
        let (server, handle) = deterministic_mock()
            .repeating(ScriptedReply::ok_text(Bytes::from_static(b"pong")))
            .build();
        let client = BenchmarkClient::new_with_safe_reqwest_builder(|builder| {
            server.configure_both(builder)
        })
        .expect("deterministic managed client");
        runtime().block_on(async {
            assert_eq!(client.ping().await.expect("generated response"), "pong");
            assert_eq!(handle.recorded_len(), 1);
        });
    }

    #[test]
    fn retry_modes_construct_on_the_generated_surface() {
        BenchmarkClient::new_with_retry_mode(RetryMode::Disabled).expect("disabled retry mode");
        BenchmarkClient::new_with_retry_mode(RetryMode::ProtocolRecovery)
            .expect("protocol recovery mode");
        let status = StatusRetryConfig::new(2, [http::StatusCode::BAD_GATEWAY])
            .expect("valid status retry configuration");
        BenchmarkClient::new_with_retry_mode(RetryMode::Status(status))
            .expect("fixed-origin status retry mode");
    }

    #[test]
    fn generated_response_limit_remains_structural() {
        let (server, _handle) = deterministic_mock()
            .reply(ScriptedReply::ok_text(Bytes::from_static(b"oversized")))
            .build();
        let mut client = BenchmarkClient::new_with_safe_reqwest_builder(|builder| {
            server.configure_both(builder)
        })
        .expect("deterministic managed client");
        client.configure_mut(|config| {
            config.max_response_body_bytes(4);
        });
        let error = runtime()
            .block_on(client.ping().execute())
            .expect_err("response must exceed configured limit");
        assert!(matches!(
            error,
            ApiClientError::ResponseTooLarge { .. }
                | ApiClientError::ResponseBodyLimitExceeded { .. }
        ));
    }
}
