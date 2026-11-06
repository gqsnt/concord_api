//! The two public SDK workflows and their interruption/continuation semantics.
mod support;

use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroUsize,
    time::Duration,
};

use concord_example_testserver::{Gate, Reply, TestServer};
use concord_sdk_microsoft_graph::{
    AuthContext, ErrorClass, Field, GraphClient, GraphError, GroupField, LinkProblem, MemberField,
    ObjectKind, UserField,
    pagination::{Outcome, PagesError, Resume},
};
use futures_util::StreamExt;
use serde_json::json;
use support::*;

fn delta_reply(r: &concord_example_testserver::Recorded) -> Reply {
    match r.query().unwrap_or("") {
        q if q.contains("synthetic-next-round") => reply(r, NO_CHANGES),
        q if q.contains("deltatoken") => reply(r, INCREMENTAL),
        q if q.contains("synthetic-final") => reply(r, FINAL),
        q if q.contains("skiptoken") => reply(r, EMPTY),
        _ => reply(r, INITIAL),
    }
}

#[tokio::test]
async fn workflow_a_full_delta_empty_page_saved_checkpoint_and_incremental_sync() {
    let server = TestServer::start(delta_reply).await;
    let api = client(&server, "tenant-a");
    let listing = api
        .users_delta(&[
            UserField::DisplayName,
            UserField::Mail,
            UserField::AccountEnabled,
        ])
        .unwrap();
    let mut pages = listing.pages();
    let mut changes = Vec::new();
    let mut completed = None;
    let mut page_sizes = Vec::new();
    while let Some(page) = pages.next_page().await.unwrap() {
        page_sizes.push(page.items.len());
        changes.extend(page.items);
        if let Some(delta) = page.meta.delta_link {
            completed = Some(delta);
        }
    }
    assert_eq!(page_sizes, [2, 0, 1]);
    assert_eq!(changes.len(), 3);
    assert_eq!(pages.finish().outcome, Outcome::Exhausted);
    let checkpoint = completed.unwrap();
    assert!(checkpoint.url().contains("/users/microsoft.graph.delta?"));
    let stored = checkpoint.to_json();
    assert!(!stored.contains("synthetic-token"));

    // A later client, token refreshed within the same principal/permissions context.
    let later = GraphClient::for_loopback(
        AuthContext::new("tenant-a", "synthetic-principal-permissions-v1").unwrap(),
        "synthetic-refreshed-token",
        &format!("{}/v1.0", server.url()),
    )
    .unwrap();
    let restored = later.restore_delta(&stored).unwrap();
    let mut incremental = later.resume_users(&restored).unwrap().pages();
    let page = incremental.next_page().await.unwrap().unwrap();
    assert_eq!(page.items.len(), 4);
    let changed = &page.items[0];
    assert_eq!(changed.id, "user-a");
    assert_eq!(changed.display_name, Field::Value("Ada Updated".into()));
    assert_eq!(changed.mail, Field::Null);
    assert_eq!(changed.account_enabled, Field::Value(false));
    assert_eq!(changed.surname, Field::Missing);
    assert_eq!(page.items[1].removed.as_ref().unwrap().reason, "changed");
    assert_eq!(page.items[2].removed.as_ref().unwrap().reason, "deleted");
    assert_eq!(page.items[3].additional["syntheticExtension"], "retained");
    let new_checkpoint = page.meta.delta_link.unwrap();
    assert!(incremental.next_page().await.unwrap().is_none());
    assert_ne!(checkpoint, new_checkpoint);
    let no_changes = later
        .resume_users(&new_checkpoint)
        .unwrap()
        .fetch(None)
        .await
        .unwrap();
    assert!(no_changes.items.is_empty());
    assert_eq!(no_changes.meta.delta_link.unwrap(), new_checkpoint); // repeated delta is legal

    let requests = server.requests();
    assert_eq!(requests.len(), 5);
    assert_get(
        &requests[0],
        "/v1.0/users/delta?%24select=accountEnabled%2CdisplayName%2Cid%2Cmail",
        "tenant-a",
    );
    assert_get(
        &requests[1],
        "/v1.0/users/delta?$skiptoken=synthetic%2Bpage%2F2%3D&opaque=a+b&opaque=%252F",
        "tenant-a",
    );
    assert_get(
        &requests[2],
        "/v1.0/users/delta?$skiptoken=synthetic-final",
        "tenant-a",
    );
    assert_eq!(
        requests[3].target,
        "/v1.0/users/microsoft.graph.delta?$deltatoken=synthetic%2Bcheckpoint%2Fone%3D"
    );
    assert_eq!(
        requests[3].header("authorization"),
        Some("Bearer synthetic-refreshed-token")
    );
    assert_eq!(
        requests[4].target,
        "/v1.0/users/delta?$deltatoken=synthetic-next-round"
    );
    assert!(
        requests
            .iter()
            .all(|r| r.method == "GET" && r.body_len == 0)
    );
    server.shutdown().await;
}

