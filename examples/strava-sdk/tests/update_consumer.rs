//! Consumer view of the activity update (maintenance ticket), against the loopback server only.
//!
//! An application edits an activity it listed, resolves an uncertain update by re-reading it,
//! reacts to an athlete who did not grant `activity:write`, and sees the update share the
//! non-upload usage windows with reads. Live provider calls: NOT_RUN.

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_strava::pagination::Resume;
use concord_sdk_strava::{
    ActivityUpdate, Credentials, DataType, Error, OAuth, Session, TokenRequest, Tokens, UploadFile,
    concord::reqwest, transport,
};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

type Res = Result<(), Box<dyn std::error::Error + Send + Sync>>;

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
fn credentials() -> Credentials {
    Credentials::new(104_233, "app-secret-for-fixtures".into()).unwrap()
}
fn tokens(scope: &str) -> Tokens {
    Tokens {
        token_type: "Bearer".into(),
        access_token: "access-fixture".into(),
        refresh_token: "refresh-fixture".into(),
        expires_at: now() + 21_600,
        expires_in: 21_600,
        scope: Some(scope.into()),
    }
}
fn connect(server: &TestServer, tokens: Tokens, timeout: Duration) -> Session {
    Session::connect(&server.url(), &server.url(), credentials(), tokens, timeout).unwrap()
}

/// A stored activity the fixture provider edits like the reference describes: only sent
/// fields change, `tags.Commute` sets commute, `gear_id: "none"` clears the gear.
#[derive(Clone)]
struct Stored(Arc<Mutex<serde_json::Value>>);
impl Stored {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(serde_json::json!({
            "id": 15_500_000_001u64, "name": "Morning Ride", "distance": 28_099.6,
            "moving_time": 4_207, "sport_type": "Ride", "description": null,
            "gear_id": "b1234", "commute": false, "trainer": false, "resource_state": 3
        }))))
    }
    fn apply(&self, body: &[u8]) {
        let change: serde_json::Value = serde_json::from_slice(body).unwrap();
        let mut stored = self.0.lock().unwrap();
        for (key, value) in change.as_object().unwrap() {
            match (key.as_str(), value) {
                ("tags", tags) => stored["commute"] = tags["Commute"].clone(),
                ("gear_id", gear) if gear == "none" => stored["gear_id"] = serde_json::Value::Null,
                (key, value) => stored[key] = value.clone(),
            }
        }
    }
    fn body(&self) -> String {
        self.0.lock().unwrap().to_string()
    }
}

