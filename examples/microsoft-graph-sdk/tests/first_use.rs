//! First-use application suite for `concord-sdk-microsoft-graph`, written by an independent
//! consumer from the README, the rustdoc and the Microsoft Learn pages, against a fake Graph
//! served by `concord_example_testserver`.
//!
//! Outcome A: sync users through delta and reuse the final delta link.
//! Outcome B: enumerate groups and their direct members.
//!
//! Bodies are hand-authored from the documentation (see `FIRST_USE.md`, "Fixture provenance");
//! `{origin}` is replaced by the loopback origin the request arrived at. Values marked
//! "invented" are not in the documentation.

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_microsoft_graph::pagination::{Incomplete, Outcome, PagesError, Resume};
use concord_sdk_microsoft_graph::{
    AuthContext, DeltaLink, DirectoryObject, ErrorBody, ErrorClass, Field, GraphClient, GraphError,
    Group, GroupField, LinkProblem, Listing, MemberField, NextLink, ObjectKind, ProtocolProblem,
    UserDelta, UserField,
};
use futures_util::StreamExt;

// ---------------------------------------------------------------------------------------------
// Fake Graph plumbing
// ---------------------------------------------------------------------------------------------

const TOKEN: &str = "synthetic-token-not-a-credential";
const REFRESHED_TOKEN: &str = "synthetic-token-refreshed-not-a-credential";
const TENANT: &str = "synthetic-tenant";
const PRINCIPAL: &str = "synthetic-app-permissions-v1";

fn context() -> AuthContext {
    AuthContext::new(TENANT, PRINCIPAL).expect("synthetic context")
}

fn client_with(server: &TestServer, ctx: AuthContext, token: &str) -> GraphClient {
    GraphClient::for_loopback(ctx, token, &format!("{}/v1.0", server.url()))
        .expect("loopback client")
}

fn client(server: &TestServer) -> GraphClient {
    client_with(server, context(), TOKEN)
}

/// The origin the request was sent to, as Graph would put it in its links.
fn origin(req: &Recorded) -> String {
    format!("http://{}", req.host)
}

fn ok(req: &Recorded, body: &str) -> Reply {
    Reply::json(200, body.replace("{origin}", &origin(req)))
}

/// A Graph error document (envelope from `concepts/errors.md`).
fn graph_error(status: u16, code: &str, message: &str) -> Reply {
    Reply::json(
        status,
        format!(
            r#"{{"error":{{"code":"{code}","message":"{message}","innerError":{{"date":"2026-11-06T08:00:00","request-id":"0000aaaa-1111-2222-3333-444455556666","client-request-id":"0000aaaa-1111-2222-3333-444455556666"}}}}}}"#
        ),
    )
    .header("request-id", "0000aaaa-1111-2222-3333-444455556666")
}

/// The fake answers an unexpected request with a 404 document instead of panicking in the
/// server task, so a routing mistake shows up as a provider error in the test.
fn no_route() -> Reply {
    graph_error(
        404,
        "fakeGraphNoRoute",
        "The fake Graph has no route for this request.",
    )
}

fn query_value(req: &Recorded, name: &str) -> Option<String> {
    req.query_all(name).into_iter().next()
}

/// Every Graph request in this suite is an authenticated GET without a body.
fn assert_get(req: &Recorded, token: &str) {
    assert_eq!(req.method, "GET");
    assert_eq!(
        req.header("authorization"),
        Some(&*format!("Bearer {token}"))
    );
    assert!(req.body.is_empty());
}

// ---------------------------------------------------------------------------------------------
// Outcome A fixtures (concepts/delta-query-users.md)
// ---------------------------------------------------------------------------------------------

const SKIP_1: &str = "oEBwdSP6uehIAxQOWq_3Ksh_TLol6KIm3stvdc6hGhZRi1hQ7Spe__dpvm3U4zReE4CYXC2zOtaKdi7KHlUtC2CbRiBIUwOxPKLa";
const SKIP_2: &str = "pqwSUjGYvb3jQpbwVAwEL7yuI3dU1LecfkkfLPtnIjtQ5LOhVoS7qQG_wdVCHHlbQpga7";
const DELTA_1: &str = "oEcOySpF_hWYmTIUZBOIfPzcwisr_rPe8o9M54L45qEXQGmvQC6T2dbL-9O7nSU-njKhFiGlAZqewNAThmCVnNxqPu5gOBegrm1CaVZ-ZtFZ2tPOAO98OD9y0ao460";
const DELTA_2: &str = "MF1LuFYbK6Lw4DtZ4o9PDrcGekRP65WEJfDmM0H26l4v9zILCPFiPwSAAeRBghxgiwsXEfywcVQ9R8VEWuYAB50Yw3KvJ-8Z1zamVotGX2b_AHVS_Z-3b0NAtmGpod";
/// Invented: a skip token inside an incremental round.
const SKIP_INCREMENTAL: &str = "syntheticIncrementalSkipToken_0001";

const USERS_FIELDS: [UserField; 3] = [
    UserField::DisplayName,
    UserField::GivenName,
    UserField::Surname,
];

const USERS_PAGE_1: &str = r#"{
  "@odata.context":"{origin}/v1.0/$metadata#users(displayName,givenName,surname)",
  "@odata.nextLink":"{origin}/v1.0/users/delta?$skiptoken=oEBwdSP6uehIAxQOWq_3Ksh_TLol6KIm3stvdc6hGhZRi1hQ7Spe__dpvm3U4zReE4CYXC2zOtaKdi7KHlUtC2CbRiBIUwOxPKLa",
  "value": [
    {"displayName":"Cameron White","givenName":"Cameron","surname":"White","id":"ffff7b1a-13b6-477b-8c0c-380905cd99f7"},
    {"displayName":"Delia Dennis","givenName":"Delia","surname":"Dennis","id":"605d1257-ffff-40b6-8e6f-528a53f5dc55"},
    {"id": "86462606-fde0-4fc4-9e0c-a20eb73e54c6","@removed": {"reason": "deleted"}},
    {"displayName": "Conf Room Adams","id": "6ea91a8d-e32e-41a1-b7bd-d2d185eed0e0"}
  ]
}"#;

const USERS_PAGE_2: &str = r#"{
  "@odata.context":"{origin}/v1.0/$metadata#users",
  "@odata.nextLink":"{origin}/v1.0/users/delta?$skiptoken=pqwSUjGYvb3jQpbwVAwEL7yuI3dU1LecfkkfLPtnIjtQ5LOhVoS7qQG_wdVCHHlbQpga7",
  "value": [
    {"displayName":"Mallory Cortez","givenName":"Mallory","surname":"Cortez","id":"d8c37826-ffff-4cae-b348-e2725b1e814b"},
    {"displayName":"Diego Sicilian","givenName":"Diego","surname":"Sicilian","id":"8b1ee412-cd8f-4d59-ffff-24010edb9f1f"}
  ]
}"#;

