use bytes::Bytes;
use concord_core::advanced::{
    AdvancedRequestBody, ApiOriginDescriptor, AuthChallengeMode, AuthError, AuthFuture,
    AuthPreparationMode, AuthProviderBinding, CredentialContext, CredentialId, CredentialProvider,
    CredentialProviderState, FixedOriginDescriptor, InvalidateReason, OctetStream, OriginScheme,
    PreparedBody, PreparedEndpoint, PreparedRequestEntity, PreparedStreamEndpoint,
    RequestAuthentication, RequestEntity, SafeProxy,
};
use concord_core::prelude::{
    ApiClient, ApiClientError, ApiKey, ClientContext, RequestExecutionMeta, RetryMode, Text,
};
use http::{HeaderMap, Method, StatusCode};
use http_body::{Body, Frame, SizeHint};
use std::convert::Infallible;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::thread;
use std::time::Duration;

struct LocalRequestEntity;

impl RequestEntity for LocalRequestEntity {
    type Input = PreparedBody;

    fn prepare(
        body: Self::Input,
        _ctx: concord_core::advanced::ErrorContext,
    ) -> Result<PreparedRequestEntity, ApiClientError> {
        Ok(PreparedRequestEntity { body })
    }
}

struct LocalBody {
    bytes: Option<Bytes>,
}

impl LocalBody {
    fn new(bytes: &'static [u8]) -> Self {
        Self {
            bytes: Some(Bytes::from_static(bytes)),
        }
    }
}

impl Body for LocalBody {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        Poll::Ready(self.bytes.take().map(Frame::data).map(Ok))
    }

    fn is_end_stream(&self) -> bool {
        self.bytes.is_none()
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::with_exact(self.bytes.as_ref().map_or(0, |bytes| bytes.len()) as u64)
    }
}

#[derive(Clone)]
struct PublicContext;

#[derive(Clone)]
struct PublicAuthVars {
    acquired: Arc<AtomicUsize>,
    invalidated: Arc<AtomicUsize>,
}

#[derive(Clone)]
struct PublicAuthState {
    provider: Arc<CredentialProviderState<PublicContext, PublicProvider>>,
}

impl ClientContext for PublicContext {
    type Vars = ();
    type AuthVars = PublicAuthVars;
    type AuthState = PublicAuthState;

    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTP;
    const DOMAIN: &'static str = "example.com";
    const ORIGIN: ApiOriginDescriptor =
        ApiOriginDescriptor::FixedSingleOrigin(FixedOriginDescriptor {
            scheme: OriginScheme::Http,
            authority: "example.com",
        });

    fn init_auth_state(_vars: &Self::Vars, auth: &Self::AuthVars) -> Self::AuthState {
        PublicAuthState {
            provider: Arc::new(CredentialProviderState::new(PublicProvider {
                acquired: auth.acquired.clone(),
                invalidated: auth.invalidated.clone(),
            })),
        }
    }

    fn auth_provider_binding<'a>(
        credential: &CredentialId,
        state: &'a Self::AuthState,
    ) -> Option<AuthProviderBinding<'a, Self>> {
        (credential == &CredentialId::new("public", "token")).then(|| {
            state.provider.secret_binding(
                AuthPreparationMode::PerExecution,
                AuthChallengeMode::Refresh,
            )
        })
    }
}

#[derive(Clone)]
struct PublicProvider {
    acquired: Arc<AtomicUsize>,
    invalidated: Arc<AtomicUsize>,
}

impl CredentialProvider<PublicContext> for PublicProvider {
    type Credential = ApiKey;

    fn id(&self) -> CredentialId {
        CredentialId::new("public", "token")
    }

    fn acquire<'a>(
        &'a self,
        _ctx: CredentialContext<'a, PublicContext>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move {
            let generation = self.acquired.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(ApiKey::new(format!("fixture-token-{generation}")))
        })
    }

    fn invalidate<'a>(
        &'a self,
        _ctx: CredentialContext<'a, PublicContext>,
        _current: Option<&'a Self::Credential>,
        _reason: InvalidateReason,
    ) -> AuthFuture<'a, Result<(), AuthError>> {
        Box::pin(async move {
            self.invalidated.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }
}

