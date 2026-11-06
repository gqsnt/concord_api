//! First-use application tests for `concord-sdk-dropbox`, written by a consumer who did not
//! author the SDK, from its README/rustdoc and Concord's public docs.
//!
//! The provider is a local fake of Dropbox's `files/list_folder` RPC written for these tests from
//! the official documentation (Team Files guide, Error Handling guide, `list_folder` and
//! `list_folder/continue` references). Every ID, cursor, name and token is synthetic.
//!
//! - Outcome A: list one team member's folder in an explicit root.
//! - Outcome B: list another member/root concurrently through the same client.

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use concord::bytes::Bytes;
use concord::reqwest;
use concord_example_testserver::{Body, Generator, Recorded, Reply, TestServer};
use concord_sdk_dropbox::pagination::{PagesError, Resume};
use concord_sdk_dropbox::{
    Client, ClientConfig, Entry, Error, InputError, ListFolderOptions, LookupError, PageSize,
    PathRoot, ProviderErrorKind, RateLimitReason, RequestContext, RetryAfter, TaggedError,
};
use futures_util::StreamExt;
use serde_json::{Value, json};

const TOKEN: &str = "SYNTHETIC_FIRST_USE_TOKEN_NOT_A_CREDENTIAL";
const MEMBER_A: &str = "dbmid:SYNTHETIC_FIRST_USE_A";
const MEMBER_B: &str = "dbmid:SYNTHETIC_FIRST_USE_B";
const MEMBER_C: &str = "dbmid:SYNTHETIC_FIRST_USE_C";

// ---------------------------------------------------------------------------------------------
// A fake Dropbox, written from the documentation.
// ---------------------------------------------------------------------------------------------

type Hook = Box<dyn Fn(Reply) -> Reply + Send + Sync>;

/// One folder the fake can list: the member allowed to read it, the `Dropbox-API-Path-Root`
/// that member must send, the root-relative path, and the entries of each page.
struct Listing {
    label: &'static str,
    member: &'static str,
    path_root: Value,
    path: &'static str,
    pages: Vec<Vec<Value>>,
    /// Per page index: receives the reply the fake would send and may replace it.
    hooks: Vec<(usize, Hook)>,
}

impl Listing {
    fn new(
        label: &'static str,
        member: &'static str,
        path_root: Value,
        path: &'static str,
        pages: Vec<Vec<Value>>,
    ) -> Self {
        Self {
            label,
            member,
            path_root,
            path,
            pages,
            hooks: Vec::new(),
        }
    }

    fn hook(mut self, page: usize, hook: impl Fn(Reply) -> Reply + Send + Sync + 'static) -> Self {
        self.hooks.push((page, Box::new(hook)));
        self
    }
}

/// `files/list_folder` and `files/list_folder/continue` as documented: bearer auth, a
/// team-linked token selecting a member with `Dropbox-API-Select-User`, a JSON
/// `Dropbox-API-Path-Root`, JSON bodies, opaque cursors, `has_more`, 409 endpoint errors with
/// `error_summary`/`error`, 401 auth errors, 422 path-root errors.
struct FakeDropbox {
    listings: Vec<Listing>,
}

impl FakeDropbox {
    fn new(listings: Vec<Listing>) -> Arc<Self> {
        Arc::new(Self { listings })
    }

    async fn serve(self: &Arc<Self>) -> TestServer {
        let fake = Arc::clone(self);
        TestServer::start(move |request| fake.respond(request)).await
    }

    /// The opaque cursor that continues listing `listing` at page `next`. It deliberately holds
    /// characters that must not be re-encoded: `+ / = %` a quote and non-ASCII.
    fn cursor(&self, listing: usize, next: usize) -> String {
        format!("AAF{}+/=%2F\"é-{next}", self.listings[listing].label)
    }

    fn find_cursor(&self, cursor: &str) -> Option<(usize, usize)> {
        (0..self.listings.len()).find_map(|l| {
            (1..=self.listings[l].pages.len())
                .find(|&next| self.cursor(l, next) == cursor)
                .map(|next| (l, next))
        })
    }

    fn respond(&self, request: &Recorded) -> Reply {
        if request.method != "POST" {
            return plain(400, "Error: expected POST");
        }
        if request.header("authorization") != Some(format!("Bearer {TOKEN}").as_str()) {
            return dropbox_error(
                401,
                json!({"error_summary": "invalid_access_token/...", "error": {".tag": "invalid_access_token"}}),
            );
        }
        let Ok(body) = serde_json::from_slice::<Value>(&request.body) else {
            return plain(
                400,
                "Error in call to API function: request body: could not decode input as JSON",
            );
        };
        let member = request
            .header("dropbox-api-select-user")
            .unwrap_or_default();
        let root: Value = request
            .header("dropbox-api-path-root")
            .and_then(|text| serde_json::from_str(text).ok())
            .unwrap_or(Value::Null);
        match request.path() {
            "/2/files/list_folder" => {
                if !self.listings.iter().any(|l| l.member == member) {
                    return dropbox_error(
                        401,
                        json!({"error_summary": "invalid_select_user/..", "error": {".tag": "invalid_select_user"}}),
                    );
                }
                let path = body["path"].as_str().unwrap_or_default();
                let Some(index) = self
                    .listings
                    .iter()
                    .position(|l| l.member == member && l.path == path)
                else {
                    return dropbox_error(
                        409,
                        json!({
                            "error_summary": "path/not_found/...",
                            "error": {".tag": "path", "path": {".tag": "not_found"}},
                            "user_message": {"text": "Synthetic: that folder is missing.", "locale": "en"}
                        }),
                    );
                };
                let listing = &self.listings[index];
                if root != listing.path_root {
                    return root_mismatch(&root, &listing.path_root);
                }
                self.page(index, 0)
            }
            "/2/files/list_folder/continue" => {
                let cursor = body["cursor"].as_str().unwrap_or_default();
                let Some((index, next)) = self.find_cursor(cursor) else {
                    return dropbox_error(
                        409,
                        json!({"error_summary": "reset/..", "error": {".tag": "reset"}}),
                    );
                };
                let listing = &self.listings[index];
                if member != listing.member
                    || root != listing.path_root
                    || body.as_object().map(|o| o.len()) != Some(1)
                {
                    // Not documented behavior: a loud failure so a context mix-up cannot pass.
                    return plain(
                        400,
                        "fake: cursor continued with another member, root or extra fields",
                    );
                }
                self.page(index, next)
            }
            _ => plain(400, "Unknown API function"),
        }
    }

