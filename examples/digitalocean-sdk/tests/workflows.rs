//! Connected read workflows and pagination trust/completion regressions, all on loopback.
mod support;

use concord_sdk_digitalocean::{
    Error, TagName,
    concord_pagination::{Outcome, PagesError, Resume},
};
use concord_testserver::{Recorded, Reply, TestServer};
use std::sync::atomic::{AtomicUsize, Ordering};
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

// Maintenance: tag-filtered droplets and droplet snapshots (see MAINTENANCE.md).

fn targets(server: &TestServer) -> Vec<String> {
    server.requests().into_iter().map(|r| r.target).collect()
}

#[tokio::test]
async fn tagged_droplets_then_snapshots_of_the_selected_droplet() {
    let server = TestServer::start(workflow_reply).await;
    let api = client(&server);
    let tagged = api
        .tagged_droplet_pages(&prod(), size(), cap(2))
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(
        tagged.iter().map(|d| d.id).collect::<Vec<_>>(),
        [3164444, 3164446]
    );
    assert!(
        tagged
            .iter()
            .all(|d| d.tags.iter().any(|t| t == "env:prod"))
    );
    let mut pages = api.snapshot_pages(tagged[0].id, size(), cap(2));
    let first = pages.next_page().await.unwrap().unwrap();
    assert!(first.next.is_some());
    let snapshot = &first.items[0];
    assert_eq!((snapshot.id, snapshot.kind.as_str()), (6372321, "snapshot"));
    assert_eq!(snapshot.regions, ["nyc3", "sfo3"]);
    assert_eq!(snapshot.min_disk_size, 25);
    assert!((snapshot.size_gigabytes - 2.34).abs() < 1e-9);
    assert_eq!(snapshot.created_at, "2020-07-28T16:47:44Z");
    assert_eq!(
        first.meta.operation().unwrap().name(),
        "DigitalOcean.droplet.snapshots"
    );
    let second = pages.next_page().await.unwrap().unwrap();
    assert_eq!(second.items[0].kind, "backup");
    assert!(second.next.is_none(), "an empty links object completes");
    assert!(pages.next_page().await.unwrap().is_none());
    assert_eq!(pages.finish().outcome, Outcome::Exhausted);
    assert_eq!(
        targets(&server),
        [
            "/v2/droplets?per_page=1&tag_name=env%3Aprod",
            "/v2/droplets?page=2&per_page=1&tag_name=env%3Aprod",
            "/v2/droplets/3164444/snapshots?per_page=1",
            "/v2/droplets/3164444/snapshots?page=2&per_page=1",
        ]
    );
    assert_wire(&server.requests());
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn invalid_tags_are_refused_before_any_request() {
    let server = TestServer::start(workflow_reply).await;
    for tag in [
        "",
        " ",
        "env:prod ",
        " env:prod",
        "env prod",
        "a\tb",
        "a\nb",
        "a\u{7f}",
    ] {
        assert!(matches!(TagName::new(tag), Err(Error::TagName)), "{tag:?}");
    }
    assert!(!format!("{} {:?}", Error::TagName, Error::TagName).contains("env"));
    for tag in ["env:prod", "web", "team+ops", "a&tag_name=b", "café"] {
        assert_eq!(TagName::new(tag).unwrap().as_str(), tag);
    }
    assert!(server.requests().is_empty());
    server.shutdown().await;
}

#[tokio::test]
async fn next_link_must_keep_exactly_the_traversal_tag() {
    // Same origin and collection: only the tag differs from the traversal's plan.
    for (case, query, accepted) in [
        ("encoded", "page=2&per_page=1&tag_name=env%3Aprod", true),
        ("unencoded", "page=2&per_page=1&tag_name=env:prod", true),
        ("reordered", "tag_name=env%3Aprod&per_page=1&page=2", true),
        ("dropped", "page=2&per_page=1", false),
        ("other tag", "page=2&per_page=1&tag_name=env%3Adev", false),
        ("case", "page=2&per_page=1&tag_name=ENV%3APROD", false),
        ("repeated", "page=2&tag_name=env%3Aprod&tag_name=web", false),
        ("empty", "page=2&per_page=1&tag_name=", false),
        ("renamed key", "page=2&per_page=1&tag=env%3Aprod", false),
    ] {
        let server = TestServer::start(move |r: &Recorded| {
            if r.query_all("page") == ["2"] {
                return Reply::json(200, include_str!("../fixtures/tagged-last.json"));
            }
            let next = format!("http://{}/v2/droplets?{query}", r.host);
            Reply::json(
                200,
                serde_json::json!({"droplets":[],"links":{"pages":{"next":next}}}).to_string(),
            )
        })
        .await;
        let result = client(&server)
            .tagged_droplet_pages(&prod(), size(), cap(3))
            .items()
            .collect_complete()
            .await;
        if accepted {
            assert_eq!(result.unwrap()[0].id, 3164446, "{case}");
            assert_eq!(server.requests().len(), 2, "{case}");
            assert_eq!(server.requests()[1].query_all("tag_name"), ["env:prod"]);
        } else {
            let incomplete = result.unwrap_err();
            assert!(
                matches!(incomplete.error.provider_error(), Some(Error::UnsafeNext)),
                "{case}"
            );
            // As for the other listings: the page carrying the refused link is the position.
            assert!(matches!(
                incomplete.error,
                PagesError::Provider {
                    failed: Resume::Start,
                    ..
                }
            ));
            assert_eq!(server.requests().len(), 1, "{case}: nothing followed");
        }
        server.shutdown().await;
    }
}

#[tokio::test]
async fn unfiltered_listings_refuse_a_link_that_adds_a_tag() {
    let server = TestServer::start(|r: &Recorded| {
        let next = format!("http://{}{}?page=2&tag_name=env%3Aprod", r.host, r.path());
        let key = if r.path().ends_with("snapshots") {
            "snapshots"
        } else {
            "droplets"
        };
        Reply::json(
            200,
            serde_json::json!({key: [], "links": {"pages": {"next": next}}}).to_string(),
        )
    })
    .await;
    let api = client(&server);
    for error in [
        api.droplet_pages(size(), cap(2))
            .next_page()
            .await
            .unwrap_err(),
        api.snapshot_pages(3164444, size(), cap(2))
            .next_page()
            .await
            .unwrap_err(),
    ] {
        assert!(matches!(error.provider_error(), Some(Error::UnsafeNext)));
    }
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn resumed_tagged_traversal_keeps_the_tag_and_refuses_foreign_keys() {
    let server = TestServer::start(workflow_reply).await;
    let api = client(&server);
    let bounded = api
        .tagged_droplet_pages(&prod(), size(), cap(1))
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(bounded.collected[0].id, 3164444);
    assert!(matches!(bounded.error, PagesError::Bounded { .. }));
    let tagged_key = bounded.error.resume().cloned().unwrap();
    assert!(matches!(tagged_key, Resume::At(_)));

    let rest = api
        .tagged_droplet_pages(&prod(), size(), cap(1))
        .starting_at(tagged_key.clone())
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(rest[0].id, 3164446);
    // A resumed start sends the tag too.
    let again = api
        .tagged_droplet_pages(&prod(), size(), cap(2))
        .starting_at(Resume::Start)
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(again.len(), 2);

    let untagged_key = api
        .droplet_pages(size(), cap(1))
        .items()
        .collect_complete()
        .await
        .unwrap_err()
        .error
        .resume()
        .cloned()
        .unwrap();
    let sent = server.requests().len();
    let dev = TagName::new("env:dev").unwrap();
    for mut foreign in [
        api.droplet_pages(size(), cap(2))
            .starting_at(tagged_key.clone()),
        api.tagged_droplet_pages(&dev, size(), cap(2))
            .starting_at(tagged_key.clone()),
        api.tagged_droplet_pages(&prod(), size(), cap(2))
            .starting_at(untagged_key),
    ] {
        let error = foreign.next_page().await.unwrap_err();
        assert!(matches!(error.provider_error(), Some(Error::UnsafeNext)));
        assert!(matches!(
            error,
            PagesError::Provider {
                failed: Resume::At(_),
                ..
            }
        ));
    }
    let error = api
        .snapshot_pages(3164444, size(), cap(2))
        .starting_at(tagged_key)
        .next_page()
        .await
        .unwrap_err();
    assert!(matches!(error.provider_error(), Some(Error::UnsafeNext)));
    assert_eq!(
        server.requests().len(),
        sent,
        "foreign keys are refused unsent"
    );
    assert_eq!(
        targets(&server),
        [
            "/v2/droplets?per_page=1&tag_name=env%3Aprod",
            "/v2/droplets?page=2&per_page=1&tag_name=env%3Aprod",
            "/v2/droplets?per_page=1&tag_name=env%3Aprod",
            "/v2/droplets?page=2&per_page=1&tag_name=env%3Aprod",
            "/v2/droplets?per_page=1",
        ]
    );
    assert_wire(&server.requests());
    server.shutdown().await;
}

#[tokio::test]
async fn failed_tagged_page_is_retried_from_its_reported_position() {
    let failures = AtomicUsize::new(0);
    let server = TestServer::start(move |r: &Recorded| {
        if r.query_all("page") == ["2"] && failures.fetch_add(1, Ordering::SeqCst) == 0 {
            Reply::json(503, include_str!("../fixtures/error-secret.json"))
        } else {
            workflow_reply(r)
        }
    })
    .await;
    let api = client(&server);
    let incomplete = api
        .tagged_droplet_pages(&prod(), size(), cap(3))
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    let (error, failed, _) = incomplete.error.into_provider_error().unwrap();
    assert!(matches!(error, Error::Provider(_)));
    let rest = api
        .tagged_droplet_pages(&prod(), size(), cap(3))
        .starting_at(failed)
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(rest[0].id, 3164446);
    assert_eq!(
        targets(&server)[1..],
        [
            "/v2/droplets?page=2&per_page=1&tag_name=env%3Aprod",
            "/v2/droplets?page=2&per_page=1&tag_name=env%3Aprod",
        ]
    );
    server.shutdown().await;
}

#[tokio::test]
async fn snapshot_traversal_shares_completion_cap_and_trust_rules() {
    // Completion for every optional links shape.
    for links in [
        "",
        ",\"links\":{}",
        ",\"links\":{\"pages\":{\"prev\":\"unused\"}}",
        ",\"links\":{\"pages\":{\"next\":null}}",
    ] {
        let body = format!("{{\"snapshots\":[]{links},\"meta\":{{\"total\":9}}}}");
        let server = TestServer::start(move |_: &Recorded| Reply::json(200, &body)).await;
        let snapshots = client(&server)
            .snapshot_pages(3164444, size(), cap(1))
            .items()
            .collect_complete()
            .await
            .unwrap();
        assert!(snapshots.is_empty());
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }

    // An empty page with next continues; the cap is never completion; resume continues.
    let server = TestServer::start(|r: &Recorded| {
        if r.query_all("page") == ["2"] {
            workflow_reply(r)
        } else {
            let next = format!(
                "http://{}/v2/droplets/3164444/snapshots?page=2&per_page=1",
                r.host
            );
            Reply::json(
                200,
                serde_json::json!({"snapshots":[],"links":{"pages":{"next":next}}}).to_string(),
            )
        }
    })
    .await;
    let api = client(&server);
    let bounded = api
        .snapshot_pages(3164444, size(), cap(1))
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert!(bounded.collected.is_empty());
    assert!(matches!(bounded.error, PagesError::Bounded { .. }));
    let resumed = api
        .snapshot_pages(3164444, size(), cap(1))
        .starting_at(bounded.error.resume().cloned().unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(resumed[0].kind, "backup");
    // A key of this droplet's snapshots does not resume another droplet's.
    let key = api
        .snapshot_pages(3164444, size(), cap(1))
        .items()
        .collect_complete()
        .await
        .unwrap_err()
        .error
        .resume()
        .cloned()
        .unwrap();
    let sent = server.requests().len();
    let error = api
        .snapshot_pages(42, size(), cap(1))
        .starting_at(key)
        .next_page()
        .await
        .unwrap_err();
    assert!(matches!(error.provider_error(), Some(Error::UnsafeNext)));
    assert_eq!(server.requests().len(), sent);
    server.shutdown().await;

    // Untrusted hosts and other collections never receive the bearer token.
    let trap = TestServer::start(|_: &Recorded| panic!("untrusted server was contacted")).await;
    let untrusted = trap.url().replace("127.0.0.1", "localhost");
    for next in [
        format!("{untrusted}/v2/droplets/3164444/snapshots?page=2"),
        "__ORIGIN__/v2/droplets/42/snapshots?page=2".to_owned(),
        "__ORIGIN__/v2/droplets/3164444/actions?page=2".to_owned(),
        "__ORIGIN__/v2/snapshots?page=2".to_owned(),
    ] {
        let server = TestServer::start(move |r: &Recorded| {
            let body = serde_json::json!({"snapshots":[],"links":{"pages":{"next":next}}});
            Reply::json(200, fixture(&body.to_string(), r))
        })
        .await;
        let error = client(&server)
            .snapshot_pages(3164444, size(), cap(3))
            .next_page()
            .await
            .unwrap_err();
        assert!(matches!(error.provider_error(), Some(Error::UnsafeNext)));
        assert_eq!(server.requests().len(), 1);
        assert_wire(&server.requests());
        server.shutdown().await;
    }
    assert!(trap.requests().is_empty());
    trap.shutdown().await;
}
