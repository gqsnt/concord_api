//! Independent first-use consumer suite for `concord-sdk-twilio`.
//!
//! Written by an application developer who did not author the SDK, from its README/rustdoc and
//! Concord's public docs only. The provider is a small in-memory fake of the Twilio Messages
//! resource (`2010-04-01`), written here from Twilio's public documentation and served by
//! `concord-example-testserver` on loopback. Every value is synthetic: the credentials, SIDs,
//! request ids and phone numbers below are deliberately not valid Twilio values and nothing is
//! ever sent to a real provider.

use concord::reqwest;
use concord::{ErrorKind, Operation};
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_twilio::{
    AuthError, BasicAuth, CreateMessage, Error, ErrorDocumentProblem, MAX_ERROR_BODY, Message,
    MessageStatus, ProviderError, Response, Twilio,
};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

// ---------------------------------------------------------------------------------------------
// Synthetic identity. None of these resemble real Twilio credentials or SIDs.
// ---------------------------------------------------------------------------------------------

const ACCOUNT: &str = "AC_consumer_account";
const USER: &str = "consumer-user";
const PASS: &str = "consumer-pass-not-real";
/// `base64("consumer-user:consumer-pass-not-real")`, computed independently with coreutils.
const EXPECTED_AUTH: &str = "Basic Y29uc3VtZXItdXNlcjpjb25zdW1lci1wYXNzLW5vdC1yZWFs";
const AUTH_TOKEN_B64: &str = "Y29uc3VtZXItdXNlcjpjb25zdW1lci1wYXNzLW5vdC1yZWFs";

const TO: &str = "+15005550006";
const FROM: &str = "+15005550001";
const TEXT: &str = "Hello café & + 🌍";
/// The form body for `TO`/`FROM`/`TEXT`, written by hand from the
/// `application/x-www-form-urlencoded` rules (space as `+`; `+`, `&` and UTF-8 escaped).
const EXPECTED_FORM: &str =
    "To=%2B15005550006&From=%2B15005550001&Body=Hello+caf%C3%A9+%26+%2B+%F0%9F%8C%8D";

/// Twilio answers JSON with a charset parameter; the fake does the same.
const JSON: &str = "application/json; charset=utf-8";
const MESSAGES_PATH: &str = "/2010-04-01/Accounts/AC_consumer_account/Messages.json";
const REQUEST_ID: &str = "RQ_consumer_request_0001";

// ---------------------------------------------------------------------------------------------
// A fake Twilio Messages resource, written from the public docs (Message resource; Twilio's
// response/exception documents). It validates the same things the docs say Twilio validates for
// this slice and keeps per-message state so a fetch after a create sees a status lifecycle.
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct Observation {
    status: &'static str,
    error_code: Option<i64>,
    error_message: Option<&'static str>,
}

const fn seen(status: &'static str) -> Observation {
    Observation {
        status,
        error_code: None,
        error_message: None,
    }
}

struct Stored {
    sid: String,
    to: String,
    from: String,
    body: String,
    current: Observation,
    upcoming: VecDeque<Observation>,
}

#[derive(Default)]
struct FakeState {
    /// Statuses every new message goes through on successive fetches (after `queued`).
    lifecycle: Vec<Observation>,
    messages: Vec<Stored>,
    /// One-shot replies that pre-empt routing (a provider outage, a broken gateway, ...).
    overrides: VecDeque<Reply>,
}

#[derive(Clone)]
struct FakeTwilio(Arc<Mutex<FakeState>>);

impl FakeTwilio {
    fn new(lifecycle: Vec<Observation>) -> Self {
        Self(Arc::new(Mutex::new(FakeState {
            lifecycle,
            ..FakeState::default()
        })))
    }

    fn push_override(&self, reply: Reply) {
        self.0.lock().unwrap().overrides.push_back(reply);
    }

    fn stored_count(&self) -> usize {
        self.0.lock().unwrap().messages.len()
    }

    async fn serve(&self) -> TestServer {
        let fake = self.clone();
        TestServer::start(move |request| fake.route(request)).await
    }

    fn route(&self, request: &Recorded) -> Reply {
        let mut state = self.0.lock().unwrap();
        if let Some(reply) = state.overrides.pop_front() {
            return reply;
        }
        if request.header_all("authorization") != [EXPECTED_AUTH] {
            return exception(401, 401, Some(20003), "Authenticate");
        }
        let item_prefix = "/2010-04-01/Accounts/AC_consumer_account/Messages/";
        match request.method.as_str() {
            "POST" if request.path() == MESSAGES_PATH => create(&mut state, request),
            "GET" => match request
                .path()
                .strip_prefix(item_prefix)
                .and_then(|rest| rest.strip_suffix(".json"))
            {
                Some(sid) => fetch(&mut state, sid, request.path()),
                None => not_found(request.path()),
            },
            _ => not_found(request.path()),
        }
    }
}