    fn page(&self, index: usize, page: usize) -> Reply {
        let listing = &self.listings[index];
        let entries = listing.pages.get(page).cloned().unwrap_or_default();
        let has_more = page + 1 < listing.pages.len();
        let next = (page + 1).min(listing.pages.len());
        let reply = Reply::json(
            200,
            json!({"entries": entries, "cursor": self.cursor(index, next), "has_more": has_more})
                .to_string(),
        )
        .header(
            "x-dropbox-request-id",
            format!("synthetic-{}-{page}", listing.label),
        );
        match listing.hooks.iter().find(|(at, _)| *at == page) {
            Some((_, hook)) => hook(reply),
            None => reply,
        }
    }
}

/// HTTP 422 as in the path-root header reference: a `root` mismatch reports the member's actual
/// root; an inaccessible namespace is `no_permission`.
fn root_mismatch(sent: &Value, actual: &Value) -> Reply {
    if sent[".tag"] == "root" {
        let root = actual["root"]
            .as_str()
            .or(actual["namespace_id"].as_str())
            .unwrap_or("0");
        dropbox_error(
            422,
            json!({
                "error_summary": "invalid_root/...",
                "error": {
                    ".tag": "invalid_root",
                    "invalid_root": {
                        ".tag": "team",
                        "root_namespace_id": root,
                        "home_namespace_id": "2101",
                        "home_path": "/Synthetic Member"
                    }
                }
            }),
        )
    } else {
        dropbox_error(
            422,
            json!({"error_summary": "no_permission/...", "error": {".tag": "no_permission"}}),
        )
    }
}

fn dropbox_error(status: u16, document: Value) -> Reply {
    Reply::json(status, document.to_string())
        .header("x-dropbox-request-id", format!("synthetic-error-{status}"))
}

fn plain(status: u16, text: &str) -> Reply {
    Reply::text(status, "text/plain; charset=utf-8", text)
        .header("x-dropbox-request-id", format!("synthetic-error-{status}"))
}

fn file(path_display: &str, n: u32) -> Value {
    json!({
        ".tag": "file",
        "name": path_display.rsplit('/').next().unwrap(),
        "id": format!("id:SYNTHETICFILE{n:04}"),
        "client_modified": "2026-01-02T03:04:05Z",
        "server_modified": "2026-01-02T03:04:06Z",
        "rev": format!("0000000{n:05}abc"),
        "size": 1000 + u64::from(n),
        "path_lower": path_display.to_lowercase(),
        "path_display": path_display,
        "is_downloadable": true,
        "content_hash": format!("{n:064x}"),
        "has_explicit_shared_members": false
    })
}

fn folder(path_display: &str, n: u32) -> Value {
    json!({
        ".tag": "folder",
        "name": path_display.rsplit('/').next().unwrap(),
        "id": format!("id:SYNTHETICFOLDER{n:04}"),
        "path_lower": path_display.to_lowercase(),
        "path_display": path_display,
        "sharing_info": {"read_only": false, "traverse_only": false, "no_access": false}
    })
}

fn deleted(path_display: &str) -> Value {
    json!({
        ".tag": "deleted",
        "name": path_display.rsplit('/').next().unwrap(),
        "path_lower": path_display.to_lowercase(),
        "path_display": path_display
    })
}

// ---------------------------------------------------------------------------------------------
// Application helpers.
// ---------------------------------------------------------------------------------------------

fn client(server: &TestServer) -> Client {
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    Client::new(
        http,
        ClientConfig {
            token: TOKEN.into(),
        },
    )
    .with_base_url(format!("{}/2", server.url()))
    .unwrap()
}

fn member_a() -> RequestContext {
    RequestContext::new(MEMBER_A, PathRoot::root("1001").unwrap()).unwrap()
}

fn root_1001() -> Value {
    json!({".tag": "root", "root": "1001"})
}

fn names(entries: &[Entry]) -> Vec<&str> {
    entries
        .iter()
        .map(|entry| entry.path().name.as_str())
        .collect()
}

fn body_json(request: &Recorded) -> Value {
    serde_json::from_slice(&request.body).unwrap()
}

fn path_root(request: &Recorded) -> Value {
    serde_json::from_str(request.header("dropbox-api-path-root").unwrap()).unwrap()
}

fn assert_context(request: &Recorded, member: &str, root: &Value) {
    assert_eq!(request.method, "POST");
    assert_eq!(
        request.header("authorization"),
        Some(format!("Bearer {TOKEN}").as_str())
    );
    assert_eq!(request.header("dropbox-api-select-user"), Some(member));
    assert_eq!(&path_root(request), root);
    assert_eq!(request.header("content-type"), Some("application/json"));
}

/// The typed SDK error of a failed page.
fn provider_error<K>(error: &PagesError<Error, K>) -> &Error {
    match error {
        PagesError::Provider { error, .. } => error,
        other => panic!("expected a failed page, got {other:?}"),
    }
}

