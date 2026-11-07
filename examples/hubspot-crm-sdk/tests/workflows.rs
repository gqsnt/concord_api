//! Connected workflows against loopback fixtures.
use concord::reqwest;
use concord_example_testserver::{Gate, Reply, TestServer};
use concord_pagination::Resume;
use concord_sdk_hubspot_crm::{CreateContact, Error, Filter, FilterGroup, HubSpot, SearchRequest};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

fn client(url: &str, timeout: Duration) -> HubSpot {
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(timeout)
        .build()
        .unwrap();
    HubSpot::new(
        http,
        "synthetic-client".into(),
        "synthetic-secret".into(),
        "synthetic-refresh-0".into(),
    )
    .unwrap()
    .with_base_url(url)
    .unwrap()
}

#[tokio::test]
async fn search_follow_cursor_and_get_then_create_and_get() {
    let server = TestServer::start(|r| match (r.method.as_str(), r.path()) {
        ("POST", "/oauth/2026-09/token") => {
            Reply::json(200, include_str!("../fixtures/oauth.json"))
        }
        ("POST", "/crm/v3/objects/contacts/search") => {
            let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
            if body.get("after").is_some() {
                Reply::json(200, include_str!("../fixtures/search-next.json"))
            } else {
                Reply::json(200, include_str!("../fixtures/search-first.json"))
            }
        }
        ("POST", "/crm/v3/objects/contacts") => {
            Reply::json(201, include_str!("../fixtures/created.json"))
        }
        ("GET", "/crm/v3/objects/contacts/102") => {
            Reply::json(200, include_str!("../fixtures/contact.json"))
        }
        ("GET", "/crm/v3/objects/contacts/103") => {
            Reply::json(200, include_str!("../fixtures/created.json"))
        }
        _ => Reply::new(404),
    })
    .await;
    let hub = client(&server.url(), Duration::from_secs(3));
    let request = SearchRequest {
        filter_groups: vec![FilterGroup {
            filters: vec![Filter {
                property_name: "email".into(),
                operator: "EQ".into(),
                value: "anna.smith@example.com".into(),
            }],
        }],
        limit: Some(1),
        ..Default::default()
    };
    let all = hub
        .search(request, Resume::Start)
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(
        all.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        ["101", "102"]
    );
    assert_eq!(hub.get(&all[1].id).await.unwrap().id, "102");
    let mut create = CreateContact::default();
    create
        .properties
        .insert("email".into(), "anna.smith@example.com".into());
    create.properties.insert("firstname".into(), "Anna".into());
    create.properties.insert("lastname".into(), "Smith".into());
    let created = hub.create(create).await.unwrap();
    assert_eq!(hub.get(&created.id).await.unwrap(), created);
    let requests = server.requests();
    assert_eq!(requests.len(), 6);
    let oauth = &requests[0];
    assert_eq!(
        oauth.header("content-type"),
        Some("application/x-www-form-urlencoded")
    );
    let form = oauth.form_pairs();
    assert!(form.contains(&("grant_type".into(), "refresh_token".into())));
    assert!(form.contains(&("refresh_token".into(), "synthetic-refresh-0".into())));
    assert!(form.contains(&("client_secret".into(), "synthetic-secret".into())));
    assert!(oauth.query().is_none());
    let pages: Vec<_> = requests
        .iter()
        .filter(|r| r.path().ends_with("/search"))
        .collect();
    let first: serde_json::Value = serde_json::from_slice(&pages[0].body).unwrap();
    let second: serde_json::Value = serde_json::from_slice(&pages[1].body).unwrap();
    assert_eq!(
        first["filterGroups"][0]["filters"][0]["propertyName"],
        "email"
    );
    assert!(first.get("after").is_none());
    assert_eq!(second["after"], "cursor+2");
    assert_eq!(
        pages[0].header("authorization"),
        Some("Bearer synthetic-access-1")
    );
    assert_eq!(pages[0].header("content-type"), Some("application/json"));
    assert_eq!(hub.refresh_token().unwrap(), "synthetic-refresh-1");
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_refresh_is_single_flight_and_replacement_is_used_next() {
    let gate = Gate::new();
    let gate_server = gate.clone();
    let count = Arc::new(AtomicUsize::new(0));
    let counts = count.clone();
    let server = TestServer::start(move |r| {
        if r.path() == "/oauth/2026-09/token" {
            let n = counts.fetch_add(1, Ordering::SeqCst);
            if n == 0 { Reply::json(200, r#"{"access_token":"access-1","refresh_token":"refresh-1","expires_in":31,"token_type":"bearer"}"#).after(gate_server.clone()) }
            else { Reply::json(200, r#"{"access_token":"access-2","refresh_token":"refresh-2","expires_in":1800,"token_type":"bearer"}"#) }
        } else { Reply::json(200, include_str!("../fixtures/contact.json")) }
    }).await;
    let hub = client(&server.url(), Duration::from_secs(5));
    let a = {
        let h = hub.clone();
        tokio::spawn(async move { h.get("102").await })
    };
    assert!(
        server
            .wait_until(Duration::from_secs(2), |s| s.requests().len() == 1)
            .await
    );
    let joined = Arc::new(AtomicBool::new(false));
    let b = {
        let h = hub.clone();
        let joined = joined.clone();
        tokio::spawn(async move {
            use std::future::{Future, poll_fn};
            let future = h.get("102");
            tokio::pin!(future);
            poll_fn(|cx| {
                assert!(future.as_mut().poll(cx).is_pending());
                std::task::Poll::Ready(())
            })
            .await;
            joined.store(true, Ordering::SeqCst);
            future.await
        })
    };
    assert!(
        server
            .wait_until(Duration::from_secs(2), |_| joined.load(Ordering::SeqCst))
            .await
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
    gate.open();
    a.await.unwrap().unwrap();
    b.await.unwrap().unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(hub.refresh_token().unwrap(), "refresh-1");
    tokio::time::sleep(Duration::from_millis(1100)).await;
    hub.get("102").await.unwrap();
    let refreshes: Vec<_> = server
        .requests()
        .into_iter()
        .filter(|r| r.path() == "/oauth/2026-09/token")
        .collect();
    assert_eq!(refreshes.len(), 2);
    assert!(
        refreshes[1]
            .form_pairs()
            .contains(&("refresh_token".into(), "refresh-1".into()))
    );
    assert_eq!(hub.refresh_token().unwrap(), "refresh-2");
    server.shutdown().await;
}

#[tokio::test]
async fn timed_out_create_is_uncertain_and_never_replayed() {
    let server = TestServer::start(|r| {
        if r.path().contains("/oauth/") {
            Reply::json(200, include_str!("../fixtures/oauth.json"))
        } else {
            Reply::json(201, include_str!("../fixtures/created.json"))
                .delay(Duration::from_millis(300))
        }
    })
    .await;
    let hub = client(&server.url(), Duration::from_millis(100));
    assert!(matches!(
        hub.create(CreateContact::default()).await,
        Err(Error::CreateUncertain)
    ));
    assert_eq!(
        server
            .requests()
            .iter()
            .filter(|r| r.path() == "/crm/v3/objects/contacts")
            .count(),
        1
    );
    server.shutdown().await;
}

#[tokio::test]
async fn bounded_search_is_incomplete_and_cursor_can_resume() {
    use concord_pagination::PagesError;
    use std::num::NonZeroUsize;
    let server = TestServer::start(|r| {
        if r.path().contains("/oauth/") {
            Reply::json(200, include_str!("../fixtures/oauth.json"))
        } else {
            let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
            if body.get("after").is_some() {
                Reply::json(200, include_str!("../fixtures/search-next.json"))
            } else {
                Reply::json(200, include_str!("../fixtures/search-first.json"))
            }
        }
    })
    .await;
    let hub = client(&server.url(), Duration::from_secs(3));
    let incomplete = hub
        .search(SearchRequest::default(), Resume::Start)
        .max_pages(NonZeroUsize::new(1).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected[0].id, "101");
    let resume = match incomplete.error {
        PagesError::Bounded { resume, .. } => resume,
        other => panic!("{other:?}"),
    };
    let remaining = hub
        .search(SearchRequest::default(), resume)
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(remaining[0].id, "102");
    server.shutdown().await;
}