fn create(state: &mut FakeState, request: &Recorded) -> Reply {
    if request.header("content-type") != Some("application/x-www-form-urlencoded") {
        return exception(400, 400, Some(20001), "Request body must be form encoded.");
    }
    let pairs = request.form_pairs();
    let field = |name: &str| {
        pairs
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    };
    let Some(to) = field("To") else {
        return exception(400, 400, Some(21604), "A 'To' phone number is required.");
    };
    let Some(from) = field("From") else {
        return exception(400, 400, Some(21603), "A 'From' phone number is required.");
    };
    let Some(body) = field("Body") else {
        return exception(400, 400, Some(21602), "Message body is required.");
    };
    if !to.starts_with('+') {
        let explanation = format!("The 'To' number {to} is not a valid phone number.");
        return exception(400, 400, Some(21211), &explanation);
    }
    let sid = format!("SM_consumer_{:04}", state.messages.len() + 1);
    let stored = Stored {
        sid,
        to,
        from,
        body,
        current: seen("queued"),
        upcoming: state.lifecycle.iter().cloned().collect(),
    };
    let document = resource(&stored);
    state.messages.push(stored);
    Reply::text(201, JSON, document).header("Twilio-Request-Id", REQUEST_ID)
}

fn fetch(state: &mut FakeState, sid: &str, path: &str) -> Reply {
    let Some(stored) = state.messages.iter_mut().find(|m| m.sid == sid) else {
        return not_found(path);
    };
    if let Some(next) = stored.upcoming.pop_front() {
        stored.current = next;
    }
    Reply::text(200, JSON, resource(stored)).header("Twilio-Request-Id", REQUEST_ID)
}

fn not_found(path: &str) -> Reply {
    let explanation = format!("The requested resource {path} was not found");
    exception(404, 404, Some(20404), &explanation)
}

/// Twilio's exception document: `code`, `message`, `more_info`, `status`.
fn exception(http: u16, claimed: u16, code: Option<i64>, message: &str) -> Reply {
    Reply::text(http, JSON, exception_json(claimed, code, message))
        .header("Twilio-Request-Id", REQUEST_ID)
}

fn exception_json(claimed: u16, code: Option<i64>, message: &str) -> String {
    let more_info = code.map(|c| format!("https://www.twilio.com/docs/errors/{c}"));
    format!(
        r#"{{"code":{},"message":{},"more_info":{},"status":{claimed}}}"#,
        code.map_or("null".to_owned(), |c| c.to_string()),
        js(message),
        js_opt(more_info.as_deref()),
    )
}

/// The Message resource with the documented property set (unrelated fields included, since a
/// real response carries them).
fn resource(m: &Stored) -> String {
    let uri = format!("/2010-04-01/Accounts/{ACCOUNT}/Messages/{}", m.sid);
    format!(
        concat!(
            r#"{{"account_sid":{},"api_version":"2010-04-01","body":{},"#,
            r#""date_created":"Fri, 06 Nov 2026 10:00:00 +0000","date_sent":null,"#,
            r#""date_updated":"Fri, 06 Nov 2026 10:00:01 +0000","direction":"outbound-api","#,
            r#""error_code":{},"error_message":{},"from":{},"messaging_service_sid":null,"#,
            r#""num_media":"0","num_segments":"1","price":null,"price_unit":"USD","#,
            r#""sid":{},"status":{},"subresource_uris":{{"media":{}}},"to":{},"uri":{}}}"#
        ),
        js(ACCOUNT),
        js(&m.body),
        m.current
            .error_code
            .map_or("null".to_owned(), |c| c.to_string()),
        js_opt(m.current.error_message),
        js(&m.from),
        js(&m.sid),
        js(m.current.status),
        js(&format!("{uri}/Media.json")),
        js(&m.to),
        js(&format!("{uri}.json")),
    )
}

/// Minimal JSON string escaping (the SDK's dev-dependencies do not include a JSON library).
fn js(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn js_opt(s: Option<&str>) -> String {
    s.map_or_else(|| "null".to_owned(), js)
}

// ---------------------------------------------------------------------------------------------
// Application-side setup, following the README's construction snippet.
// ---------------------------------------------------------------------------------------------

fn http(timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(timeout)
        .build()
        .expect("reqwest client")
}

fn twilio_with(server: &TestServer, user: &str, pass: &str) -> Twilio {
    Twilio::new(
        http(Duration::from_secs(5)),
        ACCOUNT,
        BasicAuth::new(user, pass).expect("synthetic credentials are well formed"),
    )
    .with_base_url(server.url())
    .expect("loopback base url")
}

fn twilio(server: &TestServer) -> Twilio {
    twilio_with(server, USER, PASS)
}

fn hello() -> CreateMessage {
    CreateMessage::new(TO, FROM, TEXT)
}

/// Strings an application must never see in a log line built from SDK diagnostics.
fn assert_no_leak(shown: &str) {
    for secret in [PASS, AUTH_TOKEN_B64, TO, FROM, "café", "🌍"] {
        assert!(
            !shown.contains(secret),
            "diagnostic leaked {secret:?}: {shown}"
        );
    }
}

fn concord_kind(error: &Error) -> Option<ErrorKind> {
    error.concord().map(concord::Error::kind)
}

fn provider_parts(error: &Error) -> (u16, &Result<ProviderError, ErrorDocumentProblem>) {
    match error {
        Error::Provider { meta, document } => (meta.status.as_u16(), document),
        other => panic!("expected a provider error, got {other:?}"),
    }
}

fn header<'a>(recorded: &'a Recorded, name: &str) -> Option<&'a str> {
    recorded.header(name)
}