const USERS_PAGE_3: &str = r#"{
  "@odata.context":"{origin}/v1.0/$metadata#users",
  "@odata.deltaLink":"{origin}/v1.0/users/delta?$deltatoken=oEcOySpF_hWYmTIUZBOIfPzcwisr_rPe8o9M54L45qEXQGmvQC6T2dbL-9O7nSU-njKhFiGlAZqewNAThmCVnNxqPu5gOBegrm1CaVZ-ZtFZ2tPOAO98OD9y0ao460",
  "value": [
    {"displayName":"Lidia Holloway","givenName":"Lidia","surname":"Holloway","id":"25dcffff-959e-4ece-9973-e5d9b800e8cc"},
    {"displayName":"Patti Fernandez","givenName":"Patti","surname":"Fernandez","id":"f6ede700-27d0-4c42-bfb9-4dffff43c74a"}
  ]
}"#;

/// Invented page: an incremental round that needs a next page, with a replayed user whose
/// surname is explicitly cleared to null.
const USERS_INCREMENTAL_1: &str = r#"{
  "@odata.context":"{origin}/v1.0/$metadata#users",
  "@odata.nextLink":"{origin}/v1.0/users/delta?$skiptoken=syntheticIncrementalSkipToken_0001",
  "value": [
    {"surname":null,"id":"f6ede700-27d0-4c42-bfb9-4dffff43c74a"}
  ]
}"#;

/// The documented "changes" deltaLink response; the removed id is replaced by one from the
/// initial round (Diego Sicilian) so the removal hits a stored user.
const USERS_INCREMENTAL_2: &str = r#"{
  "@odata.context":"{origin}/v1.0/$metadata#users",
  "@odata.deltaLink":"{origin}/v1.0/users/delta?$deltatoken=MF1LuFYbK6Lw4DtZ4o9PDrcGekRP65WEJfDmM0H26l4v9zILCPFiPwSAAeRBghxgiwsXEfywcVQ9R8VEWuYAB50Yw3KvJ-8Z1zamVotGX2b_AHVS_Z-3b0NAtmGpod",
  "value": [
    {"displayName":"MOD Administrator","givenName":"MOD","surname":"Administrator","id":"25dcffff-959e-4ece-9973-e5d9b800e8cc"},
    {"id":"8b1ee412-cd8f-4d59-ffff-24010edb9f1f","@removed": {"reason": "changed"}}
  ]
}"#;

/// The documented "no changes" deltaLink response (same delta token as the previous round).
const USERS_NO_CHANGES: &str = r#"{
  "@odata.context":"{origin}/v1.0/$metadata#users",
  "@odata.deltaLink":"{origin}/v1.0/users/delta?$deltatoken=MF1LuFYbK6Lw4DtZ4o9PDrcGekRP65WEJfDmM0H26l4v9zILCPFiPwSAAeRBghxgiwsXEfywcVQ9R8VEWuYAB50Yw3KvJ-8Z1zamVotGX2b_AHVS_Z-3b0NAtmGpod",
  "value": []
}"#;

/// The documented users-delta conversation: initial round (3 pages), an incremental round
/// (2 pages) and a no-change round.
fn users_delta_route(req: &Recorded) -> Reply {
    if req.path() != "/v1.0/users/delta" {
        return no_route();
    }
    if let Some(skip) = query_value(req, "$skiptoken") {
        return match skip.as_str() {
            SKIP_1 => ok(req, USERS_PAGE_2),
            SKIP_2 => ok(req, USERS_PAGE_3),
            SKIP_INCREMENTAL => ok(req, USERS_INCREMENTAL_2),
            _ => no_route(),
        };
    }
    if let Some(delta) = query_value(req, "$deltatoken") {
        return match delta.as_str() {
            DELTA_1 => ok(req, USERS_INCREMENTAL_1),
            DELTA_2 => ok(req, USERS_NO_CHANGES),
            _ => no_route(),
        };
    }
    ok(req, USERS_PAGE_1)
}

// ---------------------------------------------------------------------------------------------
// Outcome A application code
// ---------------------------------------------------------------------------------------------

/// What the application keeps per user (selected properties only).
#[derive(Debug, Default, Clone, PartialEq)]
struct LocalUser {
    display_name: Option<String>,
    given_name: Option<String>,
    surname: Option<String>,
}

fn apply(slot: &mut Option<String>, field: &Field<String>) {
    match field {
        Field::Missing => {}
        Field::Null => *slot = None,
        Field::Value(value) => *slot = Some(value.clone()),
    }
}

/// Merges sparse changes by id: `@removed` (either reason) deletes the local record.
fn merge(store: &mut BTreeMap<String, LocalUser>, changes: &[UserDelta]) {
    for change in changes {
        if change.removed.is_some() {
            store.remove(&change.id);
            continue;
        }
        let user = store.entry(change.id.clone()).or_default();
        apply(&mut user.display_name, &change.display_name);
        apply(&mut user.given_name, &change.given_name);
        apply(&mut user.surname, &change.surname);
    }
}

struct Round {
    changes: Vec<UserDelta>,
    checkpoint: DeltaLink,
    pages: usize,
}

/// The traversal error of every listing. Boxed in application signatures: unboxed it trips
/// `clippy::result_large_err` (at least 256 bytes).
type PagesFailure = Box<PagesError<GraphError, NextLink>>;

/// Reads one delta round page by page (the README's "large round" shape): staging each page,
/// and keeping only the final page's delta link as the checkpoint.
async fn read_round(listing: &Listing<UserDelta>) -> Result<Round, PagesFailure> {
    let mut pages = listing.pages();
    let mut changes = Vec::new();
    let mut checkpoint = None;
    let mut count = 0;
    while let Some(page) = pages.next_page().await.map_err(Box::new)? {
        count += 1;
        // A page carries exactly one of the two links.
        assert_ne!(page.next.is_some(), page.meta.delta_link.is_some());
        assert_eq!(page.meta.response.status().as_u16(), 200);
        changes.extend(page.items);
        if let Some(delta) = page.meta.delta_link {
            checkpoint = Some(delta);
        }
    }
    Ok(Round {
        changes,
        checkpoint: checkpoint.expect("a completed round carries a delta link"),
        pages: count,
    })
}

fn user(display: &str, given: &str, surname: &str) -> LocalUser {
    LocalUser {
        display_name: Some(display.to_owned()),
        given_name: Some(given.to_owned()),
        surname: Some(surname.to_owned()),
    }
}