struct Reply {
    status: StatusCode,
    content_type: &'static str,
    body: &'static [u8],
}

impl Reply {
    fn text(status: StatusCode, body: &'static [u8]) -> Self {
        Self {
            status,
            content_type: "text/plain",
            body,
        }
    }

    fn bytes(body: &'static [u8]) -> Self {
        Self {
            status: StatusCode::OK,
            content_type: "application/octet-stream",
            body,
        }
    }
}

#[derive(Clone, Debug)]
struct CapturedRequest {
    headers: HeaderMap,
    body: Bytes,
}

struct ScriptedServer {
    address: String,
    requests: Arc<Mutex<Vec<CapturedRequest>>>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl ScriptedServer {
    fn start(replies: Vec<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback bind");
        listener
            .set_nonblocking(true)
            .expect("nonblocking loopback");
        let address = listener.local_addr().expect("loopback address").to_string();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let thread = thread::spawn(move || {
            for reply in replies {
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            if thread_stop.load(Ordering::Acquire) {
                                return;
                            }
                            thread::sleep(Duration::from_millis(2));
                        }
                        Err(error) => panic!("loopback accept failed: {error}"),
                    }
                };
                stream
                    .set_nonblocking(false)
                    .expect("blocking request stream");
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .expect("request timeout");
                let request = read_request(&mut stream);
                captured.lock().expect("captured requests").push(request);
                write_reply(&mut stream, reply);
            }
        });
        Self {
            address,
            requests,
            stop,
            thread: Some(thread),
        }
    }

    fn proxy(&self) -> SafeProxy {
        let url = format!("http://{}", self.address);
        SafeProxy::all(&url).expect("safe loopback proxy")
    }

    fn finish(mut self) -> Vec<CapturedRequest> {
        self.join();
        self.requests.lock().expect("captured requests").clone()
    }

    fn join(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let result = thread.join();
            if !thread::panicking() {
                result.expect("loopback thread");
            }
        }
    }
}

impl Drop for ScriptedServer {
    fn drop(&mut self) {
        self.join();
    }
}

fn read_request(stream: &mut TcpStream) -> CapturedRequest {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).expect("request line");
    assert!(!request_line.is_empty(), "request line");
    let mut headers = HeaderMap::new();
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).expect("request header");
        if line == "\r\n" {
            break;
        }
        let (name, value) = line.trim_end().split_once(':').expect("header syntax");
        let name = http::HeaderName::from_bytes(name.as_bytes()).expect("header name");
        let value = http::HeaderValue::from_str(value.trim()).expect("header value");
        if name == http::header::CONTENT_LENGTH {
            content_length = value
                .to_str()
                .expect("content length text")
                .parse()
                .expect("content length number");
        }
        headers.append(name, value);
    }
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body).expect("request body");
    CapturedRequest {
        headers,
        body: Bytes::from(body),
    }
}

fn write_reply(stream: &mut TcpStream, reply: Reply) {
    let reason = reply.status.canonical_reason().unwrap_or("Response");
    write!(
        stream,
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        reply.status.as_u16(),
        reason,
        reply.content_type,
        reply.body.len()
    )
    .expect("response head");
    stream.write_all(reply.body).expect("response body");
    stream.flush().expect("response flush");
}

fn advanced_body(bytes: &'static [u8]) -> AdvancedRequestBody {
    AdvancedRequestBody::new(LocalBody::new(bytes))
}

fn endpoint(
    name: &'static str,
    path: &'static str,
    body: PreparedBody,
) -> PreparedEndpoint<Text<String>> {
    PreparedEndpoint::from_request_entity::<LocalRequestEntity>(name, Method::POST, path, body)
        .expect("public request entity")
}