#[tokio::test]
async fn workflow_b_groups_and_direct_members_associate_only_known_user_objects() {
    let server = TestServer::start(|r| match r.path() {
        "/v1.0/users/delta" => delta_reply(r),
        "/v1.0/groups" if r.query().is_some_and(|q| q.contains("skiptoken")) => {
            reply(r, GROUPS_FINAL)
        }
        "/v1.0/groups" => reply(r, GROUPS),
        "/v1.0/groups/group-one/members" if r.query().is_some_and(|q| q.contains("skiptoken")) => {
            reply(r, MEMBERS_FINAL)
        }
        "/v1.0/groups/group-one/members" => reply(r, MEMBERS),
        "/v1.0/groups/group-two/members" => reply(r, MEMBERS_TWO),
        _ => panic!("unexpected operation or transitive request: {}", r.target),
    })
    .await;
    let api = client(&server, "tenant-a");
    let user_pages = api
        .users_delta(&[UserField::DisplayName])
        .unwrap()
        .pages()
        .collect_complete()
        .await
        .unwrap();
    let users: BTreeSet<_> = user_pages
        .into_iter()
        .flat_map(|p| p.items)
        .map(|u| u.id)
        .collect();
    let groups = api
        .groups(&[GroupField::DisplayName])
        .unwrap()
        .pages()
        .items()
        .collect_complete()
        .await
        .unwrap();
    let mut associations = BTreeMap::new();
    let mut member_results = BTreeMap::new();
    for group in groups {
        let members = api
            .direct_members(&group.id, &[MemberField::DisplayName])
            .unwrap()
            .pages()
            .items()
            .collect_complete()
            .await
            .unwrap();
        let accessible: BTreeSet<_> = members
            .iter()
            .filter(|m| m.kind() == ObjectKind::User && users.contains(&m.id))
            .map(|m| m.id.clone())
            .collect();
        associations.insert(group.id.clone(), accessible);
        member_results.insert(group.id, members);
    }
    assert_eq!(
        associations["group-one"],
        BTreeSet::from(["user-a".into(), "user-c".into()])
    );
    assert_eq!(associations["group-two"], BTreeSet::from(["user-a".into()]));
    let members = &member_results["group-one"];
    assert_eq!(members.len(), 7);
    assert_eq!(members[1].kind(), ObjectKind::User);
    assert_eq!(members[1].display_name, None);
    assert!(members[1].additional["userPrincipalName"].is_null());
    assert_eq!(members[2].kind(), ObjectKind::Device);
    assert_eq!(
        members[3].kind(),
        ObjectKind::Other("#microsoft.graph.syntheticFutureType")
    );
    assert_eq!(members[3].additional["syntheticProperty"]["kept"], true);
    assert_eq!(members[5].kind(), ObjectKind::Group);
    assert_eq!(members[6].kind(), ObjectKind::OrganizationalContact);

    let requests = server.requests();
    assert_eq!(requests.len(), 8);
    assert_get(
        &requests[3],
        "/v1.0/groups?%24select=displayName%2Cid",
        "tenant-a",
    );
    assert_get(
        &requests[4],
        "/v1.0/groups?$skiptoken=synthetic%2Bgroups%3D",
        "tenant-a",
    );
    assert_get(
        &requests[5],
        "/v1.0/groups/group-one/members?%24select=displayName%2Cid&%24count=true",
        "tenant-a",
    );
    assert_get(
        &requests[6],
        "/v1.0/groups/group-one/members?$skiptoken=synthetic%2Fmembers%3D&opaque=%2b+",
        "tenant-a",
    );
    assert_get(
        &requests[7],
        "/v1.0/groups/group-two/members?%24select=displayName%2Cid&%24count=true",
        "tenant-a",
    );
    assert!(
        requests[5..]
            .iter()
            .all(|r| r.header("consistencylevel") == Some("eventual"))
    );
    server.shutdown().await;
}

