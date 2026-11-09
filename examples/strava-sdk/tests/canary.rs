//! Secrets: OAuth client secret, access token, refresh token and authorization code each get
//! distinct canaries. They travel in OAuth forms and the API bearer header; an expired session
//! forces refresh, while a live session sends its initial access token. Numeric client_id
//! cannot hold an ASCII canary: 123 is used and Credentials Debug is still checked.
//! Distinct canaries also cover withheld ActivityUpdate strings and UploadFile filename,
//! body, external_id, name and sport_type. Read, update write and OAuth cover all seven cases;
//! upload has a successful wire control. No wire exemptions.
//! Excludes native reqwest, *_unredacted and clear Activity model fields. Tokens, sessions,
//! generated configs/handles, Prepared, reports, provider errors and metadata are formatted.

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Reply, TestServer};
use std::time::Duration;

fn http() -> concord::reqwest::Client {
    concord::reqwest::Client::builder()
        .no_proxy()
        .redirect(concord::reqwest::redirect::Policy::none())
        .retry(concord::reqwest::retry::never())
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap()
}

fn refused() -> String {
    concord_example_testserver::refused_url()
}

// Echo credentials in headers and in diagnostics whose formatting is withheld.
fn reply(case: usize, success: &str, problem: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::json(200, success),
        1 => Reply::json(401, problem),
        2 => Reply::json(500, problem),
        3 => Reply::json(200, format!("{{invalid {echo}")),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::json(200, "{}").with_body(Body::FailAfter(vec![concord::bytes::Bytes::from(
            format!("{{\"incomplete\":\"{echo}"),
        )])),
        _ => unreachable!(),
    };
    response
        .header("x-request-id", echo)
        .header("set-cookie", echo)
}

async fn receive<T>(c: &mut Canaries, prepared: concord::Prepared<T>) -> Result<T, concord::Error> {
    let result = prepared.receive().await;
    c.assert_debug_absent("Received Result", &result);
    match result {
        Ok(received) => {
            c.assert_debug_absent("response head", &received.head());
            c.assert_debug_absent("response metadata", &received.meta());
            received.decode().await
        }
        Err(error) => Err(error),
    }
}

fn concord_error(c: &mut Canaries, error: &concord::Error) {
    c.assert_debug_absent("concord::Error", error);
    c.assert_error_absent("concord::Error and sources", error);
    if let Some(meta) = error.head_meta() {
        c.assert_debug_absent("error response metadata", &meta);
    }
}

use concord_sdk_strava::{
    ActivityUpdate, Credentials, DataType, Error, OAuth, Session, StravaConfig, TokenRequest,
    Tokens, UploadFile,
};

fn tokens(access: &str, refresh: &str, expired: bool) -> Tokens {
    Tokens {
        token_type: "Bearer".into(),
        access_token: access.into(),
        refresh_token: refresh.into(),
        expires_at: if expired { 0 } else { u64::MAX },
        expires_in: 3600,
        scope: Some("activity:read_all activity:write".into()),
    }
}