fn tagged(error: &Error) -> &TaggedError {
    match &error
        .provider()
        .expect("a typed Dropbox error document")
        .kind
    {
        ProviderErrorKind::Tagged(tag) => tag,
        other => panic!("expected a tagged error, got {other:?}"),
    }
}

/// No further request arrives within a short bounded wait (no retry, no read-ahead).
async fn no_more_requests(server: &TestServer, expected: usize) {
    let more = server
        .wait_until(Duration::from_millis(150), |s| {
            s.requests().len() > expected
        })
        .await;
    assert!(!more, "unexpected extra request");
    assert_eq!(server.requests().len(), expected);
}

// ---------------------------------------------------------------------------------------------
// Outcome A: one member, one explicit root.
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_member_folder_is_read_page_by_page_until_has_more_is_false() {
    let fake = FakeDropbox::new(vec![Listing::new(
        "a",
        MEMBER_A,
        root_1001(),
        "/Projects",
        vec![
            vec![file("/Projects/Plan.txt", 1)],
            vec![], // an empty intermediate page is not the end
            vec![folder("/Projects/Designs", 2), deleted("/Projects/Old.txt")],
        ],
    )]);
    let server = fake.serve().await;
    let client = client(&server);
    let context = member_a();

    let mut pages = client.list_folder_pages(context.clone(), ListFolderOptions::new("/Projects"));
    tokio::task::yield_now().await;
    assert!(
        server.requests().is_empty(),
        "building a traversal sends nothing"
    );

    let first = pages.next_page().await.unwrap().expect("first page");
    assert_eq!(names(&first.items), ["Plan.txt"]);
    let Entry::File(plan) = &first.items[0] else {
        panic!("a file")
    };
    assert_eq!(plan.id, "id:SYNTHETICFILE0001");
    assert_eq!(plan.size, 1001);
    assert_eq!(plan.path.path_lower.as_deref(), Some("/projects/plan.txt"));
    assert_eq!(plan.server_modified, "2026-01-02T03:04:06Z");
    assert!(first.meta.has_more);
    assert_eq!(first.meta.http.status(), 200);
    assert_eq!(first.meta.http.request_id(), Some("synthetic-a-0"));
    assert_eq!(first.meta.cursor.as_str(), fake.cursor(0, 1));
    assert_eq!(first.meta.cursor.context(), &context);
    assert_eq!(
        first.next.as_ref().map(|c| c.as_str()),
        Some(fake.cursor(0, 1).as_str())
    );

    let second = pages
        .next_page()
        .await
        .unwrap()
        .expect("the empty page continues");
    assert!(second.items.is_empty());
    assert!(second.meta.has_more);
    assert_eq!(second.meta.http.request_id(), Some("synthetic-a-1"));

    let third = pages.next_page().await.unwrap().expect("last page");
    assert_eq!(names(&third.items), ["Designs", "Old.txt"]);
    assert!(matches!(third.items[0], Entry::Folder(_)));
    assert!(matches!(third.items[1], Entry::Deleted(_)));
    assert!(!third.meta.has_more);
    assert!(third.next.is_none(), "has_more: false ends the chain");
    assert_eq!(
        third.meta.cursor.as_str(),
        fake.cursor(0, 3),
        "the last cursor is kept"
    );

    assert!(pages.next_page().await.unwrap().is_none(), "exhausted");
    assert!(
        pages.next_page().await.unwrap().is_none(),
        "and stays exhausted"
    );

    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    for request in &requests {
        assert_context(request, MEMBER_A, &root_1001());
    }
    assert_eq!(requests[0].path(), "/2/files/list_folder");
    assert_eq!(requests[0].query(), None);
    assert_eq!(
        body_json(&requests[0]),
        json!({
            "path": "/Projects",
            "recursive": false,
            "include_deleted": false,
            "include_mounted_folders": true,
            "include_non_downloadable_files": true
        })
    );
    assert_eq!(requests[1].path(), "/2/files/list_folder/continue");
    assert_eq!(
        body_json(&requests[1]),
        json!({"cursor": fake.cursor(0, 1)})
    );
    assert_eq!(requests[2].path(), "/2/files/list_folder/continue");
    assert_eq!(
        body_json(&requests[2]),
        json!({"cursor": fake.cursor(0, 2)})
    );
}

#[tokio::test]
async fn a_single_page_calls_follow_has_more_with_the_returned_cursor() {
    let fake = FakeDropbox::new(vec![Listing::new(
        "a",
        MEMBER_A,
        root_1001(),
        "",
        vec![vec![folder("/Projects", 1)], vec![file("/Readme.md", 2)]],
    )]);
    let server = fake.serve().await;
    let client = client(&server);

    let first = client
        .list_folder(member_a(), ListFolderOptions::new(""))
        .await
        .unwrap();
    assert_eq!(first.meta.status(), 200);
    let start_op = first.meta.operation().expect("declared operation identity");
    assert_eq!(start_op.method(), Some("POST"));

    let mut entries = first.value.entries;
    let mut has_more = first.value.has_more;
    let mut cursor = first.value.cursor;
    let mut continue_op = None;
    while has_more {
        let next = client.list_folder_continue(&cursor).await.unwrap();
        continue_op = next.meta.operation();
        entries.extend(next.value.entries);
        has_more = next.value.has_more;
        cursor = next.value.cursor;
    }
    assert_eq!(names(&entries), ["Projects", "Readme.md"]);
    assert_eq!(cursor.context(), &member_a());
    let continue_op = continue_op.expect("operation identity on continue");
    assert_ne!(
        start_op.name(),
        continue_op.name(),
        "the two RPCs are told apart"
    );

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        body_json(&requests[0])["path"],
        "",
        "the selected root is the empty path"
    );
    assert_eq!(requests[1].path(), "/2/files/list_folder/continue");
}

