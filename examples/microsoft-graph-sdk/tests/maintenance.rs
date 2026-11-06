//! Focused wire regressions for empty delta pages and limited/unknown member types.
mod support;

use std::num::NonZeroUsize;

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_microsoft_graph::{
    DirectoryObject, ErrorClass, GraphError, MemberField, ObjectKind, UserField,
    pagination::{Outcome, PagesError, Resume},
};
use futures_util::StreamExt;
use support::*;

const EMPTY_OPAQUE: &[u8] = include_bytes!("../fixtures/users-empty-opaque-next.json");
const LIMITED_MEMBERS: &[u8] = include_bytes!("../fixtures/members-limited-unknown.json");
const EMPTY_TARGET: &str = "/v1.0/users/delta?$skiptoken=synthetic-final";
const AFTER_EMPTY_TARGET: &str = "/v1.0/users/microsoft.graph.delta?$skiptoken=synthetic%2Bempty%2Fnext%3D&opaque=%2b+&opaque=%252F";
const DELTA_TARGET: &str =
    "/v1.0/users/microsoft.graph.delta?$deltatoken=synthetic%2Bcheckpoint%2Fone%3D";
const MEMBER_TARGET: &str = "/v1.0/groups/group-one/members?$skiptoken=synthetic%2Blimited%2Fnext%3D&opaque=%2b+&opaque=%252F";

fn delta_route(request: &Recorded) -> Reply {
    match request.target.as_str() {
        AFTER_EMPTY_TARGET => reply(request, FINAL),
        EMPTY_TARGET => reply(request, EMPTY_OPAQUE),
        DELTA_TARGET => reply(request, EMPTY),
        "/v1.0/users/delta?$skiptoken=synthetic%2Bpage%2F2%3D&opaque=a+b&opaque=%252F" => {
            reply(request, EMPTY)
        }
        "/v1.0/users/delta?%24select=displayName%2Cid" => reply(request, INITIAL),
        _ => Reply::json(404, include_bytes!("../fixtures/error-not-found.json")),
    }
}

#[tokio::test]
async fn consecutive_empty_delta_pages_continue_in_initial_and_incremental_rounds() {
    let server = TestServer::start(delta_route).await;
    let api = client(&server, "tenant-a");
    let mut pages = api.users_delta(&[UserField::DisplayName]).unwrap().pages();
    let mut sizes = Vec::new();
    let mut checkpoint = None;
    while let Some(page) = pages.next_page().await.unwrap() {
        sizes.push(page.items.len());
        assert_eq!(page.meta.response.status().as_u16(), 200);
        assert_ne!(page.next.is_some(), page.meta.delta_link.is_some());
        if let Some(delta) = page.meta.delta_link {
            checkpoint = Some(delta);
        }
    }
    assert_eq!(sizes, [2, 0, 0, 1]);
    assert_eq!(pages.finish().outcome, Outcome::Exhausted);
    let checkpoint = checkpoint.unwrap();
    assert_eq!(checkpoint.url(), format!("{}{DELTA_TARGET}", server.url()));

    // The same rule applies when the very first page of a later round is empty.
    let later = client(&server, "tenant-a");
    let restored = later.restore_delta(&checkpoint.to_json()).unwrap();
    let incremental = later
        .resume_users(&restored)
        .unwrap()
        .pages()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(
        incremental
            .iter()
            .map(|page| page.items.len())
            .collect::<Vec<_>>(),
        [0, 0, 1]
    );
    assert!(
        incremental[..2]
            .iter()
            .all(|page| page.meta.delta_link.is_none())
    );
    assert_eq!(incremental[2].meta.delta_link.as_ref(), Some(&checkpoint));

    let requests = server.requests();
    let targets = [
        "/v1.0/users/delta?%24select=displayName%2Cid",
        "/v1.0/users/delta?$skiptoken=synthetic%2Bpage%2F2%3D&opaque=a+b&opaque=%252F",
        EMPTY_TARGET,
        AFTER_EMPTY_TARGET,
        DELTA_TARGET,
        EMPTY_TARGET,
        AFTER_EMPTY_TARGET,
    ];
    assert_eq!(requests.len(), targets.len());
    for (request, target) in requests.iter().zip(targets) {
        assert_get(request, target, "tenant-a");
    }
    server.shutdown().await;
}

