//! Loopback evidence for buffered outcomes owned by extras: `NotFound<R>`,
//! accepted bytes, explicit status-body JSON decoding and downstream buffered
//! codec bridges (V3-E07, V3-E09, V3-E10).

// The endpoint harness and status-body imports serve the JSON outcome tests;
// without `json` only the buffered-codec bridge test runs.
#![cfg_attr(not(feature = "json"), allow(unused_imports, dead_code))]

use std::marker::PhantomData;
use std::time::Duration;

use concord_core::__private::{
    AcceptedStatus, ClientCore, Effect, EmptyBody, EndpointSpec, HeadBuilder, PreparedHead,
    RequestMeta, RequestRepresentation, ResponseRepresentation, accepted_bytes_request,
    build_client_core, default_http_config, default_runtime_config, request, response_request,
};
use concord_core::contracts::{
    ApiError, BodyDisposition, Bytes, CodecError, CodecErrorKind, DecodeBody, DecodeContext,
    DispatchKnowledge, EncodeBody, EncodeContext, ErrorCategory, Method, StatusBodyDecodeErrorKind,
    StatusBodyState, StatusCode,
};
use concord_test_support::server::{ResponseScript, ScriptedServer};

const RESPONSE_LIMIT: usize = 64;

fn core(server: &ScriptedServer) -> ClientCore {
    build_client_core(
        &server.url("/v1/"),
        default_http_config(),
        default_runtime_config()
            .response_body_limit(RESPONSE_LIMIT)
            .request_body_limit(64),
    )
    .expect("loopback client core")
}

struct GetEndpoint<R, O> {
    id: &'static str,
    accepted: AcceptedStatus,
    marker: PhantomData<fn() -> (R, O)>,
}

impl<R, O> GetEndpoint<R, O> {
    fn new(id: &'static str, accepted: AcceptedStatus) -> Self {
        Self {
            id,
            accepted,
            marker: PhantomData,
        }
    }
}

impl<R, O> EndpointSpec for GetEndpoint<R, O>
where
    R: ResponseRepresentation<O>,
    O: Send + 'static,
{
    type Input = ();
    type Body = EmptyBody;
    type Output = O;
    type Response = R;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new_with_contract(
            self.id,
            Method::GET,
            Some(Duration::from_secs(5)),
            self.accepted.clone(),
            Effect::Safe,
        )
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("items")?;
        builder.push_static("1")?;
        builder.finish()
    }

    fn into_body(self) {}
}

#[cfg(feature = "json")]
mod json_outcomes {
    use concord_extras::codec::{Json, NotFound};
    use serde::Deserialize;

    use super::*;

    #[derive(Debug, Deserialize, Eq, PartialEq)]
    struct Item {
        id: u32,
    }

    #[derive(Debug, Deserialize, Eq, PartialEq)]
    struct Problem {
        code: String,
    }

    fn not_found_accepts() -> AcceptedStatus {
        AcceptedStatus::codes([StatusCode::OK, StatusCode::NOT_FOUND]).expect("status set")
    }

    type FindEndpoint = GetEndpoint<NotFound<Json>, Option<Item>>;

    fn find() -> FindEndpoint {
        GetEndpoint::new("outcomes.find", not_found_accepts())
    }