#[tokio::test]
async fn a_items_and_stream_return_every_entry_in_provider_order() {
    let pages = vec![
        vec![file("/P/1.txt", 1), file("/P/2.txt", 2)],
        vec![],
        vec![deleted("/P/3.txt"), folder("/P/4", 4)],
    ];
    let fake = FakeDropbox::new(vec![Listing::new("a", MEMBER_A, root_1001(), "/P", pages)]);
    let server = fake.serve().await;
    let client = client(&server);

    let collected = client
        .list_folder_pages(member_a(), ListFolderOptions::new("/P"))
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(names(&collected), ["1.txt", "2.txt", "3.txt", "4"]);

    let streamed: Vec<_> = client
        .list_folder_pages(member_a(), ListFolderOptions::new("/P"))
        .items()
        .into_stream()
        .collect()
        .await;
    assert!(streamed.iter().all(Result::is_ok));
    let streamed: Vec<Entry> = streamed.into_iter().map(Result::unwrap).collect();
    assert_eq!(names(&streamed), ["1.txt", "2.txt", "3.txt", "4"]);
    assert_eq!(server.requests().len(), 6);
}

#[tokio::test]
async fn a_options_go_on_the_initial_request_only_with_raw_unicode_path() {
    let fake = FakeDropbox::new(vec![Listing::new(
        "a",
        MEMBER_A,
        root_1001(),
        "/Équipe/Été 100% \"final\"",
        vec![vec![file("/Équipe/Été 100% \"final\"/Ça.txt", 1)], vec![]],
    )]);
    let server = fake.serve().await;
    let client = client(&server);

    let mut options = ListFolderOptions::new("/Équipe/Été 100% \"final\"");
    options.recursive = true;
    options.include_deleted = true;
    options.include_mounted_folders = false;
    options.limit = Some(PageSize::new(2).unwrap());
    let entries = client
        .list_folder_pages(member_a(), options)
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(names(&entries), ["Ça.txt"]);

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        body_json(&requests[0]),
        json!({
            "path": "/Équipe/Été 100% \"final\"",
            "recursive": true,
            "include_deleted": true,
            "include_mounted_folders": false,
            "include_non_downloadable_files": true,
            "limit": 2
        })
    );
    assert!(
        requests[0].body_text().contains("Été 100%"),
        "raw UTF-8, not percent-encoded"
    );
    assert_eq!(
        body_json(&requests[1]),
        json!({"cursor": fake.cursor(0, 1)})
    );
    assert!(
        requests[1].body_text().contains("é-1"),
        "cursor bytes sent unchanged"
    );
}

#[tokio::test]
async fn a_provider_error_document_on_the_first_page_is_typed_and_ends_the_traversal() {
    let fake = FakeDropbox::new(vec![Listing::new(
        "a",
        MEMBER_A,
        root_1001(),
        "/Projects",
        vec![vec![]],
    )]);
    let server = fake.serve().await;
    let client = client(&server);

    let mut pages = client.list_folder_pages(member_a(), ListFolderOptions::new("/Missing"));
    let failure = pages.next_page().await.unwrap_err();
    let PagesError::Provider { failed, .. } = &failure else {
        panic!("{failure:?}")
    };
    assert!(
        matches!(failed, Resume::Start),
        "the first page is the one to retry"
    );
    let error = provider_error(&failure);
    let Error::Http(http) = error else {
        panic!("{error:?}")
    };
    assert_eq!(http.meta.status(), 409);
    assert_eq!(http.meta.request_id(), Some("synthetic-error-409"));
    assert!(matches!(
        tagged(error),
        TaggedError::Path {
            path: LookupError::NotFound
        }
    ));
    let provider = error.provider().unwrap();
    assert!(provider.summary.starts_with("path/not_found/"));
    let message = provider.user_message.as_ref().expect("user_message");
    assert_eq!(
        message.text.as_deref(),
        Some("Synthetic: that folder is missing.")
    );
    assert_eq!(message.locale.as_deref(), Some("en"));
    let raw: Value = serde_json::from_slice(http.body.bytes().expect("body kept")).unwrap();
    assert_eq!(raw["error"]["path"][".tag"], "not_found");

    // Logging the traversal error: its own text is generic; the SDK error is its `source()`.
    let source = std::error::Error::source(&failure).expect("the SDK error is the source");
    assert_eq!(source.to_string(), error.to_string());
    assert!(error.to_string().contains("409"), "{error}");

    let shown = format!("{failure} | {failure:?} | {error} | {error:?}");
    assert!(
        !shown.contains(TOKEN),
        "no credential in diagnostics: {shown}"
    );
    assert!(
        !shown.contains(MEMBER_A),
        "no member context in diagnostics: {shown}"
    );
    assert!(
        !shown.contains("that folder is missing"),
        "no error body in diagnostics: {shown}"
    );

    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    assert!(
        matches!(pages.next_page().await, Err(PagesError::Ended { .. })),
        "never Ok(None)"
    );
    no_more_requests(&server, 1).await;
}

#[tokio::test]
async fn a_reset_continuation_keeps_the_prefix_and_the_failed_cursor() {
    let fake = FakeDropbox::new(vec![
        Listing::new(
            "a",
            MEMBER_A,
            root_1001(),
            "/Projects",
            vec![
                vec![file("/Projects/One.txt", 1)],
                vec![file("/Projects/Two.txt", 2)],
            ],
        )
        .hook(1, |_| {
            dropbox_error(
                409,
                json!({"error_summary": "reset/...", "error": {".tag": "reset"}}),
            )
        }),
    ]);
    let server = fake.serve().await;
    let client = client(&server);

    let incomplete = client
        .list_folder_pages(member_a(), ListFolderOptions::new("/Projects"))
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(
        names(&incomplete.collected),
        ["One.txt"],
        "the prefix, not an inventory"
    );
    let PagesError::Provider {
        failed: Resume::At(cursor),
        ..
    } = &incomplete.error
    else {
        panic!("{:?}", incomplete.error)
    };
    assert_eq!(
        cursor.as_str(),
        fake.cursor(0, 1),
        "the continuation that failed"
    );
    assert_eq!(cursor.context(), &member_a());
    let error = provider_error(&incomplete.error);
    assert!(matches!(tagged(error), TaggedError::Reset));
    assert_eq!(error.meta().unwrap().status(), 409);
    no_more_requests(&server, 2).await;
}

