//! Connected read workflows and pagination trust/completion regressions, all on loopback.
mod support;

use concord_sdk_digitalocean::{
    Error,
    concord_pagination::{Outcome, PagesError},
};
use concord_testserver::{Recorded, Reply, TestServer};
use support::*;

#[tokio::test]
async fn list_droplets_then_retrieve_selected_droplet() {
    let server = TestServer::start(workflow_reply).await;
    let api = client(&server);
    let droplets = api
        .droplet_pages(size(), cap(2))
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(
        droplets
            .iter()
            .map(|droplet| droplet.id)
            .collect::<Vec<_>>(),
        [3164444, 3164445]
    );
    let selected = api.droplet(droplets[0].id).get().send().await.unwrap();
    assert_eq!(selected.value.droplet.name, "fixture-web-01");
    assert_eq!(selected.value.droplet.networks.v4[0].gateway, None);
    assert_eq!(selected.meta.remaining().unwrap(), Some(4999));
    let requests = server.requests();
    assert_eq!(
        requests
            .iter()
            .map(|r| r.target.as_str())
            .collect::<Vec<_>>(),
        [
            "/v2/droplets?per_page=1",
            "/v2/droplets?page=2&per_page=1",
            "/v2/droplets/3164444",
        ]
    );
    assert_wire(&requests);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn selected_droplet_actions_then_retrieve_one_action() {
    let server = TestServer::start(workflow_reply).await;
    let api = client(&server);
    let droplets = api
        .droplet_pages(size(), cap(2))
        .items()
        .collect_complete()
        .await
        .unwrap();
    let droplet = api
        .droplet(droplets[0].id)
        .get()
        .send()
        .await
        .unwrap()
        .value
        .droplet;
    let mut pages = api.action_pages(droplet.id, size(), cap(2));
    let first = pages.next_page().await.unwrap().unwrap();
    let second = pages.next_page().await.unwrap().unwrap();
    assert_eq!(second.items[0].id, 7516);
    assert_eq!(second.items[0].completed_at, None);
    assert!(pages.next_page().await.unwrap().is_none());
    assert_eq!(pages.finish().outcome, Outcome::Exhausted);
    let action = api
        .droplet(droplet.id)
        .action(first.items[0].id)
        .send()
        .await
        .unwrap();
    assert_eq!(action.value.action.id, 7515);
    assert_eq!(action.value.action.resource_id, Some(droplet.id));
    assert_eq!(action.value.action.kind, "reboot");
    assert_eq!(action.value.action.status, "completed");
    let requests = server.requests();
    assert_eq!(requests.len(), 6);
    assert_eq!(
        &requests[3..]
            .iter()
            .map(|r| r.target.as_str())
            .collect::<Vec<_>>(),
        &[
            "/v2/droplets/3164444/actions?per_page=1",
            "/v2/droplets/3164444/actions?page=2&per_page=1",
            "/v2/droplets/3164444/actions/7515",
        ]
    );
    assert_wire(&requests);
    server.shutdown().await;
}

#[tokio::test]
async fn missing_next_is_completion_for_every_optional_links_shape() {
    // Synthetic omission variants: no links, empty links, empty pages, and prev-only pages.
    for links in [
        "",
        ",\"links\":{}",
        ",\"links\":{\"pages\":{}}",
        ",\"links\":{\"pages\":{\"prev\":\"unused\"}}",
        ",\"links\":null",
        ",\"links\":{\"pages\":{\"next\":null}}",
    ] {
        let droplets = format!("{{\"droplets\":[]{links},\"meta\":{{\"total\":999}}}}");
        let actions = format!("{{\"actions\":[]{links}}}");
        let server = TestServer::start(move |r: &Recorded| {
            Reply::json(
                200,
                if r.path().ends_with("actions") {
                    &actions
                } else {
                    &droplets
                },
            )
        })
        .await;
        let api = client(&server);
        assert!(
            api.droplet_pages(size(), cap(1))
                .items()
                .collect_complete()
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            api.action_pages(3164444, size(), cap(1))
                .items()
                .collect_complete()
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(server.requests().len(), 2);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn empty_page_with_next_continues_and_cap_never_claims_completion() {
    let server = TestServer::start(|r: &Recorded| {
        let body = if r.query_all("page") == ["2"] {
            include_str!("../fixtures/droplets-no-links.json")
        } else {
            include_str!("../fixtures/empty-next.json")
        };
        Reply::json(200, fixture(body, r))
    })
    .await;
    let api = client(&server);
    let incomplete = api
        .droplet_pages(size(), cap(1))
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert!(incomplete.collected.is_empty());
    assert!(matches!(incomplete.error, PagesError::Bounded { .. }));
    let mut pages = api.droplet_pages(size(), cap(1));
    assert!(pages.next_page().await.unwrap().unwrap().items.is_empty());
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Bounded { .. })
    ));
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    let all = api
        .droplet_pages(size(), cap(2))
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id, 3164444);
    assert_eq!(server.requests().len(), 4);
    server.shutdown().await;
}

#[tokio::test]
async fn untrusted_next_host_never_receives_bearer_for_either_listing() {
    let trap = TestServer::start(|_: &Recorded| panic!("untrusted server was contacted")).await;
    let target = trap.url().replace("127.0.0.1", "localhost");
    for actions in [false, true] {
        let target = target.clone();
        let server = TestServer::start(move |_: &Recorded| {
            let body =
                include_str!("../fixtures/unsafe-next.json").replace("__UNTRUSTED__", &target);
            Reply::json(
                200,
                if actions {
                    body.replace("droplets\":", "actions\":")
                } else {
                    body
                },
            )
        })
        .await;
        let api = client(&server);
        if actions {
            let error = api
                .action_pages(3164444, size(), cap(2))
                .next_page()
                .await
                .unwrap_err();
            assert!(matches!(error.provider_error(), Some(Error::UnsafeNext)));
        } else {
            let mut pages = api.droplet_pages(size(), cap(2));
            let error = pages.next_page().await.unwrap_err();
            assert!(matches!(error.provider_error(), Some(Error::UnsafeNext)));
            assert!(matches!(
                pages.next_page().await,
                Err(PagesError::Ended { .. })
            ));
        }
        assert_eq!(server.requests().len(), 1);
        assert_wire(&server.requests());
        server.shutdown().await;
    }
    assert!(trap.requests().is_empty());
    trap.shutdown().await;
}

#[tokio::test]
async fn second_page_failure_preserves_collected_prefix() {
    let server = TestServer::start(|r: &Recorded| {
        if r.query_all("page") == ["2"] {
            Reply::json(500, include_str!("../fixtures/error-secret.json"))
        } else {
            workflow_reply(r)
        }
    })
    .await;
    let incomplete = client(&server)
        .droplet_pages(size(), cap(3))
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected[0].id, 3164444);
    assert!(matches!(
        incomplete.error.provider_error(),
        Some(Error::Provider(_))
    ));
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}