    #[tokio::test]
    async fn v3_e09_not_found_acquires_the_404_body_and_maps_it_to_none() {
        let server = ScriptedServer::start([
            ResponseScript::ok(br#"{"id":7}"#.to_vec())
                .with_header("Content-Type", "application/json"),
            ResponseScript::status(404, b"<html>missing resource page</html>".to_vec())
                .with_header("Content-Type", "text/html"),
            ResponseScript::status(404, Vec::new()),
        ])
        .expect("loopback server");
        let core = core(&server);

        let present = request(&core, find()).await.expect("present item");
        assert_eq!(present, Some(Item { id: 7 }));

        // A 404 bypasses Json's media check; the body is acquired completely.
        let absent = response_request(request(&core, find()))
            .await
            .expect("404 maps to None");
        assert_eq!(absent.status(), StatusCode::NOT_FOUND);
        assert_eq!(absent.value(), &None);
        assert_eq!(absent.body_disposition(), BodyDisposition::Complete);

        let empty = request(&core, find()).await.expect("empty 404");
        assert_eq!(empty, None);

        let requests = server.requests();
        assert_eq!(requests.len(), 3);
        for recorded in &requests {
            // NotFound forwards the wrapped representation's static Accept.
            assert_eq!(recorded.header("accept"), Some("application/json"));
        }
    }

    #[tokio::test]
    async fn v3_e09_not_found_404_body_still_obeys_limits_and_completeness() {
        let server = ScriptedServer::start([
            ResponseScript::status(404, vec![b'x'; RESPONSE_LIMIT + 1]),
            ResponseScript::Chunked {
                status: 404,
                headers: Vec::new(),
                chunks: vec![vec![b'a'; 40], vec![b'b'; 40]],
            },
            ResponseScript::Truncated {
                status: 404,
                declared_length: 32,
                prefix: b"short".to_vec(),
            },
        ])
        .expect("loopback server");
        let core = core(&server);

        for label in ["known too large", "unknown length over limit", "truncated"] {
            let error = request(&core, find())
                .await
                .expect_err("a buffered 404 body can fail acquisition");
            assert_eq!(error.category(), ErrorCategory::ResponseBody, "{label}");
            assert_eq!(
                error.dispatch_knowledge(),
                DispatchKnowledge::ResponseObserved,
                "{label}"
            );
        }
    }

    #[tokio::test]
    async fn v3_e09_accepted_bytes_bypass_the_decoder_but_keep_status_negotiation_and_limits() {
        let server = ScriptedServer::start([
            ResponseScript::ok(b"<not json at all>".to_vec())
                .with_header("Content-Type", "text/html"),
            ResponseScript::ok(vec![b'z'; RESPONSE_LIMIT + 1]),
            ResponseScript::status(503, b"unavailable".to_vec()),
        ])
        .expect("loopback server");
        let core = core(&server);
        let endpoint = || GetEndpoint::<Json, Item>::new("outcomes.raw", AcceptedStatus::Success);

        let raw = accepted_bytes_request(request(&core, endpoint()))
            .await
            .expect("accepted bytes skip Json validation and decoding");
        assert_eq!(raw.status(), StatusCode::OK);
        assert_eq!(raw.value(), &Bytes::from_static(b"<not json at all>"));
        assert_eq!(raw.body_disposition(), BodyDisposition::Complete);

        let error = accepted_bytes_request(request(&core, endpoint()))
            .await
            .expect_err("accepted bytes keep the buffered limit");
        assert_eq!(error.category(), ErrorCategory::ResponseBody);

        let error = accepted_bytes_request(request(&core, endpoint()))
            .await
            .expect_err("accepted bytes keep endpoint status acceptance");
        assert_eq!(error.category(), ErrorCategory::HttpStatus);
        assert_eq!(error.http_status(), Some(StatusCode::SERVICE_UNAVAILABLE));

        let requests = server.requests();
        assert!(
            requests
                .iter()
                .all(|recorded| recorded.header("accept") == Some("application/json"))
        );

        // The ordinary Json terminal still enforces JSON media before decoding.
        let server = ScriptedServer::start([
            ResponseScript::ok(br#"{"id":1}"#.to_vec()),
            ResponseScript::ok(b"not-json".to_vec())
                .with_header("Content-Type", "application/json"),
        ])
        .expect("loopback server");
        let core = super::core(&server);
        let error = request(&core, endpoint())
            .await
            .expect_err("missing Content-Type is an invalid media type");
        assert_eq!(error.category(), ErrorCategory::ResponseContract);
        let error = request(&core, endpoint())
            .await
            .expect_err("invalid JSON is a conversion error");
        assert_eq!(error.category(), ErrorCategory::ResponseDecoding);
    }

    #[tokio::test]
    async fn v3_e10_status_body_json_decoding_is_explicit_and_does_not_mutate_the_error() {
        let server = ScriptedServer::start([
            ResponseScript::status(422, br#"{"code":"invalid-widget"}"#.to_vec())
                .with_header("Content-Type", "application/problem+json"),
            ResponseScript::status(500, b"<html>oops</html>".to_vec())
                .with_header("Content-Type", "text/html"),
        ])
        .expect("loopback server");
        let core = core(&server);
        let endpoint =
            || GetEndpoint::<Json, Item>::new("outcomes.status", AcceptedStatus::Success);

        let error = request(&core, endpoint()).await.expect_err("rejected 422");
        assert_eq!(error.category(), ErrorCategory::HttpStatus);
        let failure = error.http_response().expect("status failure");
        assert_eq!(failure.body().state(), StatusBodyState::Complete);
        let problem = failure
            .decode_status_body::<Json, Problem>()
            .expect("explicit JSON status-body decoding");
        assert_eq!(
            problem,
            Problem {
                code: "invalid-widget".to_owned()
            }
        );
        // Decoding is repeatable and leaves the original error untouched.
        assert!(failure.decode_status_body::<Json, Problem>().is_ok());
        assert_eq!(error.category(), ErrorCategory::HttpStatus);
        assert_eq!(error.http_status(), Some(StatusCode::UNPROCESSABLE_ENTITY));

        let error = request(&core, endpoint()).await.expect_err("rejected 500");
        let decode = error
            .http_response()
            .expect("status failure")
            .decode_status_body::<Json, Problem>()
            .expect_err("HTML is not JSON media");
        assert_eq!(decode.kind(), StatusBodyDecodeErrorKind::InvalidMediaType);
        assert!(!format!("{decode:?} {decode}").contains("oops"));
        assert_eq!(error.http_status(), Some(StatusCode::INTERNAL_SERVER_ERROR));
    }
}

/// A downstream buffered codec implemented only through the convenience
/// roles; the core blanket bridges supply both full representations.
struct DownstreamCodec;

const DOWNSTREAM_CONTENT_TYPE: &str = "text/x-downstream; charset=utf-8";
const DOWNSTREAM_ACCEPT: &str = "text/x-downstream";

impl EncodeBody for DownstreamCodec {
    type Input = String;
    const CONTENT_TYPE: Option<&'static str> = Some(DOWNSTREAM_CONTENT_TYPE);

    fn encode(input: String, context: EncodeContext<'_>) -> Result<Bytes, CodecError> {
        if context.body_limit.is_some_and(|limit| input.len() > limit) {
            return Err(CodecError::new(CodecErrorKind::Limit));
        }
        Ok(Bytes::from(input.to_uppercase()))
    }
}

impl DecodeBody for DownstreamCodec {
    type Output = String;
    const ACCEPT: Option<&'static str> = Some(DOWNSTREAM_ACCEPT);

    fn validate_content_type(content_type: Option<&str>) -> Result<(), CodecError> {
        if content_type.is_some_and(|value| value.starts_with("text/x-downstream")) {
            Ok(())
        } else {
            Err(CodecError::new(CodecErrorKind::Decode))
        }
    }

    fn decode(body: Bytes, context: DecodeContext<'_>) -> Result<String, CodecError> {
        assert_eq!(context.status, StatusCode::OK);
        String::from_utf8(body.to_vec()).map_err(|_| CodecError::new(CodecErrorKind::Decode))
    }
}

struct PostEndpoint {
    body: String,
}

impl EndpointSpec for PostEndpoint {
    type Input = String;
    type Body = DownstreamCodec;
    type Output = String;
    type Response = DownstreamCodec;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new("outcomes.codec", Method::POST, Some(Duration::from_secs(5)))
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("codec")?;
        builder.finish()
    }

    fn into_body(self) -> String {
        self.body
    }
}

#[tokio::test]
async fn v3_e07_downstream_encode_decode_bridges_execute_with_one_media_source() {
    fn bridges<C>()
    where
        C: RequestRepresentation<String> + ResponseRepresentation<String>,
    {
    }
    bridges::<DownstreamCodec>();
    assert_eq!(
        <DownstreamCodec as RequestRepresentation<String>>::DESCRIPTION.media_hint(),
        Some(DOWNSTREAM_CONTENT_TYPE)
    );
    assert_eq!(
        <DownstreamCodec as ResponseRepresentation<String>>::ACCEPT,
        Some(DOWNSTREAM_ACCEPT)
    );

    let server = ScriptedServer::start([
        ResponseScript::ok(b"echo".to_vec()).with_header("Content-Type", "text/x-downstream")
    ])
    .expect("loopback server");
    let core = core(&server);
    let output = request(
        &core,
        PostEndpoint {
            body: "hello".to_owned(),
        },
    )
    .await
    .expect("downstream codec round trip");
    assert_eq!(output, "echo");
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].header("content-type"),
        Some(DOWNSTREAM_CONTENT_TYPE)
    );
    assert_eq!(requests[0].header("accept"), Some(DOWNSTREAM_ACCEPT));
    assert_eq!(requests[0].body(), b"HELLO");

    // A codec limit failure is a request-body failure before any dispatch.
    let error = request(
        &core,
        PostEndpoint {
            body: "x".repeat(65),
        },
    )
    .await
    .expect_err("codec limit");
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(error.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert_eq!(server.request_count(), 1);
}