// =============================================================================================
// Outcome A: submit a synthetic message.
// =============================================================================================

#[tokio::test]
async fn a_submit_sends_the_documented_form_and_returns_the_queued_resource() {
    let fake = FakeTwilio::new(Vec::new());
    let server = fake.serve().await;
    let client = twilio(&server);

    let submitted: Response<Message> = client.create_message(hello()).await.expect("created");

    // Wire request, as the Twilio docs describe Create a Message.
    let wire = server.single();
    assert_eq!(wire.method, "POST");
    assert_eq!(wire.target, MESSAGES_PATH);
    assert_eq!(header(&wire, "authorization"), Some(EXPECTED_AUTH));
    assert_eq!(
        header(&wire, "content-type"),
        Some("application/x-www-form-urlencoded")
    );
    assert_eq!(header(&wire, "accept"), Some("application/json"));
    assert!(!wire.chunked, "a small form is sent with a content-length");
    assert_eq!(wire.body, EXPECTED_FORM.as_bytes());
    assert_eq!(
        wire.form_pairs(),
        [
            ("To".to_owned(), TO.to_owned()),
            ("From".to_owned(), FROM.to_owned()),
            ("Body".to_owned(), TEXT.to_owned()),
        ]
    );

    // Returned value and metadata.
    assert_eq!(submitted.meta.status.as_u16(), 201);
    assert_eq!(
        submitted.meta.headers().get("twilio-request-id").unwrap(),
        REQUEST_ID
    );
    let operation: Operation = submitted
        .meta
        .operation
        .expect("declared endpoint identity");
    assert!(operation.is_endpoint());
    assert_eq!(operation.method(), Some("POST"));
    // Observed name: the SDK's internal declaration is `TwilioApi`, not the public `Twilio`.
    assert_eq!(operation.name(), "TwilioApi.create_message");
    let message = submitted.value;
    assert_eq!(message.sid, "SM_consumer_0001");
    assert_eq!(message.account_sid, ACCOUNT);
    assert_eq!(message.status, MessageStatus::Queued);
    assert_eq!(message.to.as_deref(), Some(TO));
    assert_eq!(message.from.as_deref(), Some(FROM));
    assert_eq!(message.body.as_deref(), Some(TEXT));
    assert_eq!(message.error_code, None);
    assert_eq!(message.error_message, None);
    assert_eq!(fake.stored_count(), 1);
}

#[tokio::test]
async fn a_message_intent_is_per_message_and_empty_is_not_omitted() {
    let fake = FakeTwilio::new(Vec::new());
    let server = fake.serve().await;
    let client = twilio(&server);

    client
        .create_message(hello().with_message_intent("notifications"))
        .await
        .expect("with intent");
    client
        .create_message(hello())
        .await
        .expect("without intent");
    client
        .create_message(hello().with_message_intent(""))
        .await
        .expect("empty intent");

    let bodies: Vec<String> = server.requests().iter().map(Recorded::body_text).collect();
    assert_eq!(
        bodies,
        [
            format!("{EXPECTED_FORM}&MessageIntent=notifications"),
            EXPECTED_FORM.to_owned(),
            format!("{EXPECTED_FORM}&MessageIntent="),
        ]
    );
}

#[tokio::test]
async fn a_provider_rejection_is_typed_keeps_http_status_and_headers_and_hides_contents() {
    let fake = FakeTwilio::new(Vec::new());
    let server = fake.serve().await;
    let client = twilio(&server);

    // The fake rejects a recipient without `+`, quoting it in the explanation (as Twilio does).
    let error = client
        .create_message(CreateMessage::new("15005550006", FROM, TEXT))
        .await
        .expect_err("invalid recipient");

    let (status, document) = provider_parts(&error);
    assert_eq!(status, 400);
    let document = document.as_ref().expect("Twilio exception document");
    assert_eq!(document.status, 400);
    assert_eq!(document.code, Some(21211));
    assert_eq!(
        document.message,
        "The 'To' number 15005550006 is not a valid phone number."
    );
    assert_eq!(
        document.more_info.as_deref(),
        Some("https://www.twilio.com/docs/errors/21211")
    );
    assert_eq!(error.provider_error(), Some(document));
    let meta = error.meta().expect("a head arrived");
    assert_eq!(meta.headers().get("twilio-request-id").unwrap(), REQUEST_ID);
    assert_eq!(error.operation().and_then(|op| op.method()), Some("POST"));
    assert!(
        error.concord().is_none(),
        "a provider error is not a Concord error"
    );
    assert_eq!(fake.stored_count(), 0);
    assert_eq!(server.requests().len(), 1, "never retried");

    for shown in [error.to_string(), format!("{error:?}")] {
        assert_no_leak(&shown);
        assert!(!shown.contains("15005550006"), "{shown}");
        assert!(!shown.contains("not a valid phone number"), "{shown}");
    }
}