#[tokio::test]
async fn two_tenants_and_principal_profiles_cannot_exchange_links() {
    let server = TestServer::start(|r| {
        let tenant = if r.header("authorization") == Some("Bearer synthetic-token-tenant-a") {
            "tenant-a"
        } else {
            "tenant-b"
        };
        let base = format!("http://{}/v1.0", r.header("host").unwrap());
        Reply::json(
            200,
            json!({"value":[{"id":format!("{tenant}-user")}],
            "@odata.deltaLink":format!("{base}/users/delta?$deltatoken=synthetic-{tenant}")})
            .to_string(),
        )
    })
    .await;
    let a = client(&server, "tenant-a");
    let b = client(&server, "tenant-b");
    let first = a.users_delta(&[]).unwrap().fetch(None).await.unwrap();
    let second = b.users_delta(&[]).unwrap().fetch(None).await.unwrap();
    assert_eq!(first.items[0].id, "tenant-a-user");
    assert_eq!(second.items[0].id, "tenant-b-user");
    let checkpoint_a = first.meta.delta_link.unwrap();
    let checkpoint_b = second.meta.delta_link.unwrap();
    for (client, checkpoint) in [(&a, &checkpoint_b), (&b, &checkpoint_a)] {
        assert!(matches!(
            client.resume_users(checkpoint),
            Err(GraphError::Link {
                problem: LinkProblem::BindingMismatch,
                ..
            })
        ));
        assert!(client.restore_delta(&checkpoint.to_json()).is_err());
    }
    let other_principal = GraphClient::for_loopback(
        AuthContext::new("tenant-a", "synthetic-other-permissions").unwrap(),
        "synthetic-other-token",
        &format!("{}/v1.0", server.url()),
    )
    .unwrap();
    assert!(
        other_principal
            .restore_delta(&checkpoint_a.to_json())
            .is_err()
    );
    let other_server = TestServer::start(|_| panic!("must reject other base before sending")).await;
    assert!(
        client(&other_server, "tenant-a")
            .restore_delta(&checkpoint_a.to_json())
            .is_err()
    );
    assert_eq!(server.requests().len(), 2);
    assert!(other_server.requests().is_empty());
    server.shutdown().await;
    other_server.shutdown().await;
}

#[tokio::test]
async fn next_links_are_bound_to_selection_group_and_kind() {
    let server = TestServer::start(|r| match r.path() {
        "/v1.0/users/delta" => reply(r, INITIAL),
        _ => reply(r, MEMBERS),
    })
    .await;
    let api = client(&server, "tenant-a");
    let selected = api.users_delta(&[UserField::DisplayName]).unwrap();
    let next = selected.fetch(None).await.unwrap().next.unwrap();
    assert!(
        api.users_delta(&[UserField::Mail])
            .unwrap()
            .fetch(Some(&next))
            .await
            .is_err()
    );
    assert!(api.restore_delta(&next.to_json()).is_err());
    assert!(
        api.groups(&[])
            .unwrap()
            .restore_next(&next.to_json())
            .is_err()
    );
    assert!(
        client(&server, "tenant-b")
            .users_delta(&[UserField::DisplayName])
            .unwrap()
            .restore_next(&next.to_json())
            .is_err()
    );
    let members = api.direct_members("group-one", &[]).unwrap();
    let member_next = members.fetch(None).await.unwrap().next.unwrap();
    assert!(
        api.direct_members("group-two", &[])
            .unwrap()
            .fetch(Some(&member_next))
            .await
            .is_err()
    );
    assert_eq!(server.requests().len(), 2);
    for malformed in ["{}", "null", "[]", "{", "\"not a checkpoint\""] {
        assert!(selected.restore_next(malformed).is_err());
        assert!(api.restore_delta(malformed).is_err());
    }
    let mut wrong_version: serde_json::Value = serde_json::from_str(&next.to_json()).unwrap();
    wrong_version["version"] = 2.into();
    assert!(selected.restore_next(&wrong_version.to_string()).is_err());
    server.shutdown().await;
}

#[tokio::test]
async fn bounded_delta_retains_prefix_and_next_checkpoint_but_no_final_delta() {
    let server = TestServer::start(delta_reply).await;
    let api = client(&server, "tenant-a");
    let listing = api.users_delta(&[UserField::DisplayName]).unwrap();
    let incomplete = listing
        .pages()
        .max_pages(NonZeroUsize::new(2).unwrap())
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.error.outcome(), Outcome::Bounded);
    assert_eq!(incomplete.collected.len(), 2);
    assert_eq!(incomplete.collected[0].items.len(), 2);
    assert!(incomplete.collected[1].items.is_empty());
    assert!(
        incomplete
            .collected
            .iter()
            .all(|p| p.meta.delta_link.is_none())
    );
    let resume = incomplete.error.resume().unwrap().key().unwrap();
    let restored = listing.restore_next(&resume.to_json()).unwrap();
    let remaining = listing
        .pages_from(Resume::At(restored))
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].items[0].id, "user-c");
    assert!(remaining[0].meta.delta_link.is_some());
    assert_eq!(server.requests().len(), 3);
    server.shutdown().await;
}

