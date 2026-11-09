//! Secrets: sensitive CrmApi Authorization (src/lib.rs:24); HubSpot::new client ID,
//! client secret and refresh token (403-408), sent in the OAuth refresh form. Distinct
//! canaries also occupy decoded access_token and rotated refresh_token (316-320). Access
//! travels as Bearer; a second public HubSpot instance sends the rotated token as a refresh
//! credential. No exemptions. CRM get/create and OAuth refresh cover all seven scenarios;
//! native CRM Prepared, batch reports and search pagination are also checked.
//! Excludes native reqwest, explicit raw header/body/refresh_token access and clear Contact /
//! search properties. RefreshRequest has private fields and no public constructor, so its
//! Debug cannot be reached directly; HubSpot constructs/sends it. RefreshResponse Deserialize
//! is public and is checked separately against the same credential-bearing OAuth fixture.
use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
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

// Echo only into withheld diagnostics, never clear success-model fields.
fn reply(case: usize, status: u16, success: &str, problem: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::json(status, success),
        1 => Reply::json(401, problem),
        2 => Reply::json(500, problem),
        3 => Reply::json(status, format!("{{invalid {echo}")),
        4 => Reply::text(status, "text/html", echo),
        5 => Reply::json(status, "").with_body(Body::FailAfter(vec![
            format!("{{\"incomplete\":\"{echo}").into(),
        ])),
        _ => unreachable!(),
    };
    response.header("x-canary", echo).header("set-cookie", echo)
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
    c.assert_debug_absent("error kind", &error.kind());
    if let Some(meta) = error.head_meta() {
        c.assert_debug_absent("error response metadata", &meta);
    }
}

use concord_pagination::Resume;
use concord_sdk_hubspot_crm::{
    BatchReadRequest, CreateContact, CrmApi, CrmApiConfig, Error, HubSpot, RefreshResponse,
    SearchRequest,
};