#[tokio::test]
async fn edit_a_listed_activity_and_use_the_returned_activity() -> Res {
    let stored = Stored::new();
    let provider = stored.clone();
    let server = TestServer::start(move |r: &Recorded| match (r.method.as_str(), r.path()) {
        ("POST", "/oauth/token") => Reply::json(
            200,
            format!(
                r#"{{"token_type":"Bearer","expires_at":{},"expires_in":21600,"refresh_token":"r1","access_token":"a1","scope":"read activity:read_all activity:write"}}"#,
                now() + 21_600
            ),
        ),
        ("GET", "/api/v3/athlete/activities") => match r.query_all("page")[0].as_str() {
            "1" => Reply::json(200, format!("[{}]", provider.body())),
            _ => Reply::json(200, "[]"),
        },
        ("PUT", "/api/v3/activities/15500000001") => {
            provider.apply(&r.body);
            Reply::json(200, provider.body())
        }
        _ => Reply::json(404, r#"{"message":"Record Not Found","errors":[]}"#),
    })
    .await;
    let oauth = OAuth::new(transport(Duration::from_secs(5))?).with_base_url(server.url())?;
    let tokens = oauth
        .oauth()
        .token(TokenRequest::authorization_code(
            &credentials(),
            "code-from-validated-callback".into(),
        )?)
        .send()
        .await?
        .value;
    let session = Session::connect(
        &server.url(),
        &server.url(),
        credentials(),
        tokens,
        Duration::from_secs(5),
    )?;
    let api = session.client();
    let mut pages = api
        .athlete()
        .pages(30, Resume::Start, NonZeroUsize::new(5).unwrap())?;
    let listed = pages.next_page().await?.unwrap().items.remove(0);
    assert_eq!(listed.gear_id.as_deref(), Some("b1234"));

    // Change exactly what the athlete edited; everything else stays as it is.
    let edit = ActivityUpdate::new()
        .name("Gravel loop with Sam".into())
        .sport_type("GravelRide".into())
        .description("Windy along the canal".into())
        .commute(true);
    let updated = api
        .activities()
        .update(listed.id, edit.clone())
        .send()
        .await?;
    assert!(edit.is_reflected_in(&updated.value));
    assert_eq!(
        updated.value.description.as_deref(),
        Some("Windy along the canal")
    );
    assert_eq!(
        updated.value.gear_id, listed.gear_id,
        "unsent fields unchanged"
    );
    assert_eq!(updated.value.trainer, Some(false));
    assert_eq!(updated.attempts, 1);

    // Later: the bike was logged by mistake; it was on the trainer.
    let fix = ActivityUpdate::new().clear_gear().trainer(true);
    let fixed = api
        .activities()
        .update(listed.id, fix.clone())
        .send()
        .await?;
    assert!(fix.is_reflected_in(&fixed.value));
    assert_eq!(fixed.value.gear_id, None);
    assert_eq!(fixed.value.name, "Gravel loop with Sam");

    let puts: Vec<String> = server
        .requests()
        .into_iter()
        .filter(|r| r.method == "PUT")
        .map(|r| r.body_text())
        .collect();
    assert_eq!(
        puts,
        [
            r#"{"name":"Gravel loop with Sam","sport_type":"GravelRide","description":"Windy along the canal","tags":{"Commute":true}}"#,
            r#"{"gear_id":"none","trainer":true}"#,
        ]
    );
    Ok(())
}

#[tokio::test]
async fn uncertain_update_is_resolved_by_rereading_never_by_an_automatic_resend() -> Res {
    for applied in [true, false] {
        let stored = Stored::new();
        let provider = stored.clone();
        let puts = Arc::new(AtomicUsize::new(0));
        let seen = puts.clone();
        let server = TestServer::start(move |r: &Recorded| match r.method.as_str() {
            "PUT" if seen.fetch_add(1, Ordering::SeqCst) == 0 => {
                if applied {
                    // The provider commits the edit, then answers after the client's deadline.
                    provider.apply(&r.body);
                    Reply::json(200, provider.body()).delay(Duration::from_secs(1))
                } else {
                    Reply::hang_up() // lost before the provider applied it
                }
            }
            "PUT" => {
                provider.apply(&r.body);
                Reply::json(200, provider.body())
            }
            _ => Reply::json(200, provider.body()),
        })
        .await;
        let api = connect(
            &server,
            tokens("read activity:read_all activity:write"),
            Duration::from_millis(250),
        )
        .client();
        let edit = ActivityUpdate::new().name("Renamed after the race".into());
        let err = api
            .activities()
            .update(15_500_000_001, edit.clone())
            .send()
            .await
            .unwrap_err();
        assert!(err.is_ambiguous(), "{err:?}");
        assert!(matches!(err, Error::Ambiguous(_)), "{err:?}");
        assert_eq!(
            puts.load(Ordering::SeqCst),
            1,
            "the SDK never resends an update"
        );

        // The application resolves the uncertainty by reading the activity again.
        let current = api.activities().get(15_500_000_001).send().await?.value;
        assert_eq!(edit.is_reflected_in(&current), applied);
        if !edit.is_reflected_in(&current) {
            // Its own decision, made after looking: send the edit again, explicitly.
            let resent = api
                .activities()
                .update(15_500_000_001, edit.clone())
                .send()
                .await?;
            assert!(edit.is_reflected_in(&resent.value));
        }
        let methods: Vec<String> = server.requests().into_iter().map(|r| r.method).collect();
        let expected: &[&str] = if applied {
            &["PUT", "GET"]
        } else {
            &["PUT", "GET", "PUT"]
        };
        assert_eq!(methods, expected);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&stored.body())?["name"],
            "Renamed after the race"
        );
    }
    Ok(())
}