// ---------------------------------------------------------------------------------------------
// Outcome A tests
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_initial_round_then_later_rounds_reuse_the_final_delta_link() {
    let server = TestServer::start(users_delta_route).await;
    let origin = server.url();
    let graph = client(&server);
    let mut store = BTreeMap::new();

    // Initial round: the listing performs no I/O until it is read.
    let listing = graph
        .users_delta(&USERS_FIELDS)
        .expect("users delta listing");
    assert!(server.requests().is_empty());
    let initial = read_round(&listing).await.expect("initial round");
    assert_eq!(initial.pages, 3);
    assert_eq!(initial.changes.len(), 8);
    assert_eq!(
        initial.checkpoint.url(),
        format!("{origin}/v1.0/users/delta?$deltatoken={DELTA_1}")
    );

    // Sparse records: the removed user carries only its id and the marker; the room has no
    // given name or surname reported (Missing, not Null).
    let removed = &initial.changes[2];
    assert_eq!(removed.id, "86462606-fde0-4fc4-9e0c-a20eb73e54c6");
    assert_eq!(
        removed.removed.as_ref().map(|r| r.reason.as_str()),
        Some("deleted")
    );
    assert_eq!(removed.display_name, Field::Missing);
    let room = &initial.changes[3];
    assert_eq!(
        room.display_name,
        Field::Value("Conf Room Adams".to_owned())
    );
    assert_eq!(room.given_name, Field::Missing);
    assert_eq!(room.surname, Field::Missing);
    assert!(room.removed.is_none());

    merge(&mut store, &initial.changes);
    assert_eq!(store.len(), 7);

    // Wire: the first request selects the sorted fields plus id; continuation requests are
    // the exact next links, with no $select re-attached.
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    for req in &requests {
        assert_get(req, TOKEN);
        assert_eq!(req.path(), "/v1.0/users/delta");
        assert_eq!(req.header("consistencylevel"), None);
    }
    assert_eq!(
        requests[0].query_pairs(),
        [(
            "$select".to_owned(),
            "displayName,givenName,id,surname".to_owned()
        )]
    );
    assert_eq!(
        requests[1].target,
        format!("/v1.0/users/delta?$skiptoken={SKIP_1}")
    );
    assert_eq!(
        requests[2].target,
        format!("/v1.0/users/delta?$skiptoken={SKIP_2}")
    );

    // The application commits the changes and the checkpoint together. The stored form does
    // not contain the bearer token.
    let saved = initial.checkpoint.to_json();
    assert!(!saved.contains(TOKEN));
    drop(listing);

    // Later, a new process with a refreshed token under the same authorization context.
    let graph = client_with(&server, context(), REFRESHED_TOKEN);
    let restored = graph.restore_delta(&saved).expect("restore checkpoint");
    assert_eq!(restored, initial.checkpoint);
    let incremental = read_round(&graph.resume_users(&restored).expect("resume"))
        .await
        .expect("incremental round");
    assert_eq!(incremental.pages, 2);
    assert_eq!(incremental.changes.len(), 3);
    assert_eq!(incremental.changes[0].surname, Field::Null);
    assert_eq!(incremental.changes[0].display_name, Field::Missing);
    assert_eq!(
        incremental.changes[2]
            .removed
            .as_ref()
            .map(|r| r.reason.as_str()),
        Some("changed")
    );
    merge(&mut store, &incremental.changes);

    let requests = server.requests();
    assert_eq!(requests.len(), 5);
    assert_get(&requests[3], REFRESHED_TOKEN);
    assert_eq!(
        requests[3].target,
        format!("/v1.0/users/delta?$deltatoken={DELTA_1}")
    );
    assert_get(&requests[4], REFRESHED_TOKEN);
    assert_eq!(
        requests[4].target,
        format!("/v1.0/users/delta?$skiptoken={SKIP_INCREMENTAL}")
    );

    // A third round without changes: an empty final page is completion with a checkpoint,
    // and Graph may repeat the same delta link.
    let saved = incremental.checkpoint.to_json();
    let restored = graph
        .restore_delta(&saved)
        .expect("restore second checkpoint");
    let quiet = read_round(&graph.resume_users(&restored).expect("resume"))
        .await
        .expect("no-change round");
    assert_eq!(quiet.pages, 1);
    assert!(quiet.changes.is_empty());
    assert_eq!(quiet.checkpoint, incremental.checkpoint);
    assert_eq!(
        server.requests()[5].target,
        format!("/v1.0/users/delta?$deltatoken={DELTA_2}")
    );
    assert_eq!(server.requests().len(), 6);

    // The application's directory after three rounds.
    let expected: BTreeMap<String, LocalUser> = [
        (
            "ffff7b1a-13b6-477b-8c0c-380905cd99f7",
            user("Cameron White", "Cameron", "White"),
        ),
        (
            "605d1257-ffff-40b6-8e6f-528a53f5dc55",
            user("Delia Dennis", "Delia", "Dennis"),
        ),
        (
            "6ea91a8d-e32e-41a1-b7bd-d2d185eed0e0",
            LocalUser {
                display_name: Some("Conf Room Adams".to_owned()),
                ..LocalUser::default()
            },
        ),
        (
            "d8c37826-ffff-4cae-b348-e2725b1e814b",
            user("Mallory Cortez", "Mallory", "Cortez"),
        ),
        (
            "25dcffff-959e-4ece-9973-e5d9b800e8cc",
            user("MOD Administrator", "MOD", "Administrator"),
        ),
        (
            "f6ede700-27d0-4c42-bfb9-4dffff43c74a",
            LocalUser {
                display_name: Some("Patti Fernandez".to_owned()),
                given_name: Some("Patti".to_owned()),
                surname: None,
            },
        ),
    ]
    .into_iter()
    .map(|(id, user)| (id.to_owned(), user))
    .collect();
    assert_eq!(store, expected);
}

#[tokio::test]
async fn a_round_staged_at_a_page_boundary_resumes_in_a_new_process() {
    let server = TestServer::start(users_delta_route).await;

    // Stage the first page and persist its next link, then stop.
    let saved_next = {
        let graph = client(&server);
        let listing = graph.users_delta(&USERS_FIELDS).expect("listing");
        let mut pages = listing.pages();
        let first = pages
            .next_page()
            .await
            .expect("first page")
            .expect("not exhausted");
        assert_eq!(first.items.len(), 4);
        assert!(first.meta.delta_link.is_none());
        first.next.expect("next link").to_json()
    };
    assert!(!saved_next.contains(TOKEN));

    // A new process rebuilds the same listing (same context and selection) and continues.
    let graph = client_with(&server, context(), REFRESHED_TOKEN);
    let listing = graph.users_delta(&USERS_FIELDS).expect("listing");
    let next = listing
        .restore_next(&saved_next)
        .expect("restore next link");
    let rest = listing
        .pages()
        .starting_at(Resume::At(next))
        .collect_complete()
        .await
        .expect("rest of the round");
    assert_eq!(rest.len(), 2);
    assert!(rest[0].meta.delta_link.is_none());
    let checkpoint = rest[1].meta.delta_link.clone().expect("final checkpoint");
    assert!(checkpoint.url().ends_with(DELTA_1));
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests[1].target,
        format!("/v1.0/users/delta?$skiptoken={SKIP_1}")
    );
    assert_get(&requests[1], REFRESHED_TOKEN);

    // The stored link is bound to its listing and context; refusals send nothing.
    let other_selection = graph.users_delta(&[UserField::Mail]).expect("listing");
    assert!(matches!(
        other_selection.restore_next(&saved_next),
        Err(GraphError::Link {
            problem: LinkProblem::BindingMismatch,
            ..
        })
    ));
    let other_principal = AuthContext::new(TENANT, "synthetic-delegated-profile-v2").expect("ctx");
    let other = client_with(&server, other_principal, TOKEN);
    assert!(matches!(
        other.restore_delta(&checkpoint.to_json()),
        Err(GraphError::Link {
            problem: LinkProblem::BindingMismatch,
            ..
        })
    ));
    assert!(matches!(
        other
            .users_delta(&USERS_FIELDS)
            .expect("listing")
            .restore_next(&saved_next),
        Err(GraphError::Link {
            problem: LinkProblem::BindingMismatch,
            ..
        })
    ));
    // A next link is not a checkpoint, and garbage is not storage.
    assert!(matches!(
        graph.restore_delta(&saved_next),
        Err(GraphError::Link { .. })
    ));
    assert!(matches!(
        graph.restore_delta("{\"not\":\"a checkpoint\"}"),
        Err(GraphError::Link {
            problem: LinkProblem::MalformedStorage,
            ..
        })
    ));
    assert_eq!(server.requests().len(), 3);
}