#[tokio::test]
async fn cap_after_consecutive_empty_pages_keeps_the_exact_unread_link_for_a_new_client() {
    let server = TestServer::start(delta_route).await;
    let listing = client(&server, "tenant-a")
        .users_delta(&[UserField::DisplayName])
        .unwrap();
    let incomplete = listing
        .pages()
        .max_pages(NonZeroUsize::new(3).unwrap())
        .collect_complete()
        .await
        .unwrap_err();
    assert!(matches!(incomplete.error, PagesError::Bounded { .. }));
    assert_eq!(incomplete.collected.len(), 3);
    assert!(
        incomplete.collected[1..]
            .iter()
            .all(|page| page.items.is_empty())
    );
    assert!(
        incomplete
            .collected
            .iter()
            .all(|page| page.meta.delta_link.is_none())
    );
    assert_eq!(server.requests().len(), 3);
    let next = incomplete.error.resume().unwrap().key().unwrap();
    assert_eq!(next.url(), format!("{}{AFTER_EMPTY_TARGET}", server.url()));
    let saved = next.to_json();
    drop(listing);

    let restored_listing = client(&server, "tenant-a")
        .users_delta(&[UserField::DisplayName])
        .unwrap();
    let restored = restored_listing.restore_next(&saved).unwrap();
    let remaining = restored_listing
        .pages_from(Resume::At(restored))
        .max_pages(NonZeroUsize::new(1).unwrap())
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].items[0].id, "user-c");
    assert_eq!(
        remaining[0].meta.delta_link.as_ref().unwrap().url(),
        format!("{}{DELTA_TARGET}", server.url())
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    assert_get(&requests[3], AFTER_EMPTY_TARGET, "tenant-a");
    server.shutdown().await;
}

#[tokio::test]
async fn failure_after_empty_delta_pages_preserves_the_prefix_and_typed_error() {
    let server = TestServer::start(|request| {
        if request.target == AFTER_EMPTY_TARGET {
            Reply::json(429, include_bytes!("../fixtures/error-throttled.json"))
                .header("retry-after", "11")
                .header("request-id", "synthetic-after-empty-throttle")
        } else {
            delta_route(request)
        }
    })
    .await;
    let listing = client(&server, "tenant-a")
        .users_delta(&[UserField::DisplayName])
        .unwrap();
    let incomplete = listing.pages().collect_complete().await.unwrap_err();
    assert_eq!(
        incomplete
            .collected
            .iter()
            .map(|page| page.items.len())
            .collect::<Vec<_>>(),
        [2, 0, 0]
    );
    assert!(
        incomplete
            .collected
            .iter()
            .all(|page| page.meta.delta_link.is_none())
    );
    let PagesError::Provider { error, failed, .. } = &incomplete.error else {
        panic!("expected a failed page after the empty pages");
    };
    assert!(matches!(error, GraphError::Provider { .. }));
    assert_eq!(error.class(), ErrorClass::Throttled);
    assert_eq!(error.provider().unwrap().code, "TooManyRequests");
    let meta = error.meta().unwrap();
    assert_eq!(meta.status().as_u16(), 429);
    assert_eq!(meta.headers()["retry-after"], "11");
    assert_eq!(
        meta.headers()["request-id"],
        "synthetic-after-empty-throttle"
    );
    assert_eq!(
        failed.key().unwrap().url(),
        format!("{}{AFTER_EMPTY_TARGET}", server.url())
    );
    assert_eq!(server.requests().len(), 4); // no automatic retry or reset

    let mut retry = listing.pages_from(failed.clone());
    assert!(matches!(
        retry.next_page().await,
        Err(PagesError::Provider { .. })
    ));
    assert!(matches!(
        retry.next_page().await,
        Err(PagesError::Ended {
            outcome: Outcome::Failed,
            ..
        })
    ));
    assert_eq!(retry.finish().outcome, Outcome::Failed);
    let requests = server.requests();
    assert_eq!(requests.len(), 5);
    assert_get(&requests[3], AFTER_EMPTY_TARGET, "tenant-a");
    assert_get(&requests[4], AFTER_EMPTY_TARGET, "tenant-a");
    server.shutdown().await;
}