#[tokio::test]
async fn a_page_body_cut_off_mid_stream_is_an_error_not_completion() {
    let fake = FakeDropbox::new(vec![
        Listing::new(
            "a",
            MEMBER_A,
            root_1001(),
            "/Projects",
            vec![
                vec![file("/Projects/One.txt", 1)],
                vec![file("/Projects/Two.txt", 2)],
            ],
        )
        .hook(1, |reply| {
            reply.with_body(Body::FailAfter(vec![Bytes::from_static(
                b"{\"entries\": [{\".tag\": \"fi",
            )]))
        }),
    ]);
    let server = fake.serve().await;
    let client = client(&server);

    let mut stream = client
        .list_folder_pages(member_a(), ListFolderOptions::new("/Projects"))
        .items()
        .into_stream();
    let first = stream.next().await.unwrap().unwrap();
    assert_eq!(first.path().name, "One.txt");
    let failure = stream.next().await.unwrap().unwrap_err();
    let PagesError::Provider {
        failed: Resume::At(cursor),
        ..
    } = &failure
    else {
        panic!("{failure:?}")
    };
    assert_eq!(cursor.as_str(), fake.cursor(0, 1));
    let Error::Concord { error, meta } = provider_error(&failure) else {
        panic!("{failure:?}")
    };
    assert_eq!(error.kind(), concord::ErrorKind::Transport, "{error}");
    let meta = meta.as_ref().expect("the head arrived");
    assert_eq!(meta.status(), 200);
    assert_eq!(meta.request_id(), Some("synthetic-a-1"));
    assert!(
        stream.next().await.is_none(),
        "the stream ends after the error"
    );
    no_more_requests(&server, 2).await;
}

#[tokio::test]
async fn a_rate_limit_is_an_http_failure_with_its_delay_and_is_not_retried() {
    let fake = FakeDropbox::new(vec![
        Listing::new("a", MEMBER_A, root_1001(), "/Structured", vec![vec![]]).hook(0, |_| {
            dropbox_error(
                429,
                json!({
                    "error_summary": "too_many_requests/...",
                    "error": {"reason": {".tag": "too_many_requests"}, "retry_after": 7}
                }),
            )
            .header("retry-after", "7")
        }),
        Listing::new("p", MEMBER_A, root_1001(), "/Plain", vec![vec![]]).hook(0, |_| {
            plain(429, "too many requests").header("retry-after", "0")
        }),
    ]);
    let server = fake.serve().await;
    let client = client(&server);

    let error = client
        .list_folder(member_a(), ListFolderOptions::new("/Structured"))
        .await
        .unwrap_err();
    let Error::Http(http) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(http.meta.status(), 429);
    assert_eq!(
        error.meta().map(|m| m.retry_after()),
        Some(RetryAfter::Seconds(Duration::from_secs(7)))
    );
    let ProviderErrorKind::RateLimited(limit) = &error.provider().unwrap().kind else {
        panic!("{error:?}")
    };
    assert_eq!(limit.reason, RateLimitReason::TooManyRequests);
    assert_eq!(limit.retry_after, 7);

    let error = client
        .list_folder(member_a(), ListFolderOptions::new("/Plain"))
        .await
        .unwrap_err();
    let Error::Http(http) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(http.meta.status(), 429);
    assert_eq!(http.meta.retry_after(), RetryAfter::Seconds(Duration::ZERO));
    assert!(
        error.provider().is_none(),
        "plaintext is not a provider document"
    );
    assert_eq!(http.body.bytes(), Some(&b"too many requests"[..]));
    no_more_requests(&server, 2).await;
}

#[tokio::test]
async fn a_non_json_error_bodies_stay_http_failures_with_the_raw_text() {
    let fake = FakeDropbox::new(vec![
        Listing::new("a", MEMBER_A, root_1001(), "/Proxy", vec![vec![]]).hook(0, |_| {
            Reply::text(
                500,
                "text/html",
                "<html><body>Synthetic upstream failure</body></html>",
            )
        }),
    ]);
    let server = fake.serve().await;
    let client = client(&server);

    // An intermediary's HTML failure page (Error Handling guide: 5xx bodies are not documents).
    let mut bad = client.list_folder_pages(member_a(), ListFolderOptions::new("/Proxy"));
    let error = bad.next_page().await.unwrap_err();
    let Error::Http(http) = provider_error(&error) else {
        panic!("{error:?}")
    };
    assert_eq!(http.meta.status(), 500);
    assert!(http.body.provider().is_none());
    assert!(http.body.read_error().is_none());
    assert_eq!(
        http.body.bytes(),
        Some(&b"<html><body>Synthetic upstream failure</body></html>"[..])
    );
    assert!(
        !format!("{error} {error:?}").contains("Synthetic upstream"),
        "body not in diagnostics"
    );
}