#[tokio::test]
async fn a_failed_page_keeps_the_prefix_and_resumes_from_the_failed_link() {
    // The second page fails once with 503 and Retry-After, then succeeds.
    let failures = Arc::new(AtomicUsize::new(0));
    let seen = failures.clone();
    let server = TestServer::start(move |req| {
        if query_value(req, "$skiptoken").as_deref() == Some(SKIP_1)
            && seen.fetch_add(1, Ordering::SeqCst) == 0
        {
            return graph_error(
                503,
                "serviceNotAvailable",
                "The service is temporarily unavailable.",
            )
            .header("retry-after", "7");
        }
        users_delta_route(req)
    })
    .await;
    let graph = client(&server);
    let listing = graph.users_delta(&USERS_FIELDS).expect("listing");

    let incomplete = listing
        .pages()
        .collect_complete()
        .await
        .expect_err("page 2 fails");
    assert_eq!(incomplete.collected.len(), 1);
    assert_eq!(incomplete.collected[0].items.len(), 4);
    let PagesError::Provider { error, failed, .. } = &incomplete.error else {
        panic!("expected a provider failure, got {:?}", incomplete.error);
    };
    assert_eq!(
        failed.key().map(NextLink::url),
        Some(&*format!(
            "{}/v1.0/users/delta?$skiptoken={SKIP_1}",
            server.url()
        ))
    );
    assert_eq!(error.class(), ErrorClass::Service);
    let meta = error.meta().expect("response head");
    assert_eq!(meta.status().as_u16(), 503);
    assert_eq!(
        meta.headers()
            .get("retry-after")
            .map(|v| v.to_str().unwrap()),
        Some("7")
    );
    let provider = error.provider().expect("parsed error document");
    assert_eq!(provider.code, "serviceNotAvailable");
    let inner = provider.inner_error.as_ref().expect("innerError");
    assert_eq!(
        inner.additional.get("request-id").and_then(|v| v.as_str()),
        Some("0000aaaa-1111-2222-3333-444455556666")
    );
    // No automatic retry: exactly the two requests so far.
    assert_eq!(server.requests().len(), 2);

    // The application waits (not modelled) and resumes from the failed page.
    let resume = incomplete.error.resume().cloned().expect("resume point");
    let rest = listing
        .pages()
        .starting_at(resume)
        .collect_complete()
        .await
        .expect("rest of round");
    assert_eq!(rest.len(), 2);
    assert!(rest[1].meta.delta_link.is_some());
    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[2].target, requests[1].target);
}

#[tokio::test]
async fn a_throttled_page_ends_the_traversal_without_claiming_completion() {
    let server = TestServer::start(|req| {
        if query_value(req, "$skiptoken").as_deref() == Some(SKIP_1) {
            return graph_error(429, "TooManyRequests", "Too many requests.")
                .header("retry-after", "3");
        }
        users_delta_route(req)
    })
    .await;
    let graph = client(&server);
    let listing = graph.users_delta(&USERS_FIELDS).expect("listing");
    let mut pages = listing.pages();
    assert!(pages.next_page().await.expect("page 1").is_some());

    let error = pages.next_page().await.expect_err("throttled");
    let PagesError::Provider {
        error: graph_error, ..
    } = &error
    else {
        panic!("expected provider error: {error:?}");
    };
    assert_eq!(graph_error.class(), ErrorClass::Throttled);
    assert!(matches!(
        graph_error,
        GraphError::Provider {
            body: ErrorBody::Parsed(_),
            ..
        }
    ));
    assert_eq!(
        graph_error
            .meta()
            .and_then(|m| m.headers().get("retry-after"))
            .map(|v| v.to_str().unwrap()),
        Some("3")
    );
    // Diagnostics do not print the link, the token or the provider message.
    for rendered in [
        format!("{error}"),
        format!("{error:?}"),
        format!("{graph_error}"),
        format!("{graph_error:?}"),
    ] {
        assert!(!rendered.contains(SKIP_1), "{rendered}");
        assert!(!rendered.contains(TOKEN), "{rendered}");
        assert!(!rendered.contains("Too many requests."), "{rendered}");
    }

    // After an error the traversal is ended, never exhausted.
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    assert_eq!(pages.finish().outcome, Outcome::Failed);
    assert_eq!(server.requests().len(), 2);
}

#[tokio::test]
async fn a_expired_checkpoint_is_reported_and_the_application_restarts() {
    // One-page initial round, then the saved delta link is gone (410 + Location).
    let server = TestServer::start(|req| match query_value(req, "$deltatoken").as_deref() {
        Some(DELTA_1) => graph_error(410, "resyncRequired", "Resync required.").header(
            "location",
            format!("{}/v1.0/users/delta?$deltatoken=", origin(req)),
        ),
        _ if query_value(req, "$select").is_some() => ok(req, USERS_PAGE_3),
        _ => no_route(),
    })
    .await;
    let graph = client(&server);
    let listing = graph.users_delta(&USERS_FIELDS).expect("listing");
    let round = read_round(&listing).await.expect("one-page round");
    assert_eq!(round.pages, 1);

    let err = read_round(&graph.resume_users(&round.checkpoint).expect("resume"))
        .await
        .err()
        .expect("410");
    let PagesError::Provider { error, failed, .. } = &*err else {
        panic!("{err:?}")
    };
    assert_eq!(failed, &Resume::Start);
    assert_eq!(error.class(), ErrorClass::SyncStateExpired);
    let meta = error.meta().expect("head");
    assert_eq!(meta.status().as_u16(), 410);
    assert!(meta.headers().get("location").is_some());
    // The Location is not followed: one request for the round, one for the 410.
    assert_eq!(server.requests().len(), 2);

    // The application decides to restart with a full synchronization.
    let restarted = read_round(&graph.users_delta(&USERS_FIELDS).expect("listing"))
        .await
        .expect("full resync");
    assert_eq!(restarted.changes.len(), 2);
    assert_eq!(server.requests().len(), 3);
}

#[tokio::test]
async fn a_sync_state_not_found_document_is_classified_as_expired() {
    let server = TestServer::start(|req| match query_value(req, "$deltatoken").as_deref() {
        Some(DELTA_1) => graph_error(400, "syncStateNotFound", "Sync state not found."),
        _ if query_value(req, "$select").is_some() => ok(req, USERS_PAGE_3),
        _ => no_route(),
    })
    .await;
    let graph = client(&server);
    let round = read_round(&graph.users_delta(&USERS_FIELDS).expect("listing"))
        .await
        .expect("round");
    let err = read_round(&graph.resume_users(&round.checkpoint).expect("resume"))
        .await
        .err()
        .expect("expired");
    let PagesError::Provider { error, .. } = &*err else {
        panic!("{err:?}")
    };
    assert_eq!(error.class(), ErrorClass::SyncStateExpired);
    assert_eq!(error.meta().map(|m| m.status().as_u16()), Some(400));
    assert!(
        error
            .provider()
            .expect("document")
            .has_code("syncstatenotfound")
    );
    assert_eq!(server.requests().len(), 2);
}

