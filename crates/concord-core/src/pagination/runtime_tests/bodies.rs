//! Page-stable request bodies: prepared once per page from the captured
//! stable input, and never allowed to change contract to create progress.

use super::*;
use crate::body::OptionalRequestRepresentation;
use crate::codec::Text;

/// A safe search endpoint that sends an optional text body on every page.
#[derive(Clone)]
struct SearchEndpoint {
    query: Option<String>,
    cursor: String,
    /// A forged binding that illegally drops the body on continuation.
    drop_body_on_continuation: bool,
    calls: Calls,
}

impl EndpointSpec for SearchEndpoint {
    type Input = Option<String>;
    type Body = OptionalRequestRepresentation<Text>;
    type Output = TestPage;
    type Response = PageCodec;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new_with_effect(
            "pagination_runtime_api.items.search",
            Method::POST,
            Some(Duration::from_secs(30)),
            Effect::Safe,
        )
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("search")?;
        builder.push_pagination_query_value("cursor", &self.cursor)?;
        builder.finish()
    }

    fn into_body(self) -> Option<String> {
        self.query
    }
}

impl PageableEndpoint for SearchEndpoint {
    type Controller = CursorController;

    fn controller_init(&self) -> ControllerInit<Self> {
        CursorInit {
            cursor: self.cursor.clone(),
            calls: self.calls.clone(),
            fail_start: false,
            fail_advance: false,
            short_page_size: None,
            rewrite: None,
        }
    }

    fn apply_cursor(&mut self, cursor: ControllerCursor<Self>) {
        if self.drop_body_on_continuation && cursor != self.cursor {
            self.query = None;
        }
        self.cursor = cursor;
    }

    fn page_clone(&self) -> Self {
        self.calls.clones.fetch_add(1, Ordering::SeqCst);
        self.clone()
    }

    fn page_body_present(&self) -> bool {
        self.query.is_some()
    }
}

fn search(drop_body_on_continuation: bool) -> SearchEndpoint {
    SearchEndpoint {
        query: Some("stable-query-body".to_owned()),
        cursor: "c0".to_owned(),
        drop_body_on_continuation,
        calls: Calls::default(),
    }
}

#[tokio::test]
async fn page_stable_body_is_prepared_for_every_page_from_the_captured_input() {
    let harness = Harness::new();
    for (cursor, body) in [("c0", "a|c1"), ("c1", "b|-")] {
        harness
            .executor
            .application()
            .queue_expected_response(
                ExpectedRequest::new()
                    .method(Method::POST)
                    .native_url(format!("{BASE}search?cursor={cursor}"))
                    .body(Bytes::from_static(b"stable-query-body")),
                ScriptedResponse::success(Bytes::from_static(body.as_bytes())),
            )
            .expect("search page");
    }
    let endpoint = search(false);
    let calls = endpoint.calls.clone();
    let items = collect_all_request(
        Request::new(harness.core.clone(), endpoint),
        NonZeroU32::new(5).expect("nonzero"),
    )
    .await
    .expect("page-stable body traversal");
    assert_eq!(items, ["a", "b"]);
    assert_eq!(harness.requests(), 2);
    // Initial comparison copy, page 0, and the page-1 continuation.
    assert_eq!(Calls::get(&calls.clones), 3);
}

#[tokio::test]
async fn a_continuation_cannot_change_the_page_stable_body_contract() {
    let harness = Harness::new();
    harness.queue("a|c1");
    harness.queue("unused|-");
    let mut pages: Pages<String> =
        pages_request(Request::new(harness.core.clone(), search(true)), 5);
    // The body-presence contract is part of the continuation's progress
    // identity, so the page that proposed it is withheld.
    let error = pages.next_page().await.expect_err("body contract changed");
    assert_eq!(
        error.pagination_kind(),
        Some(PaginationErrorKind::ForbiddenContinuationChange)
    );
    assert_eq!(harness.requests(), 1, "the changed request is never sent");
    let summary = summary_of(&error);
    assert_eq!(summary.pages_received(), 1);
    assert_eq!(summary.pages_delivered(), 0);
    assert_eq!(summary.stop_reason(), StopReason::Failed);
    assert!(!format!("{error:?}").contains("stable-query-body"));
    assert!(pages.next_page().await.expect("returned once").is_none());
}
