use std::cell::Cell;
use std::error::Error;

use concord_core::contracts::{
    ApiError, CollectAll, CollectPrefix, ContinuationKnowledge, HeaderMap, PageDecisionLimits,
    PageResponse, PageStep, Pages, PaginationController, PaginationDescription, PaginationError,
    PaginationErrorKind, Prefix, RequestControls, StatusCode, StopReason, TraversalSummary, Url,
};

/// A custom page shape with no iterable marker.
struct CustomPage<T> {
    items: Vec<T>,
    cursor: Option<String>,
}

pub struct PrivateInit {
    pub cursor: Option<String>,
}

/// Private, non-`Clone`, non-`Sync` state behind public Init/Cursor types.
struct PrivateController {
    private_cursor: Option<String>,
    _not_sync: Cell<()>,
}

impl PaginationController<CustomPage<u64>> for PrivateController {
    type Item = u64;
    type Init = PrivateInit;
    type Cursor = Option<String>;

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

    fn start(init: PrivateInit) -> Result<Self, PaginationError> {
        Ok(Self {
            private_cursor: init.cursor,
            _not_sync: Cell::new(()),
        })
    }

    fn cursor(&self) -> Option<String> {
        self.private_cursor.clone()
    }

    fn advance(
        self,
        page: PageResponse<CustomPage<u64>>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
        let CustomPage { items, cursor } = page.into_page();
        if items.len() > limits.remaining_items() {
            return Err(PaginationError::new(
                PaginationErrorKind::CollectedItemCeilingExceeded,
            ));
        }
        Ok(match cursor {
            Some(cursor) => PageStep::Continue {
                next: Self {
                    private_cursor: Some(cursor),
                    _not_sync: Cell::new(()),
                },
                items,
            },
            None => PageStep::Done { items },
        })
    }
}

fn assert_send_static<T: Send + 'static>() {}
fn assert_send<T: Send>() {}
fn assert_controls<T: RequestControls>() {}

#[test]
fn stable_pagination_contracts_compile_without_private_imports_or_item_traits() {
    assert_send_static::<PrivateController>();
    fn assert_controller<C: PaginationController<CustomPage<u64>, Item = u64>>() {}
    assert_controller::<PrivateController>();

    assert_send::<Pages<u64>>();
    assert_send::<CollectPrefix<u64>>();
    assert_send::<CollectAll<u64>>();
    assert_send::<Prefix<u64>>();
    assert_controls::<CollectPrefix<u64>>();
    assert_controls::<CollectAll<u64>>();
    fn assert_owned<T: Clone + Send + Sync + 'static + std::fmt::Debug + Eq>() {}
    assert_owned::<TraversalSummary>();
    let stops = [
        StopReason::NotStarted,
        StopReason::Active,
        StopReason::Exhausted,
        StopReason::PageBound,
        StopReason::Application,
        StopReason::Cancelled,
        StopReason::Failed,
    ];
    for (index, stop) in stops.iter().enumerate() {
        assert!(stops[index + 1..].iter().all(|other| other != stop));
    }
    let continuation = ContinuationKnowledge::NextRequestKnown;
    let copied = continuation;
    assert_eq!(continuation, copied);
    assert_ne!(
        ContinuationKnowledge::Unknown,
        ContinuationKnowledge::Exhausted
    );
    let _: fn(&ApiError) -> Option<&TraversalSummary> = ApiError::traversal_summary;
}

#[test]
fn downstream_controllers_are_unit_testable_through_detached_page_views() {
    let current = Url::parse("https://catalog.example.test/items?cursor=a").expect("url");
    let controller =
        <PrivateController as PaginationController<CustomPage<u64>>>::start(PrivateInit {
            cursor: None,
        })
        .expect("start");
    assert_eq!(
        <PrivateController as PaginationController<CustomPage<u64>>>::cursor(&controller),
        None
    );
    let page = PageResponse::detached(
        CustomPage {
            items: vec![4_u64, 5],
            cursor: Some("b".to_owned()),
        },
        StatusCode::OK,
        &HeaderMap::new(),
        &current,
        PrivateController::DESCRIPTION,
        0,
    )
    .expect("detached page");
    assert!(
        page.current_uri().is_none(),
        "undeclared interests are dropped"
    );
    let Ok(PageStep::Continue { next, items }) =
        controller.advance(page, PageDecisionLimits::new(2))
    else {
        panic!("continuation");
    };
    assert_eq!(items, [4, 5]);
    assert_eq!(
        <PrivateController as PaginationController<CustomPage<u64>>>::cursor(&next),
        Some("b".to_owned())
    );

    let secret = PageResponse::detached(
        CustomPage::<u64> {
            items: Vec::new(),
            cursor: None,
        },
        StatusCode::OK,
        &HeaderMap::new(),
        &current,
        PaginationDescription::new(&["authorization"], false),
        0,
    );
    assert_eq!(
        secret.err().map(|error| error.kind()),
        Some(PaginationErrorKind::InvalidControllerDescription)
    );
}

#[test]
fn custom_controllers_construct_typed_safe_errors_with_recursive_redaction() {
    const SAFE_MESSAGE: &str = "reviewed pagination detail";
    const FORBIDDEN: &str = "cursor-sentinel-3f694d";
    let error = PaginationError::with_safe_message(PaginationErrorKind::Controller, SAFE_MESSAGE);
    assert_eq!(error.kind(), PaginationErrorKind::Controller);
    assert_eq!(
        error.to_string(),
        format!("pagination failed: {SAFE_MESSAGE}")
    );
    assert!(format!("{error:?}").contains(SAFE_MESSAGE));
    assert!(!error.to_string().contains(FORBIDDEN));
    assert!(!format!("{error:?}").contains(FORBIDDEN));
    let mut source = error.source();
    while let Some(error) = source {
        assert!(!error.to_string().contains(FORBIDDEN));
        assert!(!format!("{error:?}").contains(FORBIDDEN));
        source = error.source();
    }
}