fn error(c: &mut Canaries, error: &Error) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    if let Error::Concord(e) = error {
        concord_error(c, e);
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("hubspot-crm");
    let client_id = c.issue();
    let client_secret = c.issue();
    let refresh = c.issue();
    let access = c.issue();
    let rotated = c.issue();
    let oauth_body = format!(
        r#"{{"access_token":"{access}","refresh_token":"{rotated}","expires_in":1800,"token_type":"bearer"}}"#
    );
    let decoded: RefreshResponse = serde_json::from_str(&oauth_body).unwrap();
    c.assert_debug_absent("decoded RefreshResponse", &decoded);
    let oauth = concord_sdk_hubspot_crm::OAuthApi::new(http());
    c.assert_debug_absent("OAuth client", &oauth);
    c.assert_debug_absent("OAuth scope", &oauth.oauth());
    for oauth_failure in [false, true] {
        for case in 0..7 {
            let (echo, oauth_body) = (client_secret.clone(), oauth_body.clone());
            let server = TestServer::start(move |r: &Recorded| {
                let oauth = r.path().ends_with("/token");
                let contact = r#"{"id":"one","properties":{"firstname":"Alice"}}"#;
                let success = if oauth { oauth_body.clone() }
                    else if r.path().ends_with("/batch/read") {
                        format!(r#"{{"results":[],"errors":[{{"category":"OBJECT_NOT_FOUND","message":"{echo}","links":["{echo}"],"context":{{"ids":["missing"],"secret":"{echo}"}}}}]}}"#)
                    } else if r.path().ends_with("/search") {
                        format!(r#"{{"total":1,"results":[{contact}]}}"#)
                    } else { contact.into() };
                let selected = if oauth == oauth_failure { case } else { 0 };
                reply(selected, 200, &success, &format!(r#"{{"message":"{echo}"}}"#), &echo)
            }).await;
            let base = if case == 6 { refused() } else { server.url() };
            let hub = HubSpot::new(
                http(),
                client_id.clone(),
                client_secret.clone(),
                refresh.clone(),
            )
            .unwrap()
            .with_base_url(&base)
            .unwrap();
            c.assert_debug_absent("HubSpot", &hub);
            let read = hub.get("one").await;
            let write = hub.create(CreateContact::default()).await;
            c.assert_debug_absent("get Result", &read);
            c.assert_debug_absent("create Result", &write);
            assert_eq!(read.is_ok(), case == 0);
            assert_eq!(write.is_ok(), case == 0);
            if let Err(e) = &read {
                error(&mut c, e);
                if case == 1 {
                    if oauth_failure {
                        assert!(matches!(
                            e,
                            Error::Refresh {
                                status: Some(401),
                                ..
                            }
                        ));
                    } else {
                        assert_eq!(
                            match e {
                                Error::Concord(e) => e.status().unwrap().as_u16(),
                                _ => 0,
                            },
                            401
                        );
                    }
                    assert!(e.to_string().contains("401"));
                }
            }
            if let Err(e) = &write {
                error(&mut c, e);
            }
            let mut pages = hub.search(SearchRequest::default(), Resume::Start);
            c.assert_debug_absent("search traversal", &pages);
            let page = pages.next_page().await;
            c.assert_debug_absent("search page Result", &page);
            if let Err(e) = page {
                c.assert_display_absent("PagesError", &e);
                c.assert_error_absent("PagesError and sources", &e);
                error(&mut c, e.provider_error().unwrap());
            }
            if !oauth_failure {
                // Use an unflagged HeaderValue: #[sensitive] must protect the config itself.
                let config = CrmApiConfig {
                    authorization: concord::reqwest::header::HeaderValue::from_str(&format!(
                        "Bearer {access}"
                    ))
                    .unwrap(),
                };
                c.assert_debug_absent("CrmApiConfig", &config);
                let api = CrmApi::new(http(), config).with_base_url(&base).unwrap();
                let scope = api.contacts();
                let get = scope.get("one");
                let create = scope.create(CreateContact::default());
                c.assert_debug_absent("generated client", &api);
                c.assert_debug_absent("contacts scope", &scope);
                c.assert_debug_absent("get endpoint", &get);
                c.assert_debug_absent("create endpoint", &create);
                c.assert_debug_absent("get Prepared", &get.clone().prepare().unwrap());
                c.assert_debug_absent("create Prepared", &create.clone().prepare().unwrap());
                let native = receive(&mut c, get.prepare().unwrap()).await;
                c.assert_debug_absent("native get Result", &native);
                assert_eq!(native.is_ok(), case == 0);
                if let Err(e) = native {
                    concord_error(&mut c, &e);
                }
                let native = receive(&mut c, create.prepare().unwrap()).await;
                c.assert_debug_absent("native create Result", &native);
                assert_eq!(native.is_ok(), case == 0);
                if let Err(e) = native {
                    concord_error(&mut c, &e);
                }
            }
            if case == 0 {
                let batch = hub
                    .batch_read(BatchReadRequest {
                        ids: vec!["missing".into()],
                        properties: vec![],
                    })
                    .await
                    .unwrap();
                c.assert_debug_absent("batch report", &batch);
                for item in batch.items {
                    c.assert_debug_absent("batch item", &item);
                    if let Err(e) = item.result {
                        c.assert_debug_absent("ItemError", &e);
                        c.assert_error_absent("ItemError", &e);
                    }
                }
                let renewed = HubSpot::new(
                    http(),
                    client_id.clone(),
                    client_secret.clone(),
                    hub.refresh_token().unwrap(),
                )
                .unwrap()
                .with_base_url(&base)
                .unwrap();
                c.assert_debug_absent("client from rotated token", &renewed);
                c.assert_debug_absent(
                    "rotated-token get Result",
                    &renewed.get("one").await.unwrap(),
                );
                c.assert_on_wire(&server.requests());
            }
            server.shutdown().await;
        }
    }
    assert!(c.checked() >= 250);
    println!(
        "hubspot-crm: {} renderings; positive control passed",
        c.checked()
    );
}