#[tokio::test]
async fn member_page_failure_keeps_prefix_and_typed_error_and_ends_failed() {
    let server = TestServer::start(|r| {
        if r.query().is_some_and(|q| q.contains("skiptoken")) {
            Reply::json(403, include_bytes!("../fixtures/error-forbidden.json"))
                .header("request-id", "synthetic-member-denial")
        } else {
            reply(r, MEMBERS)
        }
    })
    .await;
    let listing = client(&server, "tenant-a")
        .direct_members("group-one", &[])
        .unwrap();
    let incomplete = listing
        .pages()
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 4);
    assert_eq!(incomplete.error.outcome(), Outcome::Failed);
    match &incomplete.error {
        PagesError::Provider { error, failed, .. } => {
            assert_eq!(error.class(), ErrorClass::Permission);
            assert!(matches!(failed, Resume::At(_)));
            assert_eq!(
                error.meta().unwrap().headers()["request-id"],
                "synthetic-member-denial"
            );
        }
        _ => panic!("expected failed provider page"),
    }
    let mut failed = listing.pages_from(incomplete.error.resume().unwrap().clone());
    assert!(matches!(
        failed.next_page().await,
        Err(PagesError::Provider { .. })
    ));
    assert!(matches!(
        failed.next_page().await,
        Err(PagesError::Ended {
            outcome: Outcome::Failed,
            ..
        })
    ));
    assert_eq!(failed.finish().outcome, Outcome::Failed);
    server.shutdown().await;
}

#[tokio::test]
async fn standard_item_stream_emits_failure_before_ending() {
    let server = TestServer::start(|r| {
        if r.query().is_some() {
            Reply::json(429, include_bytes!("../fixtures/error-throttled.json"))
        } else {
            reply(r, GROUPS)
        }
    })
    .await;
    let mut stream = client(&server, "tenant-a")
        .groups(&[])
        .unwrap()
        .pages()
        .items()
        .into_stream();
    assert_eq!(stream.next().await.unwrap().unwrap().id, "group-one");
    let error = stream.next().await.unwrap().unwrap_err();
    assert!(
        matches!(error, PagesError::Provider { ref error, .. } if error.class() == ErrorClass::Throttled)
    );
    assert!(stream.next().await.is_none());
    assert_eq!(stream.into_inner().finish().outcome, Outcome::Failed);
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn multipage_cycle_stops_at_cap_and_exact_last_page_at_cap_completes() {
    let server = TestServer::start(|r| {
        let token = if r.query().unwrap_or("").contains("synthetic-a") { "synthetic-b" } else { "synthetic-a" };
        Reply::json(200, json!({"value":[],"@odata.nextLink":format!("http://{}/v1.0/groups?$skiptoken={token}", r.header("host").unwrap())}).to_string())
    }).await;
    let mut pages = client(&server, "tenant-a")
        .groups(&[])
        .unwrap()
        .pages()
        .max_pages(NonZeroUsize::new(3).unwrap());
    for _ in 0..3 {
        assert!(pages.next_page().await.unwrap().is_some());
    }
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Bounded { .. })
    ));
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended {
            outcome: Outcome::Bounded,
            ..
        })
    ));
    assert_eq!(server.requests().len(), 3);
    server.shutdown().await;

    let server = TestServer::start(|r| reply(r, GROUPS_FINAL)).await;
    let complete = client(&server, "tenant-a")
        .groups(&[])
        .unwrap()
        .pages()
        .max_pages(NonZeroUsize::new(1).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(complete.len(), 1);
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn cancelling_next_page_keeps_one_request_in_flight_and_drop_is_not_exhaustion() {
    let gate = Gate::new();
    let response_gate = gate.clone();
    let server =
        TestServer::start(move |r| reply(r, GROUPS_FINAL).after(response_gate.clone())).await;
    let mut pages = client(&server, "tenant-a").groups(&[]).unwrap().pages();
    assert!(
        tokio::time::timeout(Duration::from_millis(20), pages.next_page())
            .await
            .is_err()
    );
    gate.open();
    assert_eq!(pages.next_page().await.unwrap().unwrap().items.len(), 1);
    assert!(pages.next_page().await.unwrap().is_none());
    assert_eq!(pages.finish().outcome, Outcome::Exhausted);
    assert_eq!(server.requests().len(), 1);
    let unstarted = client(&server, "tenant-a").groups(&[]).unwrap().pages();
    assert_ne!(unstarted.finish().outcome, Outcome::Exhausted);
    server.shutdown().await;
}