#[tokio::test]
async fn a_invalid_success_pages_fail_with_metadata_instead_of_completing() {
    let fake = FakeDropbox::new(vec![
        Listing::new("u", MEMBER_A, root_1001(), "/Unknown", vec![vec![]]).hook(0, |_| {
            Reply::json(200, r#"{"entries":[{".tag":"future_kind","name":"x"}],"cursor":"AAFu","has_more":false}"#)
                .header("x-dropbox-request-id", "synthetic-unknown")
        }),
        Listing::new("e", MEMBER_A, root_1001(), "/EmptyCursor", vec![vec![]]).hook(0, |_| {
            Reply::json(200, r#"{"entries":[],"cursor":"","has_more":false}"#)
                .header("x-dropbox-request-id", "synthetic-empty-cursor")
        }),
        Listing::new("m", MEMBER_A, root_1001(), "/NoHasMore", vec![vec![]]).hook(0, |_| {
            Reply::json(200, r#"{"entries":[],"cursor":"AAFm"}"#)
                .header("x-dropbox-request-id", "synthetic-no-has-more")
        }),
        Listing::new("t", MEMBER_A, root_1001(), "/Html", vec![vec![]])
            .hook(0, |_| Reply::text(200, "text/html", "<html></html>")),
    ]);
    let server = fake.serve().await;
    let client = client(&server);

    for (path, request_id) in [
        ("/Unknown", Some("synthetic-unknown")),
        ("/EmptyCursor", Some("synthetic-empty-cursor")),
        ("/NoHasMore", Some("synthetic-no-has-more")),
        ("/Html", None),
    ] {
        let mut pages = client.list_folder_pages(member_a(), ListFolderOptions::new(path));
        let failure = pages.next_page().await.unwrap_err();
        let error = provider_error(&failure);
        let meta = error
            .meta()
            .unwrap_or_else(|| panic!("{path}: metadata survives: {error:?}"));
        assert_eq!(meta.status(), 200, "{path}");
        assert_eq!(meta.request_id(), request_id, "{path}");
        match (path, error) {
            ("/EmptyCursor", Error::EmptyCursor { .. }) => {}
            ("/Unknown" | "/NoHasMore", Error::Concord { error, .. }) => {
                assert_eq!(error.kind(), concord::ErrorKind::Decode, "{path}: {error}");
            }
            ("/Html", Error::Concord { error, .. }) => {
                assert_eq!(
                    error.kind(),
                    concord::ErrorKind::Metadata,
                    "{path}: {error}"
                );
            }
            (path, other) => panic!("{path}: unexpected {other:?}"),
        }
        assert!(
            matches!(pages.next_page().await, Err(PagesError::Ended { .. })),
            "{path}"
        );
    }
}

#[tokio::test]
async fn a_cap_reached_while_more_pages_exist_is_bounded_not_complete() {
    let fake = FakeDropbox::new(vec![Listing::new(
        "a",
        MEMBER_A,
        root_1001(),
        "/Projects",
        vec![
            vec![file("/Projects/1", 1)],
            vec![file("/Projects/2", 2)],
            vec![file("/Projects/3", 3)],
        ],
    )]);
    let server = fake.serve().await;
    let client = client(&server);

    let mut pages = client
        .list_folder_pages(member_a(), ListFolderOptions::new("/Projects"))
        .max_pages(NonZeroUsize::new(2).unwrap());
    assert!(pages.next_page().await.unwrap().is_some());
    assert!(pages.next_page().await.unwrap().is_some());
    let bounded = pages.next_page().await.unwrap_err();
    let PagesError::Bounded {
        resume: Resume::At(cursor),
        ..
    } = &bounded
    else {
        panic!("{bounded:?}")
    };
    assert_eq!(
        cursor.as_str(),
        fake.cursor(0, 2),
        "where a later traversal would continue"
    );
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    no_more_requests(&server, 2).await;
}

#[tokio::test]
async fn a_default_cap_stops_a_listing_that_never_ends_after_1000_requests() {
    let served = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&served);
    let server = TestServer::start(move |_| {
        let n = counter.fetch_add(1, Ordering::SeqCst);
        Reply::json(
            200,
            json!({"entries": [], "cursor": format!("AAFloop{n}"), "has_more": true}).to_string(),
        )
    })
    .await;
    let client = client(&server);

    let incomplete = client
        .list_folder_pages(member_a(), ListFolderOptions::new(""))
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert!(incomplete.collected.is_empty());
    assert!(
        matches!(incomplete.error, PagesError::Bounded { .. }),
        "{:?}",
        incomplete.error
    );
    assert_eq!(incomplete.error.progress().admitted, 1000);
    no_more_requests(&server, 1000).await;
}

#[tokio::test]
async fn a_invalid_local_inputs_fail_before_any_request() {
    assert_eq!(PageSize::new(0).unwrap_err(), InputError::PageSize);
    assert_eq!(PageSize::new(2001).unwrap_err(), InputError::PageSize);
    assert_eq!(PageSize::new(2000).unwrap().get(), 2000);
    assert_eq!(
        PathRoot::root("10 01").unwrap_err(),
        InputError::NamespaceId
    );
    assert_eq!(
        PathRoot::namespace("").unwrap_err(),
        InputError::NamespaceId
    );
    assert!(
        PathRoot::namespace("ns:Team_B-2").is_ok(),
        "opaque IDs are accepted as text"
    );
    let error = RequestContext::new("dbmid:has space", PathRoot::Home).unwrap_err();
    assert_eq!(error, InputError::MemberId);
    assert!(
        !error.to_string().contains("has space"),
        "rejected value not echoed"
    );
    assert_eq!(
        RequestContext::new("", PathRoot::Home).unwrap_err(),
        InputError::MemberId
    );

    let http = reqwest::Client::new();
    let client = Client::new(
        http,
        ClientConfig {
            token: TOKEN.into(),
        },
    );
    assert!(client.with_base_url("not a url").is_err());
}

// ---------------------------------------------------------------------------------------------
// Outcome B: another member/root, concurrently, through the same client.
// ---------------------------------------------------------------------------------------------

fn namespace_2002() -> Value {
    json!({".tag": "namespace_id", "namespace_id": "2002"})
}

fn member_b() -> RequestContext {
    RequestContext::new(MEMBER_B, PathRoot::namespace("2002").unwrap()).unwrap()
}

/// The requests one member made, with their arrival index.
fn requests_of<'a>(requests: &'a [Recorded], member: &str) -> Vec<(usize, &'a Recorded)> {
    requests
        .iter()
        .enumerate()
        .filter(|(_, request)| request.header("dropbox-api-select-user") == Some(member))
        .collect()
}

/// Holds `reply`'s body back, sending JSON-insignificant spaces every 5 ms, until `open` is set
/// (bounded at about 10 s, which sets `timed_out`): the page cannot complete before then.
fn gated(mut reply: Reply, open: Arc<AtomicBool>, timed_out: Arc<AtomicBool>) -> Reply {
    let Body::Full(bytes) = std::mem::replace(&mut reply.body, Body::Stall) else {
        panic!("the fake's pages are complete bodies")
    };
    let mut waited = 0_u32;
    let mut done = false;
    let generator: Generator = Box::new(move || {
        if done {
            return None;
        }
        if open.load(Ordering::SeqCst) || waited >= 2000 {
            if waited >= 2000 {
                timed_out.store(true, Ordering::SeqCst);
            }
            done = true;
            return Some(bytes.clone());
        }
        waited += 1;
        Some(Bytes::from_static(b" "))
    });
    reply.with_body(Body::Generate(generator, Some(Duration::from_millis(5))))
}

#[tokio::test]
async fn b_two_members_list_concurrently_through_one_client() {
    let b_continued = Arc::new(AtomicBool::new(false));
    let timed_out = Arc::new(AtomicBool::new(false));
    let (open, late, seen) = (
        Arc::clone(&b_continued),
        Arc::clone(&timed_out),
        Arc::clone(&b_continued),
    );
    let fake = FakeDropbox::new(vec![
        Listing::new(
            "a",
            MEMBER_A,
            root_1001(),
            "/Projects",
            vec![
                vec![file("/Projects/A-1.txt", 1)],
                vec![file("/Projects/A-2.txt", 2)],
            ],
        )
        .hook(0, move |reply| {
            gated(reply, Arc::clone(&open), Arc::clone(&late))
        }),
        Listing::new(
            "b",
            MEMBER_B,
            namespace_2002(),
            "/Projects",
            vec![
                vec![folder("/Projects/B-1", 3)],
                vec![file("/Projects/B-2.txt", 4)],
            ],
        )
        .hook(1, move |reply| {
            seen.store(true, Ordering::SeqCst);
            reply
        }),
    ]);
    let server = fake.serve().await;
    let client = client(&server);

    // The README's pattern: one local selection, reassigned after the first traversal started.
    let mut selected = RequestContext::new(MEMBER_A, PathRoot::root("1001").unwrap()).unwrap();
    let first = client.list_folder_pages(selected.clone(), ListFolderOptions::new("/Projects"));
    selected = RequestContext::new(MEMBER_B, PathRoot::namespace("2002").unwrap()).unwrap();
    let second = client.list_folder_pages(selected, ListFolderOptions::new("/Projects"));

    let (a, b) = tokio::join!(
        first.items().collect_complete(),
        second.items().collect_complete()
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(names(&a), ["A-1.txt", "A-2.txt"]);
    assert_eq!(names(&b), ["B-1", "B-2.txt"]);
    assert!(
        !timed_out.load(Ordering::SeqCst),
        "B progressed while A's first page was arriving"
    );

    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    let a_requests = requests_of(&requests, MEMBER_A);
    let b_requests = requests_of(&requests, MEMBER_B);
    assert_eq!(a_requests.len(), 2);
    assert_eq!(b_requests.len(), 2);
    for (_, request) in &a_requests {
        assert_context(request, MEMBER_A, &root_1001());
        assert_eq!(
            request.header("dropbox-api-path-root"),
            Some(r#"{".tag":"root","root":"1001"}"#)
        );
    }
    for (_, request) in &b_requests {
        assert_context(request, MEMBER_B, &namespace_2002());
        assert_eq!(
            request.header("dropbox-api-path-root"),
            Some(r#"{".tag":"namespace_id","namespace_id":"2002"}"#)
        );
    }
    assert_eq!(a_requests[1].1.path(), "/2/files/list_folder/continue");
    assert_eq!(
        body_json(a_requests[1].1),
        json!({"cursor": fake.cursor(0, 1)})
    );
    assert_eq!(b_requests[1].1.path(), "/2/files/list_folder/continue");
    assert_eq!(
        body_json(b_requests[1].1),
        json!({"cursor": fake.cursor(1, 1)})
    );
    assert!(
        b_requests[1].0 < a_requests[1].0,
        "B continued before A could"
    );
}

#[tokio::test]
async fn b_one_member_failing_does_not_disturb_the_other() {
    let fake = FakeDropbox::new(vec![
        Listing::new(
            "a",
            MEMBER_A,
            root_1001(),
            "/Projects",
            vec![
                vec![file("/Projects/A-1.txt", 1)],
                vec![file("/Projects/A-2.txt", 2)],
            ],
        ),
        Listing::new("b", MEMBER_B, namespace_2002(), "/Projects", vec![vec![]]),
    ]);
    let server = fake.serve().await;
    let client = client(&server);

    // Member B selects a namespace it cannot access.
    let b_context = RequestContext::new(MEMBER_B, PathRoot::namespace("3003").unwrap()).unwrap();
    let (a, b) = tokio::join!(
        client
            .list_folder_pages(member_a(), ListFolderOptions::new("/Projects"))
            .items()
            .collect_complete(),
        client
            .list_folder_pages(b_context, ListFolderOptions::new("/Projects"))
            .items()
            .collect_complete(),
    );
    assert_eq!(names(&a.unwrap()), ["A-1.txt", "A-2.txt"]);
    let incomplete = b.unwrap_err();
    assert!(incomplete.collected.is_empty());
    assert!(matches!(
        incomplete.error,
        PagesError::Provider {
            failed: Resume::Start,
            ..
        }
    ));
    let error = provider_error(&incomplete.error);
    assert_eq!(error.meta().unwrap().status(), 422);
    assert!(matches!(tagged(error), TaggedError::NoPermission));
    assert!(
        error
            .provider()
            .unwrap()
            .summary
            .starts_with("no_permission/")
    );

    no_more_requests(&server, 3).await;
    let requests = server.requests();
    assert_eq!(requests_of(&requests, MEMBER_A).len(), 2);
    let b_requests = requests_of(&requests, MEMBER_B);
    assert_eq!(b_requests.len(), 1, "no retry, no fallback root");
    assert_eq!(
        path_root(b_requests[0].1),
        json!({".tag": "namespace_id", "namespace_id": "3003"})
    );
}

#[tokio::test]
async fn b_a_stale_root_is_reported_with_the_actual_root_and_never_switched() {
    let fake = FakeDropbox::new(vec![Listing::new(
        "b",
        MEMBER_B,
        json!({".tag": "root", "root": "2002"}),
        "",
        vec![vec![folder("/Team Shared", 5)]],
    )]);
    let server = fake.serve().await;
    let client = client(&server);

    let stale = RequestContext::new(MEMBER_B, PathRoot::root("1001").unwrap()).unwrap();
    let mut pages = client.list_folder_pages(stale, ListFolderOptions::new(""));
    let failure = pages.next_page().await.unwrap_err();
    let error = provider_error(&failure);
    assert_eq!(error.meta().unwrap().status(), 422);
    let TaggedError::InvalidRoot { invalid_root } = tagged(error) else {
        panic!("{error:?}")
    };
    assert_eq!(invalid_root.kind, "team");
    assert_eq!(invalid_root.root_namespace_id, "2002");
    assert_eq!(invalid_root.home_namespace_id.as_deref(), Some("2101"));
    assert_eq!(invalid_root.home_path.as_deref(), Some("/Synthetic Member"));
    let actual_root = invalid_root.root_namespace_id.clone();
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    no_more_requests(&server, 1).await;

    // The application, not the SDK, decides to select the reported root.
    let fresh = RequestContext::new(MEMBER_B, PathRoot::root(actual_root).unwrap()).unwrap();
    let entries = client
        .list_folder_pages(fresh, ListFolderOptions::new(""))
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(names(&entries), ["Team Shared"]);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(path_root(&requests[0]), root_1001());
    assert_eq!(
        path_root(&requests[1]),
        json!({".tag": "root", "root": "2002"})
    );
}

#[tokio::test]
async fn b_an_unknown_member_is_invalid_select_user_while_the_other_completes() {
    let fake = FakeDropbox::new(vec![Listing::new(
        "a",
        MEMBER_A,
        root_1001(),
        "/Projects",
        vec![vec![file("/Projects/A-1.txt", 1)]],
    )]);
    let server = fake.serve().await;
    let client = client(&server);

    let unknown = RequestContext::new(MEMBER_C, PathRoot::Home).unwrap();
    let (a, c) = tokio::join!(
        client
            .list_folder_pages(member_a(), ListFolderOptions::new("/Projects"))
            .items()
            .collect_complete(),
        client
            .list_folder_pages(unknown, ListFolderOptions::new("/Projects"))
            .items()
            .collect_complete(),
    );
    assert_eq!(names(&a.unwrap()), ["A-1.txt"]);
    let incomplete = c.unwrap_err();
    let error = provider_error(&incomplete.error);
    assert_eq!(error.meta().unwrap().status(), 401);
    assert!(matches!(tagged(error), TaggedError::InvalidSelectUser));

    let requests = server.requests();
    let c_requests = requests_of(&requests, MEMBER_C);
    assert_eq!(c_requests.len(), 1);
    assert_eq!(
        c_requests[0].1.header("dropbox-api-path-root"),
        Some(r#"{".tag":"home"}"#)
    );
}

#[tokio::test]
async fn b_dropping_one_members_traversal_stops_only_that_member() {
    let fake = FakeDropbox::new(vec![
        Listing::new(
            "a",
            MEMBER_A,
            root_1001(),
            "/Projects",
            vec![
                vec![file("/Projects/A-1.txt", 1)],
                vec![file("/Projects/A-2.txt", 2)],
            ],
        ),
        Listing::new(
            "b",
            MEMBER_B,
            namespace_2002(),
            "/Projects",
            vec![vec![file("/Projects/B-1.txt", 3)], vec![]],
        )
        .hook(0, |reply| reply.with_body(Body::Stall)),
    ]);
    let server = fake.serve().await;
    let client = client(&server);

    let a_pages = client.list_folder_pages(member_a(), ListFolderOptions::new("/Projects"));
    let mut b_pages = client.list_folder_pages(member_b(), ListFolderOptions::new("/Projects"));
    let (a, b_was_pending) = tokio::join!(a_pages.items().collect_complete(), async {
        tokio::time::timeout(Duration::from_millis(300), b_pages.next_page())
            .await
            .is_err()
    });
    assert!(b_was_pending, "B's first page body never arrived");
    drop(b_pages); // the application gives up on B: this is not completion

    assert_eq!(names(&a.unwrap()), ["A-1.txt", "A-2.txt"]);
    no_more_requests(&server, 3).await;
    let requests = server.requests();
    assert_eq!(requests_of(&requests, MEMBER_A).len(), 2);
    assert_eq!(
        requests_of(&requests, MEMBER_B).len(),
        1,
        "nothing continues after the drop"
    );
}