#[tokio::test]
async fn a_page_cut_off_mid_body_fails_that_page() {
    let server = TestServer::start(|req| {
        if query_value(req, "$skiptoken").as_deref() == Some(SKIP_1) {
            let body = USERS_PAGE_2.replace("{origin}", &origin(req));
            let prefix = body[..body.len() / 2].to_owned();
            return Reply::new(200)
                .header("content-type", "application/json")
                .with_body(Body::FailAfter(vec![prefix.into()]));
        }
        users_delta_route(req)
    })
    .await;
    let graph = client(&server);
    let incomplete = graph
        .users_delta(&USERS_FIELDS)
        .expect("listing")
        .pages()
        .collect_complete()
        .await
        .expect_err("cut-off page");
    assert_eq!(incomplete.collected.len(), 1);
    let PagesError::Provider { error, failed, .. } = &incomplete.error else {
        panic!("{:?}", incomplete.error)
    };
    assert!(matches!(failed, Resume::At(_)));
    let GraphError::Concord { meta, .. } = error else {
        panic!("expected a Concord failure: {error:?}")
    };
    assert_eq!(meta.as_ref().map(|m| m.status().as_u16()), Some(200));
    assert_eq!(error.class(), ErrorClass::Local);
}

#[tokio::test]
async fn a_page_with_both_links_or_neither_violates_the_delta_contract() {
    let both = r#"{"@odata.nextLink":"{origin}/v1.0/users/delta?$skiptoken=x1","@odata.deltaLink":"{origin}/v1.0/users/delta?$deltatoken=x2","value":[{"id":"ffff7b1a-13b6-477b-8c0c-380905cd99f7"}]}"#;
    let neither = r#"{"value":[{"id":"ffff7b1a-13b6-477b-8c0c-380905cd99f7"}]}"#;
    for body in [both, neither] {
        let server = TestServer::start(move |req| ok(req, body)).await;
        let graph = client(&server);
        let mut pages = graph.users_delta(&USERS_FIELDS).expect("listing").pages();
        let err = pages.next_page().await.expect_err("contract violation");
        let PagesError::Provider { error, .. } = &err else {
            panic!("{err:?}")
        };
        assert!(
            matches!(
                error,
                GraphError::Protocol {
                    problem: ProtocolProblem::DeltaLinkCardinality,
                    ..
                }
            ),
            "{error:?}"
        );
        assert_eq!(error.class(), ErrorClass::Local);
        assert!(matches!(
            pages.next_page().await,
            Err(PagesError::Ended { .. })
        ));
        assert_eq!(server.requests().len(), 1);
    }
}

#[tokio::test]
async fn a_links_leaving_the_origin_or_collection_are_refused_before_sending() {
    // A second loopback server is a different origin (other port): it must never be called.
    let elsewhere = TestServer::start(|_| Reply::json(200, r#"{"value":[]}"#)).await;
    let foreign = elsewhere.url();
    let server = TestServer::start(move |req| match query_value(req, "$skiptoken").as_deref() {
        None => ok(
            req,
            r#"{"@odata.nextLink":"{origin}/v1.0/users/delta?$skiptoken=to-elsewhere","value":[]}"#,
        ),
        Some("to-elsewhere") => ok(
            req,
            &format!(
                r#"{{"@odata.deltaLink":"{foreign}/v1.0/users/delta?$deltatoken=elsewhere","value":[]}}"#
            ),
        ),
        _ => no_route(),
    })
    .await;
    let graph = client(&server);
    let err = graph
        .users_delta(&USERS_FIELDS)
        .expect("listing")
        .pages()
        .collect_complete()
        .await
        .expect_err("foreign delta link");
    let PagesError::Provider { error, .. } = &err.error else {
        panic!("{:?}", err.error)
    };
    let GraphError::Link { problem, meta } = error else {
        panic!("{error:?}")
    };
    assert_eq!(*problem, LinkProblem::UnsafeDestination);
    assert_eq!(meta.as_ref().map(|m| m.status().as_u16()), Some(200));
    assert_eq!(server.requests().len(), 2);
    assert!(elsewhere.requests().is_empty());

    // A same-origin next link into another collection is refused too.
    let server = TestServer::start(|req| {
        ok(
            req,
            r#"{"@odata.nextLink":"{origin}/v1.0/groups?$skiptoken=sideways","value":[]}"#,
        )
    })
    .await;
    let graph = client(&server);
    let mut pages = graph.users_delta(&USERS_FIELDS).expect("listing").pages();
    let err = pages.next_page().await.expect_err("sideways next link");
    let PagesError::Provider { error, .. } = &err else {
        panic!("{err:?}")
    };
    assert!(matches!(
        error,
        GraphError::Link {
            problem: LinkProblem::UnsafeDestination,
            ..
        }
    ));
    assert_eq!(server.requests().len(), 1);
}

// ---------------------------------------------------------------------------------------------
// Outcome B fixtures (api/group-list.md, api/group-list-members.md)
// ---------------------------------------------------------------------------------------------

const GROUP_1: &str = "11111111-2222-3333-4444-555555555555";
const GROUP_2: &str = "22222222-3333-4444-5555-666666666666";
const GROUP_3: &str = "45b7d2e7-b882-4a80-ba97-10b7a63b8fa4";
/// Invented: the group-list docs do not show a paging link.
const GROUPS_SKIP: &str = "syntheticGroupsSkipToken_0001";
/// Invented: the members docs do not show a paging link.
const MEMBERS_SKIP: &str = "syntheticMembersSkipToken_0001";

/// Shape of the documented `$select=id,displayName` response, split over two pages, with the
/// third group taken from the documented default-properties example.
const GROUPS_PAGE_1: &str = r#"{
  "@odata.context":"{origin}/v1.0/$metadata#groups(displayName,id)",
  "@odata.nextLink":"{origin}/v1.0/groups?$select=displayName,id&$skiptoken=syntheticGroupsSkipToken_0001",
  "value":[
    {"id":"11111111-2222-3333-4444-555555555555","displayName":"Contoso Group 1"},
    {"id":"22222222-3333-4444-5555-666666666666","displayName":"Contoso Group 2"}
  ]
}"#;

const GROUPS_PAGE_2: &str = r#"{
  "@odata.context":"{origin}/v1.0/$metadata#groups(displayName,id)",
  "value":[
    {"id":"45b7d2e7-b882-4a80-ba97-10b7a63b8fa4","displayName":"Golf Assist"}
  ]
}"#;