#[tokio::test]
async fn a_wrong_credentials_are_a_401_provider_error() {
    let fake = FakeTwilio::new(Vec::new());
    let server = fake.serve().await;
    let client = twilio_with(&server, USER, "another-pass-not-real");

    let error = client.create_message(hello()).await.expect_err("401");
    let (status, document) = provider_parts(&error);
    assert_eq!(status, 401);
    assert_eq!(document.as_ref().unwrap().code, Some(20003));
    assert!(!error.to_string().contains("another-pass-not-real"));
    assert!(!format!("{error:?}").contains("another-pass-not-real"));
    assert_eq!(fake.stored_count(), 0);
}

#[tokio::test]
async fn a_rate_limit_keeps_retry_after_and_the_real_status_and_is_not_retried() {
    let fake = FakeTwilio::new(Vec::new());
    // The body claims 400 while HTTP says 429: the HTTP status must stay authoritative.
    fake.push_override(
        Reply::text(
            429,
            JSON,
            exception_json(400, Some(20429), "Too Many Requests"),
        )
        .header("Retry-After", "7"),
    );
    let server = fake.serve().await;
    let client = twilio(&server);

    let error = client.create_message(hello()).await.expect_err("429");
    let (status, document) = provider_parts(&error);
    assert_eq!(status, 429);
    let document = document.as_ref().unwrap();
    assert_eq!(
        document.status, 400,
        "the body's claim is kept as data only"
    );
    assert_eq!(document.code, Some(20429));
    assert_eq!(
        error.meta().unwrap().headers().get("retry-after").unwrap(),
        "7"
    );
    assert_eq!(server.requests().len(), 1, "no SDK retry");
}