fn assert_limited_members(members: &[DirectoryObject]) {
    assert_eq!(members.len(), 3);
    assert_eq!(members[0].id, "synthetic-limited-device");
    assert_eq!(members[0].object_type, "#microsoft.graph.device");
    assert_eq!(members[0].kind(), ObjectKind::Device);
    assert!(members[0].additional["deviceId"].is_null());
    assert_eq!(members[1].id, "synthetic-type-only");
    assert_eq!(members[2].id, "synthetic-null-profile");
    for member in &members[1..] {
        assert_eq!(
            member.object_type,
            "#microsoft.graph.syntheticFutureLimitedType"
        );
        assert_eq!(member.kind(), ObjectKind::Other(&member.object_type));
    }
    assert!(
        members
            .iter()
            .all(|member| member.display_name.is_none() && member.mail.is_none())
    );
    assert!(members[1].additional.is_empty());
    assert!(members[2].additional["syntheticProperty"].is_null());
}

#[tokio::test]
async fn limited_unknown_members_survive_a_cap_and_continue_from_a_saved_link() {
    let server = TestServer::start(|request| {
        if request.target == MEMBER_TARGET {
            reply(request, MEMBERS_FINAL)
        } else {
            reply(request, LIMITED_MEMBERS)
        }
    })
    .await;
    let listing = client(&server, "tenant-a")
        .direct_members("group-one", &[MemberField::DisplayName])
        .unwrap();
    let incomplete = listing
        .pages()
        .max_pages(NonZeroUsize::new(1).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert!(matches!(incomplete.error, PagesError::Bounded { .. }));
    assert_limited_members(&incomplete.collected);
    assert_eq!(server.requests().len(), 1);
    let next = incomplete.error.resume().unwrap().key().unwrap();
    assert_eq!(next.url(), format!("{}{MEMBER_TARGET}", server.url()));
    let saved = next.to_json();
    drop(listing);

    let later = client(&server, "tenant-a")
        .direct_members("group-one", &[MemberField::DisplayName])
        .unwrap();
    let restored = later.restore_next(&saved).unwrap();
    let tail = later
        .pages_from(Resume::At(restored))
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(tail.len(), 3);
    assert_eq!(tail[0].kind(), ObjectKind::User);
    assert_eq!(tail[1].kind(), ObjectKind::Group);
    assert_eq!(tail[2].kind(), ObjectKind::OrganizationalContact);
    let mut all = incomplete.collected;
    all.extend(tail);
    assert_limited_members(&all[..3]);

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_get(
        &requests[0],
        "/v1.0/groups/group-one/members?%24select=displayName%2Cid&%24count=true",
        "tenant-a",
    );
    assert_get(&requests[1], MEMBER_TARGET, "tenant-a");
    assert!(
        requests
            .iter()
            .all(|request| request.header("consistencylevel") == Some("eventual"))
    );
    server.shutdown().await;
}

#[tokio::test]
async fn member_stream_keeps_limited_unknown_objects_and_reports_a_later_failure() {
    let server = TestServer::start(|request| {
        if request.target == MEMBER_TARGET {
            Reply::json(403, include_bytes!("../fixtures/error-forbidden.json"))
        } else {
            reply(request, LIMITED_MEMBERS)
        }
    })
    .await;
    let mut stream = client(&server, "tenant-a")
        .direct_members("group-one", &[])
        .unwrap()
        .pages()
        .items()
        .into_stream();
    let mut members = Vec::new();
    for _ in 0..3 {
        members.push(stream.next().await.unwrap().unwrap());
    }
    assert_limited_members(&members);
    let failure = stream.next().await.unwrap().unwrap_err();
    let PagesError::Provider { error, failed, .. } = &failure else {
        panic!("expected the member continuation failure");
    };
    assert_eq!(error.class(), ErrorClass::Permission);
    assert_eq!(
        error.provider().unwrap().code,
        "Authorization_RequestDenied"
    );
    assert_eq!(error.meta().unwrap().status().as_u16(), 403);
    assert_eq!(
        failed.key().unwrap().url(),
        format!("{}{MEMBER_TARGET}", server.url())
    );
    assert!(stream.next().await.is_none());
    assert_eq!(stream.into_inner().finish().outcome, Outcome::Failed);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_get(&requests[0], "/v1.0/groups/group-one/members", "tenant-a");
    assert_get(&requests[1], MEMBER_TARGET, "tenant-a");
    assert!(
        requests
            .iter()
            .all(|request| request.header("consistencylevel").is_none())
    );
    server.shutdown().await;
}