#[tokio::test]
async fn athlete_who_did_not_grant_write_gets_a_typed_error_and_reads_still_work() -> Res {
    let server = TestServer::start(|r: &Recorded| match (r.method.as_str(), r.path()) {
        ("POST", "/oauth/token") => Reply::json(
            200,
            format!(
                r#"{{"token_type":"Bearer","expires_at":{},"expires_in":21600,"refresh_token":"r1","access_token":"a1","scope":"read activity:read_all"}}"#,
                now() + 21_600
            ),
        ),
        ("GET", _) => Reply::json(200, Stored::new().body()),
        _ => Reply::json(401, r#"{"message":"Authorization Error","errors":[]}"#),
    })
    .await;
    // The athlete unchecked activity:write on the consent screen; the exchange reports it.
    let oauth = OAuth::new(transport(Duration::from_secs(5))?).with_base_url(server.url())?;
    let tokens = oauth
        .oauth()
        .token(TokenRequest::authorization_code(
            &credentials(),
            "code-from-validated-callback".into(),
        )?)
        .send()
        .await?
        .value;
    let api = connect(&server, tokens, Duration::from_secs(5)).client();
    let err = api
        .activities()
        .update(15_500_000_001, ActivityUpdate::new().trainer(true))
        .send()
        .await
        .unwrap_err();
    match err {
        Error::MissingScope(scope) => assert_eq!(scope, "activity:write"), // ask to re-authorize
        other => panic!("expected a typed scope error, got {other:?}"),
    }
    assert_eq!(
        server.requests().len(),
        1,
        "only the code exchange was sent"
    );
    let read = api.activities().get(15_500_000_001).send().await?;
    assert_eq!(read.value.name, "Morning Ride");
    Ok(())
}

#[tokio::test]
async fn update_is_gated_by_the_non_upload_window_that_reads_exhausted() -> Res {
    let server = TestServer::start(|r: &Recorded| match r.method.as_str() {
        "GET" => Reply::json(200, format!("[{}]", Stored::new().body()))
            .header("x-ratelimit-limit", "200,2000")
            .header("x-ratelimit-usage", "120,900")
            .header("x-readratelimit-limit", "100,1000")
            .header("x-readratelimit-usage", "100,880"),
        "POST" => Reply::json(
            201,
            r#"{"id":19000000001,"id_str":"19000000001","external_id":"x.gpx","error":null,"status":"Your activity is still being processed.","activity_id":null}"#,
        ),
        _ => Reply::json(200, Stored::new().body()),
    })
    .await;
    let api = connect(
        &server,
        tokens("read activity:read_all activity:write"),
        Duration::from_secs(5),
    )
    .client();
    let listed = api.athlete().activities().send().await?;
    assert!(listed.rates.read?.unwrap().short.exhausted());
    let err = api
        .activities()
        .update(15_500_000_001, ActivityUpdate::new().name("x".into()))
        .send()
        .await
        .unwrap_err();
    match err {
        Error::Admission(concord_ratelimit::Error::Gated(Some(wait))) => {
            assert!(wait <= Duration::from_secs(15 * 60), "{wait:?}");
        }
        other => panic!("expected the non-upload window to gate the update, got {other:?}"),
    }
    // Uploads are exempt from the non-upload windows and still go out.
    let file = UploadFile::new("x.gpx", DataType::Gpx, reqwest::Body::from(&b"<gpx/>"[..]))?;
    api.uploads().create(file).send().await?;
    let methods: Vec<String> = server.requests().into_iter().map(|r| r.method).collect();
    assert_eq!(methods, ["GET", "POST"]);
    Ok(())
}