#[tokio::test]
async fn a_error_bodies_that_are_not_a_twilio_document_are_classified() {
    let html = "<html><body>502 Bad Gateway: upstream unavailable</body></html>";
    let valid = exception_json(503, Some(20503), "Service Unavailable");
    let at_limit = format!("{valid}{}", " ".repeat(MAX_ERROR_BODY - valid.len()));
    let over_limit = format!("{at_limit} ");
    assert_eq!(at_limit.len(), 65_536);

    let cases: Vec<(&str, Reply, u16, Result<i64, ErrorDocumentProblem>)> = vec![
        (
            "gateway html",
            Reply::text(502, "text/html", html),
            502,
            Err(ErrorDocumentProblem::ContentType),
        ),
        (
            "no content type",
            Reply::new(500).body(valid.clone()),
            500,
            Err(ErrorDocumentProblem::ContentType),
        ),
        (
            "message missing",
            Reply::text(400, JSON, r#"{"code":21211,"more_info":null,"status":400}"#),
            400,
            Err(ErrorDocumentProblem::Decode),
        ),
        (
            "not json",
            Reply::text(400, JSON, r#"{"code":21211,"message":"cut"#),
            400,
            Err(ErrorDocumentProblem::Decode),
        ),
        (
            "exactly the limit",
            Reply::text(503, JSON, at_limit),
            503,
            Ok(20503),
        ),
        (
            "one byte over the limit",
            Reply::text(503, JSON, over_limit),
            503,
            Err(ErrorDocumentProblem::TooLarge),
        ),
        (
            "connection dropped inside the error body",
            Reply::new(500)
                .header("content-type", JSON)
                .with_body(Body::FailAfter(vec![
                    br#"{"code":20500,"message":"Inter"#.to_vec().into(),
                ])),
            500,
            Err(ErrorDocumentProblem::Read),
        ),
    ];

    for (name, reply, expected_status, expected) in cases {
        let fake = FakeTwilio::new(Vec::new());
        fake.push_override(reply);
        let server = fake.serve().await;
        let error = twilio(&server)
            .create_message(hello())
            .await
            .expect_err(name);
        let (status, document) = provider_parts(&error);
        assert_eq!(status, expected_status, "{name}");
        let observed = document.as_ref().map(|d| d.code.unwrap()).map_err(|p| *p);
        assert_eq!(observed, expected, "{name}");
        assert_eq!(server.requests().len(), 1, "{name}: never retried");
        assert_no_leak(&error.to_string());
        assert!(
            !error.to_string().contains("upstream unavailable"),
            "{name}"
        );
    }
}

#[tokio::test]
async fn a_lost_response_is_a_transport_error_without_metadata_and_is_not_retried() {
    let fake = FakeTwilio::new(Vec::new());
    fake.push_override(Reply::hang_up());
    let server = fake.serve().await;
    let client = twilio(&server);

    let error = client.create_message(hello()).await.expect_err("hang up");
    assert!(
        matches!(error, Error::Concord { meta: None, .. }),
        "{error:?}"
    );
    assert_eq!(concord_kind(&error), Some(ErrorKind::Transport));
    assert!(error.meta().is_none(), "no fabricated status");
    assert!(error.provider_error().is_none());
    let operation = error.operation().expect("identity without a response");
    assert_eq!(operation.method(), Some("POST"));
    // The request reached the server: the application must treat the write as unknown.
    assert_eq!(server.requests().len(), 1, "not retried");
    assert_no_leak(&error.to_string());
    assert_no_leak(&format!("{error:?}"));
}

#[tokio::test]
async fn a_success_status_with_a_non_json_body_keeps_its_metadata() {
    let fake = FakeTwilio::new(Vec::new());
    fake.push_override(Reply::text(201, "text/html", "<p>created</p>"));
    let server = fake.serve().await;

    let error = twilio(&server)
        .create_message(hello())
        .await
        .expect_err("refused content type");
    assert_eq!(concord_kind(&error), Some(ErrorKind::Metadata));
    let meta = error.meta().expect("the 201 head arrived");
    assert_eq!(meta.status.as_u16(), 201);
    assert_eq!(meta.headers().get("content-type").unwrap(), "text/html");
    assert!(error.provider_error().is_none());
}

#[tokio::test]
async fn a_cut_off_success_body_is_an_error_not_a_partial_message() {
    let fake = FakeTwilio::new(Vec::new());
    fake.push_override(
        Reply::new(201)
            .header("content-type", JSON)
            .with_body(Body::FailAfter(vec![
                br#"{"account_sid":"AC_consumer_account","sid":"SM_consumer_0001","#
                    .to_vec()
                    .into(),
            ])),
    );
    let server = fake.serve().await;

    let error = twilio(&server)
        .create_message(hello())
        .await
        .expect_err("truncated resource");
    assert!(matches!(error, Error::Concord { .. }), "{error:?}");
    // Observed (my prediction allowed Transport or Decode): a body that stops before EOF is a
    // transport failure while reading the body, with the 201 head kept.
    assert_eq!(concord_kind(&error), Some(ErrorKind::Transport));
    assert_eq!(error.meta().expect("head arrived").status.as_u16(), 201);
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn a_malformed_credentials_are_rejected_before_any_client_exists() {
    assert_eq!(BasicAuth::new("", PASS).unwrap_err(), AuthError::Username);
    assert_eq!(
        BasicAuth::new("consumer:user", PASS).unwrap_err(),
        AuthError::Username
    );
    assert_eq!(
        BasicAuth::new("consumer\nuser", PASS).unwrap_err(),
        AuthError::Username
    );
    assert_eq!(BasicAuth::new(USER, "").unwrap_err(), AuthError::Password);
    assert_eq!(
        BasicAuth::new(USER, "pass\rnot-real").unwrap_err(),
        AuthError::Password
    );
    // A colon in the password is allowed (it is after the first colon of user:pass).
    assert!(BasicAuth::new(USER, "pass:not-real").is_ok());

    // The diagnostic names the problem, never the rejected value.
    let rejected = BasicAuth::new(USER, "zebra-sentinel\u{7}").unwrap_err();
    assert_eq!(rejected, AuthError::Password);
    for shown in [rejected.to_string(), format!("{rejected:?}")] {
        assert!(!shown.contains("zebra-sentinel"), "{shown}");
    }
}

#[tokio::test]
async fn a_client_and_inputs_do_not_print_secrets_or_contents() {
    let fake = FakeTwilio::new(Vec::new());
    let server = fake.serve().await;
    let client = twilio(&server);
    let auth = BasicAuth::new(USER, PASS).unwrap();

    assert_no_leak(&format!("{client:?}"));
    assert_no_leak(&format!("{auth:?}"));
    assert_no_leak(&format!("{:?}", hello()));

    let submitted = client.create_message(hello()).await.unwrap();
    assert_no_leak(&format!("{:?}", submitted.meta));
    assert_eq!(server.requests().len(), 1);
}

// =============================================================================================
// Outcome B: retrieve message status and expose its processing error.
// =============================================================================================

/// What an application might derive from one observation. Not a delivery guarantee: Twilio's
/// docs describe `sent` as carrier acceptance only.
#[derive(Debug, PartialEq)]
enum Report {
    Pending(MessageStatus),
    Delivered,
    NotDelivered {
        code: Option<i64>,
        explanation: Option<String>,
    },
    Unrecognized(String),
}

fn report(message: &Message) -> Report {
    match &message.status {
        MessageStatus::Delivered => Report::Delivered,
        MessageStatus::Failed | MessageStatus::Undelivered => Report::NotDelivered {
            code: message.error_code,
            explanation: message.error_message.clone(),
        },
        MessageStatus::Unknown(spelling) => Report::Unrecognized(spelling.clone()),
        other => Report::Pending(other.clone()),
    }
}

fn item_path(sid: &str) -> String {
    format!("/2010-04-01/Accounts/{ACCOUNT}/Messages/{sid}.json")
}

fn assert_fetch_wire(wire: &Recorded, sid_in_path: &str) {
    assert_eq!(wire.method, "GET");
    assert_eq!(wire.target, item_path(sid_in_path));
    assert_eq!(header(wire, "authorization"), Some(EXPECTED_AUTH));
    assert_eq!(header(wire, "accept"), Some("application/json"));
    assert_eq!(header(wire, "content-type"), None);
    assert!(wire.body.is_empty());
    assert!(!wire.chunked);
}

#[tokio::test]
async fn b_fetch_by_the_returned_sid_follows_the_status_lifecycle() {
    let fake = FakeTwilio::new(vec![seen("sending"), seen("sent"), seen("delivered")]);
    let server = fake.serve().await;
    let client = twilio(&server);

    let submitted = client.create_message(hello()).await.expect("created");
    let sid = submitted.value.sid.clone();

    let mut observed = Vec::new();
    for _ in 0..4 {
        let fetched = client.get_message(&sid).await.expect("fetched");
        assert_eq!(fetched.meta.status.as_u16(), 200);
        assert_eq!(fetched.value.sid, sid);
        assert_eq!(fetched.value.error_code, None);
        observed.push(report(&fetched.value));
    }
    assert_eq!(
        observed,
        [
            Report::Pending(MessageStatus::Sending),
            Report::Pending(MessageStatus::Sent),
            Report::Delivered,
            Report::Delivered,
        ]
    );

    let requests = server.requests();
    assert_eq!(requests.len(), 5, "one POST and exactly one GET per call");
    for wire in &requests[1..] {
        assert_fetch_wire(wire, "SM_consumer_0001");
    }
    let operation = client
        .get_message(&sid)
        .await
        .unwrap()
        .meta
        .operation
        .unwrap();
    assert_eq!(operation.method(), Some("GET"));
    assert_eq!(operation.name(), "TwilioApi.get_message");
}

#[tokio::test]
async fn b_processing_failures_are_successful_fetches_that_expose_the_error() {
    let cases = [
        (
            Observation {
                status: "undelivered",
                error_code: Some(30003),
                error_message: Some("Unreachable destination handset"),
            },
            MessageStatus::Undelivered,
        ),
        (
            Observation {
                status: "failed",
                error_code: Some(30007),
                error_message: Some("Message filtered"),
            },
            MessageStatus::Failed,
        ),
    ];
    for (observation, expected) in cases {
        let fake = FakeTwilio::new(vec![seen("sending"), observation.clone()]);
        let server = fake.serve().await;
        let client = twilio(&server);
        let sid = client.create_message(hello()).await.unwrap().value.sid;

        let first = client.get_message(&sid).await.unwrap();
        assert_eq!(
            report(&first.value),
            Report::Pending(MessageStatus::Sending)
        );

        let last = client
            .get_message(&sid)
            .await
            .expect("an HTTP 200 carrying a failure is not an Err");
        assert_eq!(last.meta.status.as_u16(), 200);
        assert_eq!(last.value.status, expected);
        assert_eq!(
            report(&last.value),
            Report::NotDelivered {
                code: observation.error_code,
                explanation: observation.error_message.map(str::to_owned),
            }
        );
        assert_eq!(server.requests().len(), 3);
    }
}

#[tokio::test]
async fn b_every_documented_status_and_a_future_one_decode() {
    let documented = [
        ("accepted", MessageStatus::Accepted),
        ("scheduled", MessageStatus::Scheduled),
        ("queued", MessageStatus::Queued),
        ("sending", MessageStatus::Sending),
        ("sent", MessageStatus::Sent),
        ("failed", MessageStatus::Failed),
        ("delivered", MessageStatus::Delivered),
        ("undelivered", MessageStatus::Undelivered),
        ("receiving", MessageStatus::Receiving),
        ("received", MessageStatus::Received),
        ("read", MessageStatus::Read),
        ("partially_delivered", MessageStatus::PartiallyDelivered),
        ("canceled", MessageStatus::Canceled),
        (
            "rerouted_future_state",
            MessageStatus::Unknown("rerouted_future_state".to_owned()),
        ),
        // Spelling is kept exactly; no case folding into a known value.
        ("Delivered", MessageStatus::Unknown("Delivered".to_owned())),
    ];
    let lifecycle = documented.iter().map(|(wire, _)| seen(wire)).collect();
    let fake = FakeTwilio::new(lifecycle);
    let server = fake.serve().await;
    let client = twilio(&server);
    let sid = client.create_message(hello()).await.unwrap().value.sid;

    for (wire, expected) in documented {
        let fetched = client.get_message(&sid).await.expect(wire);
        assert_eq!(fetched.value.status, expected, "{wire}");
    }
    assert_eq!(
        report(&client.get_message(&sid).await.unwrap().value),
        Report::Unrecognized("Delivered".to_owned())
    );
}

#[tokio::test]
async fn b_unknown_sid_is_a_404_provider_error_without_echoing_the_path() {
    let fake = FakeTwilio::new(Vec::new());
    let server = fake.serve().await;

    let error = twilio(&server)
        .get_message("SM_consumer_9999")
        .await
        .expect_err("404");
    let (status, document) = provider_parts(&error);
    assert_eq!(status, 404);
    let document = document.as_ref().unwrap();
    assert_eq!(document.code, Some(20404));
    assert!(document.message.contains("SM_consumer_9999"));
    assert_fetch_wire(&server.single(), "SM_consumer_9999");
    for shown in [error.to_string(), format!("{error:?}")] {
        assert!(!shown.contains("SM_consumer_9999"), "{shown}");
        assert!(shown.contains("404"), "{shown}");
    }
}

#[tokio::test]
async fn b_a_sid_is_always_one_path_segment() {
    let fake = FakeTwilio::new(Vec::new());
    let server = fake.serve().await;
    let client = twilio(&server);
    // An application passing through an untrusted SID must not reach another resource.
    let cases = [
        ("SM/../../Messages", "SM%2F..%2F..%2FMessages"),
        ("SM?x#f g", "SM%3Fx%23f%20g"),
        // `{sid}.json` is one segment, so `..` becomes `...json`, not a dot-segment.
        ("..", ".."),
        // A percent sign is data, not a pre-encoded escape.
        ("%2F", "%252F"),
    ];
    for (sid, _) in cases {
        let error = client.get_message(sid).await.expect_err(sid);
        assert_eq!(provider_parts(&error).0, 404, "{sid}");
    }
    // Post-wave-1 integration (coordinator): this row used to observe the documented behaviour
    // "an empty SID is sent as `Messages/.json`" (a 404 from the fake). The SDK now refuses an
    // empty SID before sending, as this suite's FIRST_USE.md friction 4 suggested.
    let empty = client.get_message("").await.expect_err("empty SID");
    assert!(matches!(empty, Error::EmptySid { .. }), "{empty:?}");
    let requests = server.requests();
    assert_eq!(requests.len(), cases.len());
    for (wire, (_, encoded)) in requests.iter().zip(cases) {
        assert_fetch_wire(wire, encoded);
    }
}

#[tokio::test]
async fn b_invalid_resource_documents_are_decode_errors_with_metadata() {
    let cases = [
        (
            "status missing",
            r#"{"sid":"SM_consumer_0001","account_sid":"AC_consumer_account"}"#,
        ),
        (
            "sid missing",
            r#"{"account_sid":"AC_consumer_account","status":"sent"}"#,
        ),
        (
            "error_code as a string",
            r#"{"sid":"SM_consumer_0001","account_sid":"AC_consumer_account","status":"failed","error_code":"30007"}"#,
        ),
        (
            "status as a number",
            r#"{"sid":"SM_consumer_0001","account_sid":"AC_consumer_account","status":3}"#,
        ),
        ("cut-off json", r#"{"sid":"SM_consumer_0001","status":"se"#),
        ("empty body", ""),
    ];
    for (name, body) in cases {
        let fake = FakeTwilio::new(Vec::new());
        fake.push_override(Reply::text(200, JSON, body));
        let server = fake.serve().await;
        let error = twilio(&server)
            .get_message("SM_consumer_0001")
            .await
            .expect_err(name);
        assert!(matches!(error, Error::Concord { .. }), "{name}: {error:?}");
        assert_eq!(concord_kind(&error), Some(ErrorKind::Decode), "{name}");
        let meta = error.meta().expect("head arrived");
        assert_eq!(meta.status.as_u16(), 200, "{name}");
        assert_eq!(meta.headers().get("content-type").unwrap(), JSON, "{name}");
        assert!(error.provider_error().is_none(), "{name}");
        let shown = error.to_string();
        assert!(!shown.contains("SM_consumer_0001"), "{name}: {shown}");
        assert!(!shown.contains("30007"), "{name}: {shown}");
        assert_eq!(server.requests().len(), 1, "{name}");
    }
}

#[tokio::test]
async fn b_absent_and_null_optional_fields_are_none() {
    let fake = FakeTwilio::new(Vec::new());
    fake.push_override(Reply::text(
        200,
        JSON,
        r#"{"sid":"SM_consumer_0001","account_sid":"AC_consumer_account","status":"delivered","body":null,"to":null}"#,
    ));
    let server = fake.serve().await;

    let fetched = twilio(&server)
        .get_message("SM_consumer_0001")
        .await
        .expect("minimal resource");
    assert_eq!(
        fetched.value,
        Message {
            sid: "SM_consumer_0001".to_owned(),
            account_sid: ACCOUNT.to_owned(),
            status: MessageStatus::Delivered,
            body: None,
            from: None,
            to: None,
            error_code: None,
            error_message: None,
        }
    );
}

#[tokio::test]
async fn b_a_stalled_fetch_ends_with_the_client_timeout_and_keeps_the_head() {
    let fake = FakeTwilio::new(Vec::new());
    fake.push_override(
        Reply::new(200)
            .header("content-type", JSON)
            .with_body(Body::Stall),
    );
    // Head never arrives in time: no metadata.
    fake.push_override(Reply::text(200, JSON, "{}").delay(Duration::from_secs(3)));
    let server = fake.serve().await;
    let client = Twilio::new(
        http(Duration::from_millis(300)),
        ACCOUNT,
        BasicAuth::new(USER, PASS).unwrap(),
    )
    .with_base_url(server.url())
    .unwrap();

    let stalled = tokio::time::timeout(Duration::from_secs(5), client.get_message("SM_a"))
        .await
        .expect("the client timeout ends the call")
        .expect_err("stalled body");
    assert_eq!(concord_kind(&stalled), Some(ErrorKind::Transport));
    assert_eq!(stalled.meta().expect("head arrived").status.as_u16(), 200);

    let late = tokio::time::timeout(Duration::from_secs(5), client.get_message("SM_b"))
        .await
        .expect("the client timeout ends the call")
        .expect_err("late head");
    assert_eq!(concord_kind(&late), Some(ErrorKind::Transport));
    assert!(late.meta().is_none());
    assert_eq!(late.operation().unwrap().name(), "TwilioApi.get_message");
    assert_eq!(server.requests().len(), 2, "no retry after either timeout");
}

#[tokio::test]
async fn b_create_succeeded_then_fetch_failed_keeps_the_sid_for_a_later_fetch() {
    let fake = FakeTwilio::new(vec![seen("delivered")]);
    let server = fake.serve().await;
    let client = twilio(&server);

    let submitted = client.create_message(hello()).await.expect("created");
    // The provider has an outage on the next request only.
    fake.push_override(Reply::text(
        503,
        "text/html",
        "<h1>Service Unavailable</h1>",
    ));
    let error = client
        .get_message(&submitted.value.sid)
        .await
        .expect_err("outage");
    let (status, document) = provider_parts(&error);
    assert_eq!(status, 503);
    assert_eq!(document, &Err(ErrorDocumentProblem::ContentType));

    // The application still owns the created resource and decides to fetch again later.
    assert_eq!(submitted.value.status, MessageStatus::Queued);
    let fetched = client
        .get_message(&submitted.value.sid)
        .await
        .expect("later fetch");
    assert_eq!(report(&fetched.value), Report::Delivered);

    let methods: Vec<String> = server.requests().iter().map(|r| r.method.clone()).collect();
    assert_eq!(
        methods,
        ["POST", "GET", "GET"],
        "no second POST, no hidden retry"
    );
    assert_eq!(fake.stored_count(), 1);
}

#[tokio::test]
async fn b_a_cloned_client_in_another_task_fetches_with_the_same_account_and_auth() {
    let fake = FakeTwilio::new(vec![seen("sent")]);
    let server = fake.serve().await;
    let client = twilio(&server);
    let sid = client.create_message(hello()).await.unwrap().value.sid;

    let worker = client.clone();
    let handle = tokio::spawn(async move { worker.get_message(sid).await });
    let fetched = handle.await.unwrap().expect("fetched from a task");
    assert_eq!(fetched.value.status, MessageStatus::Sent);
    assert_fetch_wire(&server.requests()[1], "SM_consumer_0001");
}

// =============================================================================================
// Construction details an application meets first.
// =============================================================================================

#[tokio::test]
async fn construction_base_url_variants() {
    let fake = FakeTwilio::new(Vec::new());
    let server = fake.serve().await;
    let auth = || BasicAuth::new(USER, PASS).unwrap();
    let new = || Twilio::new(http(Duration::from_secs(5)), ACCOUNT, auth());

    let slash = new().with_base_url(format!("{}/", server.url())).unwrap();
    slash.create_message(hello()).await.expect("trailing slash");
    let proxied = new()
        .with_base_url(format!("{}/twilio-proxy", server.url()))
        .unwrap();
    // The fake only knows the unprefixed path, so the prefixed request is a provider 404.
    let error = proxied.create_message(hello()).await.unwrap_err();
    assert_eq!(provider_parts(&error).0, 404);

    let targets: Vec<String> = server.requests().iter().map(|r| r.target.clone()).collect();
    assert_eq!(
        targets,
        [
            MESSAGES_PATH.to_owned(),
            format!("/twilio-proxy{MESSAGES_PATH}"),
        ]
    );

    for bad in [
        format!("{}/?region=x", server.url()),
        server.url().replace("http://", "http://user:pw@"),
        "not a url".to_owned(),
    ] {
        let rejected = new().with_base_url(&bad).expect_err(&bad);
        assert_eq!(rejected.kind(), ErrorKind::Build, "{bad}");
        assert!(!rejected.to_string().contains("pw@"), "{rejected}");
    }
    assert_eq!(server.requests().len(), 2, "rejected bases send nothing");
}