#[tokio::test]
async fn downstream_public_extension_executes_without_generated_integration_modules() {
    let server = ScriptedServer::start(vec![
        Reply::text(StatusCode::OK, b"one-shot"),
        Reply::text(StatusCode::OK, b"factory"),
        Reply::text(StatusCode::UNAUTHORIZED, b"challenge"),
        Reply::text(StatusCode::OK, b"authenticated"),
        Reply::text(StatusCode::OK, b"metadata"),
        Reply::bytes(b"stream"),
    ]);
    let acquired = Arc::new(AtomicUsize::new(0));
    let invalidated = Arc::new(AtomicUsize::new(0));
    let proxy = server.proxy();
    let client = ApiClient::<PublicContext>::with_reqwest_builder_and_retry_mode(
        (),
        PublicAuthVars {
            acquired: acquired.clone(),
            invalidated: invalidated.clone(),
        },
        RetryMode::status(1, [StatusCode::SERVICE_UNAVAILABLE]).expect("status retry mode"),
        |builder| Ok(builder.proxy(proxy)),
    )
    .expect("fixed-origin managed client");

    let one_shot = PreparedBody::one_shot(advanced_body(b"one-shot-body"), None);
    assert_eq!(
        endpoint("OneShot", "/one-shot", one_shot)
            .execute(&client)
            .await
            .expect("one-shot endpoint"),
        "one-shot"
    );

    let factory_calls = Arc::new(AtomicUsize::new(0));
    let observed = factory_calls.clone();
    let factory = PreparedBody::factory(SizeHint::with_exact(12), None, move || {
        observed.fetch_add(1, Ordering::SeqCst);
        Ok(advanced_body(b"factory-body"))
    });
    assert_eq!(
        endpoint("Factory", "/factory", factory)
            .execute(&client)
            .await
            .expect("factory endpoint"),
        "factory"
    );
    assert_eq!(factory_calls.load(Ordering::SeqCst), 1);

    let recovery_calls = Arc::new(AtomicUsize::new(0));
    let observed = recovery_calls.clone();
    let recovery_body = PreparedBody::factory(SizeHint::with_exact(13), None, move || {
        observed.fetch_add(1, Ordering::SeqCst);
        Ok(advanced_body(b"recovery-body"))
    });
    let authenticated = endpoint("Authenticated", "/authenticated", recovery_body)
        .authentication(RequestAuthentication::bearer(CredentialId::new(
            "public", "token",
        )))
        .execute(&client)
        .await
        .expect("one bounded authentication recovery");
    assert_eq!(authenticated, "authenticated");
    assert_eq!(recovery_calls.load(Ordering::SeqCst), 2);
    assert_eq!(acquired.load(Ordering::SeqCst), 2);
    assert_eq!(invalidated.load(Ordering::SeqCst), 1);

    let decoded = endpoint(
        "BufferedMetadata",
        "/buffered-metadata",
        PreparedBody::empty(),
    )
    .response(&client)
    .await
    .expect("buffered metadata");
    let buffered_meta: &RequestExecutionMeta = decoded.meta();
    assert_eq!(buffered_meta.endpoint, "BufferedMetadata");

    let mut streamed = PreparedStreamEndpoint::<OctetStream>::new(
        "StreamMetadata",
        Method::GET,
        "/stream-metadata",
        PreparedRequestEntity {
            body: PreparedBody::empty(),
        },
    )
    .execute(&client)
    .await
    .expect("stream metadata");
    let stream_meta: &RequestExecutionMeta = streamed.meta();
    assert_eq!(stream_meta.endpoint, "StreamMetadata");
    assert_eq!(
        streamed.next_chunk().await.expect("stream chunk"),
        Some(Bytes::from_static(b"stream"))
    );

    let requests = server.finish();
    assert_eq!(requests.len(), 6);
    assert_eq!(requests[0].body, Bytes::from_static(b"one-shot-body"));
    assert_eq!(requests[1].body, Bytes::from_static(b"factory-body"));
    assert_eq!(requests[2].body, Bytes::from_static(b"recovery-body"));
    assert_eq!(requests[3].body, Bytes::from_static(b"recovery-body"));
    assert_eq!(
        requests[2]
            .headers
            .get(http::header::AUTHORIZATION)
            .expect("first authorization"),
        "Bearer fixture-token-1"
    );
    assert_eq!(
        requests[3]
            .headers
            .get(http::header::AUTHORIZATION)
            .expect("second authorization"),
        "Bearer fixture-token-2"
    );
}