/// Group 1, page 1: a user, a nested group and a device (types assumed from the
/// directoryObject collection; ids and names reuse documented examples where possible).
const MEMBERS_1_PAGE_1: &str = r##"{
  "@odata.context":"{origin}/v1.0/$metadata#directoryObjects(displayName,id)",
  "@odata.count":6,
  "@odata.nextLink":"{origin}/v1.0/groups/11111111-2222-3333-4444-555555555555/members?$count=true&$select=displayName,id&$skiptoken=syntheticMembersSkipToken_0001",
  "value":[
    {"@odata.type":"#microsoft.graph.user","id":"ffff7b1a-13b6-477b-8c0c-380905cd99f7","displayName":"Cameron White"},
    {"@odata.type":"#microsoft.graph.group","id":"22222222-3333-4444-5555-666666666666","displayName":"Contoso Group 2"},
    {"@odata.type":"#microsoft.graph.device","id":"0b0b0b0b-1111-2222-3333-444444444444","displayName":"Synthetic Kiosk"}
  ]
}"##;

/// Group 1, page 2: a limited-information user (only type and id, other properties null, as
/// the permissions note documents), an organizational contact and an invented future type.
const MEMBERS_1_PAGE_2: &str = r##"{
  "@odata.context":"{origin}/v1.0/$metadata#directoryObjects(displayName,id)",
  "value":[
    {"@odata.type":"#microsoft.graph.user","id":"605d1257-ffff-40b6-8e6f-528a53f5dc55","displayName":null},
    {"@odata.type":"#microsoft.graph.orgContact","id":"0c0c0c0c-1111-2222-3333-444444444444","displayName":"Synthetic Vendor Contact"},
    {"@odata.type":"#microsoft.graph.futureThing","id":"0d0d0d0d-1111-2222-3333-444444444444","displayName":"Synthetic Future Object","futureProperty":{"kept":true}}
  ]
}"##;

const MEMBERS_2: &str = r##"{
  "@odata.context":"{origin}/v1.0/$metadata#directoryObjects(displayName,id)",
  "@odata.count":2,
  "value":[
    {"@odata.type":"#microsoft.graph.user","id":"605d1257-ffff-40b6-8e6f-528a53f5dc55","displayName":"Delia Dennis"},
    {"@odata.type":"#microsoft.graph.user","id":"d8c37826-ffff-4cae-b348-e2725b1e814b","displayName":"Mallory Cortez"}
  ]
}"##;

const MEMBERS_EMPTY: &str = r#"{
  "@odata.context":"{origin}/v1.0/$metadata#directoryObjects(displayName,id)",
  "@odata.count":0,
  "value":[]
}"#;

fn directory_route(req: &Recorded) -> Reply {
    let skip = query_value(req, "$skiptoken");
    match (req.path(), skip.as_deref()) {
        ("/v1.0/groups", None) => ok(req, GROUPS_PAGE_1),
        ("/v1.0/groups", Some(GROUPS_SKIP)) => ok(req, GROUPS_PAGE_2),
        (path, skip) => match (
            path.strip_prefix("/v1.0/groups/")
                .and_then(|p| p.strip_suffix("/members")),
            skip,
        ) {
            (Some(GROUP_1), None) => ok(req, MEMBERS_1_PAGE_1),
            (Some(GROUP_1), Some(MEMBERS_SKIP)) => ok(req, MEMBERS_1_PAGE_2),
            (Some(GROUP_2), None) => ok(req, MEMBERS_2),
            (Some(GROUP_3), None) => ok(req, MEMBERS_EMPTY),
            _ => no_route(),
        },
    }
}