fn error(c: &mut Canaries, error: &Error) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    match error {
        Error::Concord(e) | Error::Ambiguous(e) => concord_error(c, e),
        Error::Http(http) => {
            c.assert_debug_absent("HTTP failure", http);
            c.assert_debug_absent("provider document", &http.document);
            c.assert_debug_absent("metadata", &http.meta);
            c.assert_debug_absent("quota report", &http.rates);
            if let Err(e) = &http.document {
                concord_error(c, e);
            }
        }
        _ => {}
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("strava");
    let secret = c.issue();
    let access = c.issue();
    let refresh = c.issue();
    let code = c.issue();
    let changes = ActivityUpdate::new()
        .name(c.issue())
        .sport_type(c.issue())
        .description(c.issue())
        .gear_id(c.issue());
    let filename = c.issue();
    let file_body = c.issue();
    let external_id = c.issue();
    let upload_name = c.issue();
    let upload_sport = c.issue();
    c.assert_debug_absent("activity update", &changes);
    for case in 0..7 {
        let echo = secret.clone();
        let next_access = access.clone();
        let next_refresh = refresh.clone();
        let server = TestServer::start(move |r: &concord_example_testserver::Recorded| {
            let success = if r.path() == "/oauth/token" {
                serde_json::json!({"token_type":"Bearer","access_token":next_access,
                    "refresh_token":next_refresh,"expires_at":4102444800_u64,"expires_in":3600,
                    "scope":"activity:read_all activity:write"}).to_string()
            } else if r.path() == "/api/v3/uploads" {
                format!(r#"{{"id":17,"external_id":"{echo}","error":null,"status":"{echo}","activity_id":null}}"#)
            } else { include_str!("../fixtures/activity.json").into() };
            let problem = format!(r#"{{"message":"{echo}","errors":[{{"resource":"{echo}","field":"{echo}","code":"{echo}"}}]}}"#);
            reply(case, &success, &problem, &echo)
        }).await;
        let base = if case == 6 { refused() } else { server.url() };
        let credentials = Credentials::new(123, secret.clone()).unwrap();
        let stored = tokens(&access, &refresh, false);
        c.assert_debug_absent("credentials", &credentials);
        c.assert_debug_absent("tokens", &stored);
        let session = Session::connect(
            &base,
            &base,
            credentials.clone(),
            stored,
            Duration::from_secs(2),
        )
        .unwrap();
        c.assert_debug_absent("session", &session);
        c.assert_debug_absent(
            "config",
            &StravaConfig {
                policy: session.clone(),
            },
        );
        let api = session.client();
        let activities = api.activities();
        let read = activities.get(1);
        let write = activities.update(1, changes.clone());
        c.assert_debug_absent("client", &api);
        c.assert_debug_absent("scope", &activities);
        c.assert_debug_absent("read endpoint", &read);
        c.assert_debug_absent("write endpoint", &write);
        c.assert_debug_absent("read Prepared", &read.clone().prepare().unwrap());
        c.assert_debug_absent("write Prepared", &write.clone().prepare().unwrap());
        let result = read.send().await;
        c.assert_debug_absent("read Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        match result {
            Ok(response) => {
                c.assert_debug_absent("read response", &response);
                c.assert_debug_absent("metadata", &response.meta);
                c.assert_debug_absent("quota report", &response.rates);
            }
            Err(e) => error(&mut c, &e),
        }
        let result = write.send().await;
        c.assert_debug_absent("write Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        if let Err(e) = result {
            error(&mut c, &e);
        }
        c.assert_debug_absent("latest persisted tokens", &session.latest_tokens().await);
        let grant = TokenRequest::authorization_code(&credentials, code.clone()).unwrap();
        c.assert_debug_absent("OAuth grant", &grant);
        let oauth = concord::BaseUrl::parse(base.as_ref())
            .map(|origin| OAuth::new(http(), concord_sdk_strava::OAuthConfig { origin }))
            .unwrap();
        let scope = oauth.oauth();
        c.assert_debug_absent("OAuth client", &oauth);
        c.assert_debug_absent("OAuth scope", &scope);
        let op = scope.token(grant);
        c.assert_debug_absent("OAuth endpoint", &op);
        // TokenRequest is single-use, so prepare a separate grant for the native send.
        let native = scope
            .token(TokenRequest::authorization_code(&credentials, code.clone()).unwrap())
            .prepare()
            .unwrap();
        c.assert_debug_absent("OAuth Prepared", &native);
        let result = receive(&mut c, native).await;
        c.assert_debug_absent("native OAuth Result", &result);
        if let Err(e) = &result {
            concord_error(&mut c, e);
        }
        let result = op.send().await;
        c.assert_debug_absent("OAuth Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        match result {
            Ok(response) => {
                c.assert_debug_absent("OAuth response", &response);
                c.assert_debug_absent("returned tokens", &response.value);
            }
            Err(e) => error(&mut c, &e),
        }
        if case == 0 {
            let expired = Session::connect(
                &base,
                &base,
                credentials,
                tokens(&access, &refresh, true),
                Duration::from_secs(2),
            )
            .unwrap();
            let response = expired.client().activities().get(1).send().await.unwrap();
            c.assert_debug_absent("post-refresh response", &response);
            c.assert_debug_absent("rotated tokens", &expired.latest_tokens().await);
            let file = UploadFile::new(
                &filename,
                DataType::Gpx,
                concord::reqwest::Body::from(file_body.clone()),
            )
            .unwrap()
            .external_id(external_id.clone())
            .name(upload_name.clone())
            .sport_type(upload_sport.clone());
            c.assert_debug_absent("UploadFile", &file);
            let uploads = api.uploads();
            c.assert_debug_absent("upload scope", &uploads);
            let op = uploads.create(file);
            c.assert_debug_absent("upload endpoint", &op);
            let response = op.send().await.unwrap();
            c.assert_debug_absent("upload response", &response);
            c.assert_debug_absent("upload status", &response.value);
            c.assert_on_wire(&server.requests());
        }
        server.shutdown().await;
    }
    assert!(c.checked() >= 300);
    println!(
        "strava: {} renderings; positive control passed",
        c.checked()
    );
}
