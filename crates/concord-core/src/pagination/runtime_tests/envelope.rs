//! V3-P01: the §17.2 downstream cursor-envelope controller, unchanged except
//! for import paths, paging a JSON-like buffered envelope that does not
//! implement `IntoIterator`.

use serde::Deserialize;
use serde::de::DeserializeOwned;

use super::*;
use crate::pagination::PaginationErrorKind as Kind;

// ---- Canonical §17.2 source shape (only the `use` path differs) ----------

#[derive(Deserialize)]
pub struct Envelope<T> {
    pub data: Vec<T>,
    pub next: Option<String>,
}

pub struct CursorStart {
    pub after: Option<String>,
}

pub struct EnvelopePages {
    after: Option<String>,
}

impl<T: Send + 'static> PaginationController<Envelope<T>> for EnvelopePages {
    type Item = T;
    type Init = CursorStart;
    type Cursor = Option<String>;
    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

    fn start(init: Self::Init) -> Result<Self, PaginationError> {
        Ok(Self { after: init.after })
    }

    fn cursor(&self) -> Self::Cursor {
        self.after.clone()
    }

    fn advance(
        self,
        response: PageResponse<Envelope<T>>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, T>, PaginationError> {
        let Envelope { data, next } = response.into_page();
        if data.len() > limits.remaining_items() {
            return Err(PaginationError::new(Kind::CollectedItemCeilingExceeded));
        }
        match next {
            None => Ok(PageStep::Done { items: data }),
            Some(cursor) => {
                if cursor.is_empty() || self.after.as_ref() == Some(&cursor) {
                    return Err(PaginationError::new(Kind::InvalidContinuation));
                }
                Ok(PageStep::Continue {
                    next: Self {
                        after: Some(cursor),
                    },
                    items: data,
                })
            }
        }
    }
}

// ---- A JSON-like buffered representation without any iterable marker ------

struct JsonLike;

impl<T> ResponseRepresentation<T> for JsonLike
where
    T: DeserializeOwned + Send + 'static,
{
    type Driver = Buffered;

    fn from_body(body: Bytes, _head: &ResponseHead<'_>) -> Result<T, ResponseBodyError> {
        serde_json::from_slice(&body).map_err(|error| {
            drop(error);
            ResponseBodyError::new(ResponseBodyErrorKind::Decode)
        })
    }
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
struct ItemSummary {
    id: String,
}

/// `GET feed(query after?: String, query count: NonZeroU32 = DEFAULT_PAGE_SIZE)
/// -> Envelope<ItemSummary>` with `paginate via EnvelopePages { init
/// CursorStart { after: after }; cursor -> after; }`, as the macro would bind it.
#[derive(Clone)]
struct FeedEndpoint {
    after: Option<String>,
    count: NonZeroU32,
}

impl EndpointSpec for FeedEndpoint {
    type Input = ();
    type Body = EmptyBody;
    type Output = Envelope<ItemSummary>;
    type Response = JsonLike;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new(
            "catalog.regional.feed",
            Method::GET,
            Some(Duration::from_secs(30)),
        )
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("feed")?;
        if let Some(after) = &self.after {
            builder.push_pagination_query_value("after", after)?;
        }
        builder.push_query_value("count", &self.count.get())?;
        builder.finish()
    }

    fn into_body(self) {}
}

impl PageableEndpoint for FeedEndpoint {
    type Controller = EnvelopePages;

    fn controller_init(&self) -> ControllerInit<Self> {
        CursorStart {
            after: self.after.clone(),
        }
    }

    fn apply_cursor(&mut self, cursor: ControllerCursor<Self>) {
        self.after = cursor;
    }

    fn page_clone(&self) -> Self {
        self.clone()
    }

    fn page_body_present(&self) -> bool {
        false
    }
}

fn feed(after: Option<&str>) -> FeedEndpoint {
    FeedEndpoint {
        after: after.map(str::to_owned),
        count: NonZeroU32::new(2).expect("nonzero"),
    }
}

fn queue_json(harness: &Harness, url: &str, body: &'static str) {
    harness
        .executor
        .application()
        .queue_expected_response(
            ExpectedRequest::new().native_url(url.to_owned()),
            ScriptedResponse::success(Bytes::from_static(body.as_bytes())),
        )
        .expect("queued envelope page");
}

#[tokio::test]
async fn downstream_envelope_controller_pages_a_buffered_json_envelope() {
    let harness = Harness::new();
    queue_json(
        &harness,
        &format!("{BASE}feed?count=2"),
        r#"{"data":[{"id":"a"},{"id":"b"}],"next":"c-1"}"#,
    );
    queue_json(
        &harness,
        &format!("{BASE}feed?after=c-1&count=2"),
        r#"{"data":[{"id":"c"}],"next":null}"#,
    );
    let items = collect_all_request(
        Request::new(harness.core.clone(), feed(None)),
        NonZeroU32::new(10).expect("nonzero"),
    )
    .await
    .expect("complete envelope traversal");
    assert_eq!(
        items
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        ["a", "b", "c"]
    );
    assert_eq!(harness.requests(), 2);

    // The same endpoint through the lazy terminal.
    let lazy = Harness::new();
    queue_json(
        &lazy,
        &format!("{BASE}feed?after=start&count=2"),
        r#"{"data":[{"id":"x"}],"next":"y-2"}"#,
    );
    let mut pages = pages_request(Request::new(lazy.core.clone(), feed(Some("start"))), 1);
    let page = pages
        .next_page()
        .await
        .expect("first envelope page")
        .expect("page delivered");
    assert_eq!(page.index(), 0);
    assert_eq!(page.metadata().page_index(), 0);
    assert_eq!(page.items(), [ItemSummary { id: "x".to_owned() }]);
    assert!(pages.next_page().await.expect("bounded").is_none());
    let summary = pages.finish();
    assert_eq!(summary.stop_reason(), StopReason::PageBound);
    assert_eq!(
        summary.continuation(),
        ContinuationKnowledge::NextRequestKnown
    );
    assert_eq!(lazy.requests(), 1);
}

#[tokio::test]
async fn envelope_domain_rejection_of_a_repeated_cursor_withholds_the_page() {
    let harness = Harness::new();
    queue_json(
        &harness,
        &format!("{BASE}feed?after=same&count=2"),
        r#"{"data":[{"id":"withheld"}],"next":"same"}"#,
    );
    let error = collect_prefix_request(Request::new(harness.core.clone(), feed(Some("same"))), 5)
        .await
        .expect_err("repeated provider cursor");
    assert_eq!(error.pagination_kind(), Some(Kind::InvalidContinuation));
    let summary = summary_of(&error);
    assert_eq!(summary.pages_received(), 1);
    assert_eq!(summary.pages_delivered(), 0);
    assert!(!format!("{error:?}").contains("withheld"));
}