fn sorted_pairs(req: &Recorded) -> Vec<(String, String)> {
    let mut pairs = req.query_pairs();
    pairs.sort();
    pairs
}

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Outcome B application code (adapted from the README's Workflow B example)
// ---------------------------------------------------------------------------------------------

struct GroupMembership {
    group: Group,
    members: Vec<DirectoryObject>,
    accessible_user_ids: Vec<String>,
}

async fn memberships(
    client: &GraphClient,
    active_accessible_users: &BTreeSet<String>,
) -> Result<Vec<GroupMembership>, Box<dyn std::error::Error>> {
    let mut output = Vec::new();
    let mut groups = client.groups(&[GroupField::DisplayName])?.pages().items();
    while let Some(group) = groups.next().await? {
        let members = client
            .direct_members(&group.id, &[MemberField::DisplayName])?
            .pages()
            .items()
            .collect_complete()
            .await?;
        let accessible_user_ids = members
            .iter()
            .filter(|m| m.kind() == ObjectKind::User && active_accessible_users.contains(&m.id))
            .map(|m| m.id.clone())
            .collect();
        output.push(GroupMembership {
            group,
            members,
            accessible_user_ids,
        });
    }
    Ok(output)
}

fn accessible_users() -> BTreeSet<String> {
    [
        "ffff7b1a-13b6-477b-8c0c-380905cd99f7",
        "605d1257-ffff-40b6-8e6f-528a53f5dc55",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

// ---------------------------------------------------------------------------------------------
// Outcome B tests
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn b_groups_and_their_direct_members_are_joined_to_accessible_users() {
    let server = TestServer::start(directory_route).await;
    let graph = client(&server);
    let output = memberships(&graph, &accessible_users())
        .await
        .expect("memberships");

    assert_eq!(
        output
            .iter()
            .map(|m| m.group.id.as_str())
            .collect::<Vec<_>>(),
        [GROUP_1, GROUP_2, GROUP_3]
    );
    let first = &output[0];
    assert_eq!(first.group.display_name.as_deref(), Some("Contoso Group 1"));
    assert_eq!(first.group.mail, None);
    assert_eq!(first.group.security_enabled, None);
    assert_eq!(
        first
            .members
            .iter()
            .map(DirectoryObject::kind)
            .collect::<Vec<_>>(),
        [
            ObjectKind::User,
            ObjectKind::Group,
            ObjectKind::Device,
            ObjectKind::User,
            ObjectKind::OrganizationalContact,
            ObjectKind::Other("#microsoft.graph.futureThing"),
        ]
    );
    // The nested group is returned as a member, not traversed.
    assert_eq!(first.members[1].id, GROUP_2);
    // Limited information: the member stays, its name is absent.
    assert_eq!(first.members[3].id, "605d1257-ffff-40b6-8e6f-528a53f5dc55");
    assert_eq!(first.members[3].display_name, None);
    assert_eq!(first.members[3].object_type, "#microsoft.graph.user");
    // Unknown types and properties are retained verbatim.
    assert_eq!(first.members[5].object_type, "#microsoft.graph.futureThing");
    assert_eq!(
        first.members[5].additional.get("futureProperty"),
        Some(&serde_json::json!({"kept": true}))
    );
    assert_eq!(
        first.accessible_user_ids,
        [
            "ffff7b1a-13b6-477b-8c0c-380905cd99f7",
            "605d1257-ffff-40b6-8e6f-528a53f5dc55"
        ]
    );
    assert_eq!(output[1].members.len(), 2);
    assert_eq!(
        output[1].accessible_user_ids,
        ["605d1257-ffff-40b6-8e6f-528a53f5dc55"]
    );
    assert!(output[2].members.is_empty());
    assert!(output[2].accessible_user_ids.is_empty());

    // Wire: lazy interleaving (no read-ahead of the second groups page), exact next links,
    // advanced-query parameters and header on every member request.
    let requests = server.requests();
    let targets: Vec<&str> = requests.iter().map(|r| r.path()).collect();
    assert_eq!(
        targets,
        [
            "/v1.0/groups",
            "/v1.0/groups/11111111-2222-3333-4444-555555555555/members",
            "/v1.0/groups/11111111-2222-3333-4444-555555555555/members",
            "/v1.0/groups/22222222-3333-4444-5555-666666666666/members",
            "/v1.0/groups",
            "/v1.0/groups/45b7d2e7-b882-4a80-ba97-10b7a63b8fa4/members",
        ]
    );
    for req in &requests {
        assert_get(req, TOKEN);
    }
    assert_eq!(
        requests[0].query_pairs(),
        pairs(&[("$select", "displayName,id")])
    );
    assert_eq!(requests[0].header("consistencylevel"), None);
    assert_eq!(
        requests[4].target,
        format!("/v1.0/groups?$select=displayName,id&$skiptoken={GROUPS_SKIP}")
    );
    assert_eq!(requests[4].header("consistencylevel"), None);
    for i in [1, 3, 5] {
        assert_eq!(
            sorted_pairs(&requests[i]),
            pairs(&[("$count", "true"), ("$select", "displayName,id")]),
            "request {i}"
        );
    }
    assert_eq!(
        requests[2].target,
        format!(
            "/v1.0/groups/{GROUP_1}/members?$count=true&$select=displayName,id&$skiptoken={MEMBERS_SKIP}"
        )
    );
    for i in [1, 2, 3, 5] {
        assert_eq!(
            requests[i].header("consistencylevel"),
            Some("eventual"),
            "request {i}"
        );
    }
}

#[tokio::test]
async fn b_default_selection_sends_no_advanced_query() {
    let server = TestServer::start(directory_route).await;
    let graph = client(&server);
    let groups = graph
        .groups(&[])
        .expect("groups")
        .pages()
        .items()
        .collect_complete()
        .await
        .expect("groups");
    assert_eq!(groups.len(), 3);
    let members = graph
        .direct_members(GROUP_2, &[])
        .expect("members")
        .pages()
        .items()
        .collect_complete()
        .await
        .expect("members");
    assert_eq!(members.len(), 2);
    let requests = server.requests();
    assert_eq!(requests[0].target, "/v1.0/groups");
    assert_eq!(
        requests[2].target,
        format!("/v1.0/groups/{GROUP_2}/members")
    );
    assert_eq!(requests[2].header("consistencylevel"), None);
}

#[tokio::test]
async fn b_group_id_stays_one_path_segment() {
    let server = TestServer::start(|req| ok(req, MEMBERS_EMPTY)).await;
    let graph = client(&server);
    let members = graph
        .direct_members("odd/id ?#", &[MemberField::DisplayName])
        .expect("listing")
        .pages()
        .items()
        .collect_complete()
        .await
        .expect("members");
    assert!(members.is_empty());
    assert_eq!(
        server.single().path(),
        "/v1.0/groups/odd%2Fid%20%3F%23/members"
    );
}

#[tokio::test]
async fn b_a_failing_member_listing_fails_the_aggregate_with_a_typed_error() {
    let server = TestServer::start(|req| {
        if req.path() == format!("/v1.0/groups/{GROUP_2}/members") {
            return graph_error(
                403,
                "Authorization_RequestDenied",
                "Insufficient privileges to complete the operation.",
            );
        }
        directory_route(req)
    })
    .await;
    let graph = client(&server);
    let error = memberships(&graph, &accessible_users())
        .await
        .err()
        .expect("403");
    let incomplete = error
        .downcast_ref::<Incomplete<DirectoryObject, GraphError, NextLink>>()
        .expect("the member listing's Incomplete");
    assert!(incomplete.collected.is_empty());
    let PagesError::Provider { error, failed, .. } = &incomplete.error else {
        panic!("{:?}", incomplete.error)
    };
    assert_eq!(failed, &Resume::Start);
    assert_eq!(error.class(), ErrorClass::Permission);
    assert_eq!(
        error.provider().map(|p| p.code.as_str()),
        Some("Authorization_RequestDenied")
    );
    // Group 3 was never listed: the aggregate stops at the failure.
    assert_eq!(server.requests().len(), 4);
}

#[tokio::test]
async fn b_a_missing_group_is_not_found() {
    let server = TestServer::start(|_| {
        graph_error(404, "Request_ResourceNotFound", "Resource does not exist or one of its queried reference-property objects are not present.")
    })
    .await;
    let graph = client(&server);
    let incomplete = graph
        .direct_members(
            "99999999-0000-0000-0000-000000000000",
            &[MemberField::DisplayName],
        )
        .expect("listing")
        .pages()
        .items()
        .collect_complete()
        .await
        .expect_err("404");
    let PagesError::Provider { error, .. } = &incomplete.error else {
        panic!()
    };
    assert_eq!(error.class(), ErrorClass::NotFound);
    assert!(
        error
            .provider()
            .expect("document")
            .has_code("request_resourcenotfound")
    );
}

#[tokio::test]
async fn b_member_stream_yields_the_error_before_ending() {
    let server = TestServer::start(|req| {
        if query_value(req, "$skiptoken").as_deref() == Some(MEMBERS_SKIP) {
            return graph_error(500, "generalException", "An internal error occurred.");
        }
        directory_route(req)
    })
    .await;
    let graph = client(&server);
    let mut stream = graph
        .direct_members(GROUP_1, &[MemberField::DisplayName])
        .expect("listing")
        .pages()
        .items()
        .into_stream();
    let mut ids = Vec::new();
    let mut failures = Vec::new();
    while let Some(next) = stream.next().await {
        match next {
            Ok(member) => {
                assert!(failures.is_empty(), "no item after the error");
                ids.push(member.id);
            }
            Err(error) => failures.push(error),
        }
    }
    // The stream's `None` came after exactly one error: that is not completion.
    assert_eq!(ids.len(), 3);
    assert_eq!(failures.len(), 1);
    let failure = failures.pop().expect("the stream reported the failed page");
    assert_eq!(failure.outcome(), Outcome::Failed);
    let PagesError::Provider { error, .. } = &failure else {
        panic!("{failure:?}")
    };
    assert_eq!(error.class(), ErrorClass::Service);
    assert_eq!(server.requests().len(), 2);
}

#[tokio::test]
async fn b_a_page_cap_is_bounded_and_resumable() {
    let server = TestServer::start(directory_route).await;
    let graph = client(&server);
    let listing = graph.groups(&[GroupField::DisplayName]).expect("groups");
    let incomplete = listing
        .pages()
        .max_pages(NonZeroUsize::new(1).expect("nonzero"))
        .items()
        .collect_complete()
        .await
        .expect_err("cap reached while Graph has more");
    assert_eq!(incomplete.collected.len(), 2);
    assert_eq!(incomplete.error.outcome(), Outcome::Bounded);
    let PagesError::Bounded { resume, .. } = &incomplete.error else {
        panic!("{:?}", incomplete.error)
    };
    let rest = listing
        .pages()
        .starting_at(resume.clone())
        .items()
        .collect_complete()
        .await
        .expect("rest");
    assert_eq!(
        rest.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(),
        [GROUP_3]
    );
    assert_eq!(server.requests().len(), 2);

    // finish() distinguishes exhaustion from the other endings.
    let mut pages = listing.pages();
    while pages.next_page().await.expect("page").is_some() {}
    assert_eq!(pages.finish().outcome, Outcome::Exhausted);
}

#[tokio::test]
async fn b_contract_violations_and_unusable_responses_are_typed() {
    // (body or reply, expected check). Each runs against its own server.
    type Check = fn(&GraphError) -> bool;
    type Case = (&'static str, fn(&Recorded) -> Reply, Check);
    let cases: Vec<Case> = vec![
        (
            // Observed: a missing type is a body decoding failure, not `InvalidObject`.
            "member without @odata.type",
            |req| {
                ok(
                    req,
                    r#"{"value":[{"id":"ffff7b1a-13b6-477b-8c0c-380905cd99f7","displayName":"Cameron White"}]}"#,
                )
            },
            |e| {
                matches!(e, GraphError::Concord { error, .. } if error.kind() == concord::ErrorKind::Decode)
                    && e.class() == ErrorClass::Local
            },
        ),
        (
            "member with empty @odata.type",
            |req| {
                ok(
                    req,
                    r#"{"value":[{"@odata.type":"","id":"ffff7b1a-13b6-477b-8c0c-380905cd99f7"}]}"#,
                )
            },
            |e| {
                matches!(
                    e,
                    GraphError::Protocol {
                        problem: ProtocolProblem::InvalidObject,
                        ..
                    }
                )
            },
        ),
        (
            "member with empty id",
            |req| {
                ok(
                    req,
                    r##"{"value":[{"@odata.type":"#microsoft.graph.user","id":""}]}"##,
                )
            },
            |e| {
                matches!(
                    e,
                    GraphError::Protocol {
                        problem: ProtocolProblem::InvalidObject,
                        ..
                    }
                )
            },
        ),
        (
            "members page carrying a delta link",
            |req| {
                ok(
                    req,
                    r#"{"@odata.deltaLink":"{origin}/v1.0/groups/x/members?$deltatoken=x","value":[]}"#,
                )
            },
            |e| {
                matches!(
                    e,
                    GraphError::Protocol {
                        problem: ProtocolProblem::UnexpectedDeltaLink,
                        ..
                    }
                )
            },
        ),
        (
            "next link repeating the request",
            |req| {
                let body = format!(
                    r#"{{"@odata.nextLink":"{}{}","value":[]}}"#,
                    origin(req),
                    req.target
                );
                Reply::json(200, body)
            },
            |e| {
                matches!(
                    e,
                    GraphError::Link {
                        problem: LinkProblem::DidNotAdvance,
                        ..
                    }
                )
            },
        ),
        (
            "non-JSON gateway error",
            |_| Reply::text(502, "text/html", "<html><body>Bad gateway</body></html>"),
            |e| {
                matches!(
                    e,
                    GraphError::Provider {
                        body: ErrorBody::Malformed,
                        ..
                    }
                ) && e.class() == ErrorClass::Service
            },
        ),
        (
            "redirect is not followed",
            |req| Reply::new(302).header("location", format!("{}/v1.0/groups", origin(req))),
            |e| matches!(e, GraphError::Provider { .. }) && e.class() == ErrorClass::OtherHttp,
        ),
        (
            "202 is not a collection response",
            |req| Reply {
                status: 202,
                ..ok(req, r#"{"value":[]}"#)
            },
            |e| {
                matches!(
                    e,
                    GraphError::Protocol {
                        problem: ProtocolProblem::UnexpectedStatus,
                        ..
                    }
                )
            },
        ),
    ];
    for (name, reply, check) in cases {
        let server = TestServer::start(reply).await;
        let graph = client(&server);
        let incomplete = graph
            .direct_members(GROUP_1, &[MemberField::DisplayName])
            .expect("listing")
            .pages()
            .items()
            .collect_complete()
            .await
            .expect_err(name);
        let PagesError::Provider { error, .. } = &incomplete.error else {
            panic!("{name}: {:?}", incomplete.error)
        };
        assert!(check(error), "{name}: {error:?}");
        assert!(
            error.meta().is_some(),
            "{name}: the response head is retained"
        );
        assert_eq!(server.requests().len(), 1, "{name}: nothing followed");
    }
}

#[tokio::test]
async fn b_a_member_listing_resumes_from_a_stored_continuation_with_its_header() {
    // Group 1's second member page fails once.
    let failures = Arc::new(AtomicUsize::new(0));
    let seen = failures.clone();
    let server = TestServer::start(move |req| {
        if query_value(req, "$skiptoken").as_deref() == Some(MEMBERS_SKIP)
            && seen.fetch_add(1, Ordering::SeqCst) == 0
        {
            return graph_error(
                503,
                "serviceNotAvailable",
                "The service is temporarily unavailable.",
            )
            .header("retry-after", "1");
        }
        directory_route(req)
    })
    .await;

    // Stage what was read and persist the continuation, then stop.
    let (mut staged, saved) = {
        let graph = client(&server);
        let listing = graph
            .direct_members(GROUP_1, &[MemberField::DisplayName])
            .expect("listing");
        let incomplete = listing
            .pages()
            .collect_complete()
            .await
            .expect_err("page 2 fails");
        assert_eq!(incomplete.error.outcome(), Outcome::Failed);
        let saved = match incomplete.error.resume() {
            Some(Resume::At(next)) => next.to_json(),
            other => panic!("expected a next-link continuation, got {other:?}"),
        };
        let staged: Vec<DirectoryObject> = incomplete
            .collected
            .into_iter()
            .flat_map(|page| page.items)
            .collect();
        (staged, saved)
    };
    assert_eq!(staged.len(), 3);
    assert!(!saved.contains(TOKEN));

    // Later, with a refreshed token: resume the same group's listing.
    let graph = client_with(&server, context(), REFRESHED_TOKEN);
    let listing = graph
        .direct_members(GROUP_1, &[MemberField::DisplayName])
        .expect("listing");
    let next = listing.restore_next(&saved).expect("restore continuation");
    let rest = listing
        .pages()
        .starting_at(Resume::At(next))
        .items()
        .collect_complete()
        .await
        .expect("rest of the members");
    staged.extend(rest);
    assert_eq!(staged.len(), 6);
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    let resumed = &requests[2];
    assert_get(resumed, REFRESHED_TOKEN);
    assert_eq!(resumed.target, requests[1].target);
    assert_eq!(resumed.header("consistencylevel"), Some("eventual"));

    // The continuation belongs to group 1's member listing only.
    let other_group = graph
        .direct_members(GROUP_2, &[MemberField::DisplayName])
        .expect("listing");
    assert!(matches!(
        other_group.restore_next(&saved),
        Err(GraphError::Link {
            problem: LinkProblem::BindingMismatch,
            ..
        })
    ));
    let groups = graph.groups(&[GroupField::DisplayName]).expect("listing");
    assert!(matches!(
        groups.restore_next(&saved),
        Err(GraphError::Link {
            problem: LinkProblem::BindingMismatch,
            ..
        })
    ));
    assert_eq!(server.requests().len(), 3);
}
