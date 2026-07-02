use crate::error::{ApiClientError, ErrorContext};
use crate::pagination::{
    EndpointField, EndpointPagination, EndpointPaginationController, PageAdvance, PageApply,
    PageApplyResult, PageDecision, PageItems, ProgressKey,
};
use std::num::NonZeroUsize;

/// Output helper trait for cursor pagination.
/// The output type must both:
/// - expose items (PageItems)
/// - expose a "next cursor" value (HasNextCursor)
pub trait HasNextCursor {
    type Cursor: Clone + Eq + std::hash::Hash + ToString + Send + 'static;
    fn next_cursor(&self) -> Option<Self::Cursor>;
}

impl<T: Send + 'static> HasNextCursor for Vec<T> {
    type Cursor = String;

    fn next_cursor(&self) -> Option<Self::Cursor> {
        None
    }
}

/// Cursor pagination:
/// - request: cursor + per_page
/// - response: provides a "next cursor"
#[derive(Clone, Debug)]
pub struct CursorPagination<C = String> {
    /// Initial cursor (usually None).
    pub cursor: Option<C>,
    /// Page size (must be > 0).
    pub per_page: u64,

    /// If false, first request omits the cursor param when `cursor` is None.
    pub send_cursor_on_first: bool,

    /// If true, stop when response has no cursor (None) after collecting that page.
    pub stop_when_cursor_missing: bool,
}

impl Default for CursorPagination<String> {
    fn default() -> Self {
        Self {
            cursor: None,
            per_page: 20,
            send_cursor_on_first: false,
            stop_when_cursor_missing: true,
        }
    }
}

impl<Page> EndpointPagination<Page> for CursorPagination<String>
where
    Page: PageItems + HasNextCursor<Cursor = String>,
{
    fn apply(&mut self, ctx: PageApply<'_>) -> Result<PageApplyResult, ApiClientError> {
        Ok(PageApplyResult {
            expected_items_per_page: Some(validate_per_page(self.per_page, "cursor", ctx.ctx)?),
        })
    }

    fn advance(
        &mut self,
        page: &Page,
        _ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError> {
        let _ = validate_per_page(
            self.per_page,
            "cursor",
            &ErrorContext {
                endpoint: "pagination",
                method: http::Method::GET,
            },
        )?;
        self.cursor = page.next_cursor();
        if self.cursor.is_none() && self.stop_when_cursor_missing {
            return Ok(PageDecision::Stop);
        }
        Ok(PageDecision::Continue)
    }

    fn progress_key(&self) -> Option<ProgressKey> {
        self.cursor
            .as_ref()
            .map(|cursor| ProgressKey::Str(cursor.to_string()))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CursorBindings<E, C> {
    pub cursor: EndpointField<E, Option<C>>,
    pub per_page: EndpointField<E, u64>,
}

#[derive(Clone, Debug)]
pub struct CursorState<C> {
    pub cursor: Option<C>,
    pub per_page: u64,
    pub started: bool,
}

impl<E: 'static, Page> EndpointPaginationController<E, Page> for CursorPagination<String>
where
    Page: PageItems + HasNextCursor,
{
    type Bindings = CursorBindings<E, Page::Cursor>;
    type State = CursorState<Page::Cursor>;

    fn init(
        &self,
        bindings: &Self::Bindings,
        endpoint: &E,
        ctx: PageApply<'_>,
    ) -> Result<Self::State, ApiClientError> {
        let cursor = bindings.cursor.get(endpoint);
        let per_page = bindings.per_page.get(endpoint);
        validate_per_page(per_page, "cursor", ctx.ctx)?;
        Ok(CursorState {
            cursor,
            per_page,
            started: false,
        })
    }

    fn apply(
        &self,
        bindings: &Self::Bindings,
        state: &mut Self::State,
        endpoint: &mut E,
        ctx: PageApply<'_>,
    ) -> Result<PageApplyResult, ApiClientError> {
        bindings.per_page.set(endpoint, state.per_page);
        let should_send_cursor = state.started || self.send_cursor_on_first;
        let cursor = if should_send_cursor {
            state.cursor.clone()
        } else {
            None
        };
        bindings.cursor.set(endpoint, cursor);
        Ok(PageApplyResult {
            expected_items_per_page: Some(validate_per_page(state.per_page, "cursor", ctx.ctx)?),
        })
    }

    fn advance(
        &self,
        _bindings: &Self::Bindings,
        state: &mut Self::State,
        page: &Page,
        _page_ctx: PageAdvance<'_>,
    ) -> Result<PageDecision, ApiClientError> {
        state.started = true;
        state.cursor = page.next_cursor();
        if state.cursor.is_none() && self.stop_when_cursor_missing {
            return Ok(PageDecision::Stop);
        }
        Ok(PageDecision::Continue)
    }

    fn progress_key(&self, state: &Self::State) -> Option<ProgressKey> {
        state
            .cursor
            .as_ref()
            .map(|cursor| ProgressKey::Str(cursor.to_string()))
    }
}

fn validate_per_page(
    value: u64,
    controller: &'static str,
    ctx: &ErrorContext,
) -> Result<NonZeroUsize, ApiClientError> {
    let value = usize::try_from(value).map_err(|_| ApiClientError::Pagination {
        ctx: ctx.clone(),
        msg: format!("{controller}: per_page does not fit in usize").into(),
    })?;
    NonZeroUsize::new(value).ok_or_else(|| ApiClientError::Pagination {
        ctx: ctx.clone(),
        msg: format!("{controller}: per_page must be greater than zero").into(),
    })
}

#[cfg(test)]
mod controller_tests {
    use super::*;
    use http::Method;

    #[derive(Clone, Debug)]
    struct TestPage {
        items: Vec<String>,
        next: Option<String>,
    }

    impl PageItems for TestPage {
        type Item = String;

        fn item_count_hint(&self) -> Option<usize> {
            Some(self.items.len())
        }

        fn into_items(self) -> Vec<Self::Item> {
            self.items
        }
    }

    impl HasNextCursor for TestPage {
        type Cursor = String;

        fn next_cursor(&self) -> Option<Self::Cursor> {
            self.next.clone()
        }
    }

    #[test]
    fn cursor_string_single_object_pagination_advances_cursor() {
        let ctx = ErrorContext {
            endpoint: "Items",
            method: Method::GET,
        };
        let mut pagination = CursorPagination {
            cursor: Some("start".to_string()),
            per_page: 2,
            send_cursor_on_first: false,
            stop_when_cursor_missing: true,
        };

        let applied = <CursorPagination as EndpointPagination<TestPage>>::apply(
            &mut pagination,
            PageApply {
                endpoint: "List",
                page_index: 0,
                ctx: &ctx,
            },
        )
        .expect("cursor apply");
        assert_eq!(applied.expected_items_per_page, NonZeroUsize::new(2));
        assert_eq!(
            <CursorPagination as EndpointPagination<TestPage>>::progress_key(&pagination),
            Some(ProgressKey::Str("start".to_string()))
        );

        let decision = <CursorPagination as EndpointPagination<TestPage>>::advance(
            &mut pagination,
            &TestPage {
                items: vec!["a".to_string()],
                next: Some("next-1".to_string()),
            },
            PageAdvance {
                endpoint: "List",
                page_index: 0,
                received_items: 1,
            },
        )
        .expect("cursor advance");
        assert_eq!(decision, PageDecision::Continue);
        assert_eq!(
            <CursorPagination as EndpointPagination<TestPage>>::progress_key(&pagination),
            Some(ProgressKey::Str("next-1".to_string()))
        );

        let mut zero_per_page = CursorPagination {
            cursor: None,
            per_page: 0,
            send_cursor_on_first: false,
            stop_when_cursor_missing: true,
        };
        assert!(matches!(
            <CursorPagination as EndpointPagination<TestPage>>::apply(
                &mut zero_per_page,
                PageApply {
                    endpoint: "List",
                    page_index: 0,
                    ctx: &ctx,
                },
            ),
            Err(ApiClientError::Pagination { .. })
        ));
    }

    #[test]
    fn cursor_string_single_object_pagination_respects_missing_cursor_stop() {
        let ctx = ErrorContext {
            endpoint: "Items",
            method: Method::GET,
        };

        let mut stop_pagination = CursorPagination {
            cursor: Some("start".to_string()),
            per_page: 2,
            send_cursor_on_first: false,
            stop_when_cursor_missing: true,
        };
        let decision = <CursorPagination as EndpointPagination<TestPage>>::advance(
            &mut stop_pagination,
            &TestPage {
                items: vec!["a".to_string()],
                next: None,
            },
            PageAdvance {
                endpoint: "List",
                page_index: 0,
                received_items: 1,
            },
        )
        .expect("cursor stop advance");
        assert_eq!(decision, PageDecision::Stop);
        assert_eq!(
            <CursorPagination as EndpointPagination<TestPage>>::progress_key(&stop_pagination),
            None
        );

        let mut continue_pagination = CursorPagination {
            cursor: Some("start".to_string()),
            per_page: 2,
            send_cursor_on_first: false,
            stop_when_cursor_missing: false,
        };
        let decision = <CursorPagination as EndpointPagination<TestPage>>::advance(
            &mut continue_pagination,
            &TestPage {
                items: vec!["a".to_string()],
                next: None,
            },
            PageAdvance {
                endpoint: "List",
                page_index: 0,
                received_items: 1,
            },
        )
        .expect("cursor continue advance");
        assert_eq!(decision, PageDecision::Continue);
        assert_eq!(
            <CursorPagination as EndpointPagination<TestPage>>::progress_key(&continue_pagination),
            None
        );

        assert!(matches!(
            <CursorPagination as EndpointPagination<TestPage>>::apply(
                &mut continue_pagination,
                PageApply {
                    endpoint: "List",
                    page_index: 0,
                    ctx: &ctx,
                },
            ),
            Ok(PageApplyResult {
                expected_items_per_page: Some(_),
            })
        ));
    }

    #[test]
    fn cursor_string_single_object_pagination_preserves_empty_cursor() {
        let ctx = ErrorContext {
            endpoint: "Items",
            method: Method::GET,
        };
        let mut pagination = CursorPagination {
            cursor: Some("start".to_string()),
            per_page: 2,
            send_cursor_on_first: false,
            stop_when_cursor_missing: true,
        };

        let decision = <CursorPagination as EndpointPagination<TestPage>>::advance(
            &mut pagination,
            &TestPage {
                items: vec!["a".to_string()],
                next: Some(String::new()),
            },
            PageAdvance {
                endpoint: "List",
                page_index: 0,
                received_items: 1,
            },
        )
        .expect("cursor advance with empty string");
        assert_eq!(decision, PageDecision::Continue);
        assert_eq!(pagination.cursor, Some(String::new()));
        assert_eq!(
            <CursorPagination as EndpointPagination<TestPage>>::progress_key(&pagination),
            Some(ProgressKey::Str(String::new()))
        );

        let applied = <CursorPagination as EndpointPagination<TestPage>>::apply(
            &mut pagination,
            PageApply {
                endpoint: "List",
                page_index: 1,
                ctx: &ctx,
            },
        )
        .expect("cursor apply after empty cursor");
        assert_eq!(applied.expected_items_per_page, NonZeroUsize::new(2));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pagination::{EndpointField, PageItems};
    use http::Method;

    #[derive(Clone, Eq, PartialEq, Hash, Debug)]
    struct CursorToken(String);

    impl std::fmt::Display for CursorToken {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(&self.0)
        }
    }

    #[derive(Clone, Debug)]
    struct TestPage {
        items: Vec<String>,
        next: Option<CursorToken>,
    }

    impl PageItems for TestPage {
        type Item = String;

        fn item_count_hint(&self) -> Option<usize> {
            Some(self.items.len())
        }

        fn into_items(self) -> Vec<Self::Item> {
            self.items
        }
    }

    impl HasNextCursor for TestPage {
        type Cursor = CursorToken;

        fn next_cursor(&self) -> Option<Self::Cursor> {
            self.next.clone()
        }
    }

    #[derive(Clone, Debug)]
    struct TestEndpoint {
        cursor: Option<CursorToken>,
        per_page: u64,
    }

    #[test]
    fn cursor_controller_reads_writes_advances_and_tracks_progress_key() {
        let controller = CursorPagination {
            send_cursor_on_first: false,
            stop_when_cursor_missing: true,
            ..Default::default()
        };
        let bindings = CursorBindings {
            cursor: EndpointField::new(
                |ep: &TestEndpoint| ep.cursor.clone(),
                |ep: &mut TestEndpoint, value| ep.cursor = value,
            ),
            per_page: EndpointField::new(
                |ep: &TestEndpoint| ep.per_page,
                |ep: &mut TestEndpoint, value| ep.per_page = value,
            ),
        };
        let ctx = ErrorContext {
            endpoint: "Items",
            method: Method::GET,
        };
        let mut endpoint = TestEndpoint {
            cursor: Some(CursorToken("start".into())),
            per_page: 2,
        };

        let mut state =
            <CursorPagination as EndpointPaginationController<TestEndpoint, TestPage>>::init(
                &controller,
                &bindings,
                &endpoint,
                PageApply {
                    endpoint: "List",
                    page_index: 0,
                    ctx: &ctx,
                },
            )
            .expect("valid cursor controller");
        assert_eq!(state.cursor, Some(CursorToken("start".into())));
        assert_eq!(state.per_page, 2);
        assert_eq!(
            <CursorPagination as EndpointPaginationController<TestEndpoint, TestPage>>::progress_key(
                &controller,
                &state
            ),
            Some(ProgressKey::Str("start".into()))
        );

        let applied =
            <CursorPagination as EndpointPaginationController<TestEndpoint, TestPage>>::apply(
                &controller,
                &bindings,
                &mut state,
                &mut endpoint,
                PageApply {
                    endpoint: "List",
                    page_index: 0,
                    ctx: &ctx,
                },
            )
            .expect("cursor apply");
        assert_eq!(endpoint.cursor, None);
        assert_eq!(endpoint.per_page, 2);
        assert_eq!(applied.expected_items_per_page, NonZeroUsize::new(2));

        let decision =
            <CursorPagination as EndpointPaginationController<TestEndpoint, TestPage>>::advance(
                &controller,
                &bindings,
                &mut state,
                &TestPage {
                    items: vec!["a".into()],
                    next: Some(CursorToken("next-1".into())),
                },
                PageAdvance {
                    endpoint: "List",
                    page_index: 0,
                    received_items: 1,
                },
            )
            .expect("cursor advance");
        assert_eq!(decision, PageDecision::Continue);
        assert_eq!(state.started, true);
        assert_eq!(state.cursor, Some(CursorToken("next-1".into())));
        assert_eq!(
            <CursorPagination as EndpointPaginationController<TestEndpoint, TestPage>>::progress_key(
                &controller,
                &state
            ),
            Some(ProgressKey::Str("next-1".into()))
        );

        let applied =
            <CursorPagination as EndpointPaginationController<TestEndpoint, TestPage>>::apply(
                &controller,
                &bindings,
                &mut state,
                &mut endpoint,
                PageApply {
                    endpoint: "List",
                    page_index: 1,
                    ctx: &ctx,
                },
            )
            .expect("cursor apply after advance");
        assert_eq!(endpoint.cursor, Some(CursorToken("next-1".into())));
        assert_eq!(endpoint.per_page, 2);
        assert_eq!(applied.expected_items_per_page, NonZeroUsize::new(2));
    }

    #[test]
    fn cursor_controller_can_send_cursor_on_first_page() {
        let controller = CursorPagination {
            send_cursor_on_first: true,
            stop_when_cursor_missing: true,
            ..Default::default()
        };
        let bindings = CursorBindings {
            cursor: EndpointField::new(
                |ep: &TestEndpoint| ep.cursor.clone(),
                |ep: &mut TestEndpoint, value| ep.cursor = value,
            ),
            per_page: EndpointField::new(
                |ep: &TestEndpoint| ep.per_page,
                |ep: &mut TestEndpoint, value| ep.per_page = value,
            ),
        };
        let ctx = ErrorContext {
            endpoint: "Items",
            method: Method::GET,
        };
        let mut endpoint = TestEndpoint {
            cursor: Some(CursorToken("start".into())),
            per_page: 2,
        };

        let mut state =
            <CursorPagination as EndpointPaginationController<TestEndpoint, TestPage>>::init(
                &controller,
                &bindings,
                &endpoint,
                PageApply {
                    endpoint: "List",
                    page_index: 0,
                    ctx: &ctx,
                },
            )
            .expect("valid cursor controller");

        <CursorPagination as EndpointPaginationController<TestEndpoint, TestPage>>::apply(
            &controller,
            &bindings,
            &mut state,
            &mut endpoint,
            PageApply {
                endpoint: "List",
                page_index: 0,
                ctx: &ctx,
            },
        )
        .expect("cursor apply");
        assert_eq!(endpoint.cursor, Some(CursorToken("start".into())));
    }

    #[test]
    fn cursor_controller_preserves_typed_empty_cursor_without_string_filter() {
        let controller = CursorPagination {
            send_cursor_on_first: true,
            stop_when_cursor_missing: true,
            ..Default::default()
        };
        let bindings = CursorBindings {
            cursor: EndpointField::new(
                |ep: &TestEndpoint| ep.cursor.clone(),
                |ep: &mut TestEndpoint, value| ep.cursor = value,
            ),
            per_page: EndpointField::new(
                |ep: &TestEndpoint| ep.per_page,
                |ep: &mut TestEndpoint, value| ep.per_page = value,
            ),
        };
        let ctx = ErrorContext {
            endpoint: "Items",
            method: Method::GET,
        };
        let mut endpoint = TestEndpoint {
            cursor: Some(CursorToken(String::new())),
            per_page: 2,
        };

        let mut state =
            <CursorPagination as EndpointPaginationController<TestEndpoint, TestPage>>::init(
                &controller,
                &bindings,
                &endpoint,
                PageApply {
                    endpoint: "List",
                    page_index: 0,
                    ctx: &ctx,
                },
            )
            .expect("valid cursor controller");

        <CursorPagination as EndpointPaginationController<TestEndpoint, TestPage>>::apply(
            &controller,
            &bindings,
            &mut state,
            &mut endpoint,
            PageApply {
                endpoint: "List",
                page_index: 0,
                ctx: &ctx,
            },
        )
        .expect("cursor apply");
        assert_eq!(endpoint.cursor, Some(CursorToken(String::new())));
    }

    #[test]
    fn cursor_controller_missing_cursor_respects_stop_flag() {
        let bindings = CursorBindings {
            cursor: EndpointField::new(
                |ep: &TestEndpoint| ep.cursor.clone(),
                |ep: &mut TestEndpoint, value| ep.cursor = value,
            ),
            per_page: EndpointField::new(
                |ep: &TestEndpoint| ep.per_page,
                |ep: &mut TestEndpoint, value| ep.per_page = value,
            ),
        };
        let ctx = ErrorContext {
            endpoint: "Items",
            method: Method::GET,
        };
        let endpoint = TestEndpoint {
            cursor: None,
            per_page: 2,
        };
        let page = TestPage {
            items: vec!["a".into()],
            next: None,
        };

        let stop_controller = CursorPagination {
            send_cursor_on_first: false,
            stop_when_cursor_missing: true,
            ..Default::default()
        };
        let mut stop_state =
            <CursorPagination as EndpointPaginationController<TestEndpoint, TestPage>>::init(
                &stop_controller,
                &bindings,
                &endpoint,
                PageApply {
                    endpoint: "List",
                    page_index: 0,
                    ctx: &ctx,
                },
            )
            .expect("valid cursor controller");
        let decision =
            <CursorPagination as EndpointPaginationController<TestEndpoint, TestPage>>::advance(
                &stop_controller,
                &bindings,
                &mut stop_state,
                &page,
                PageAdvance {
                    endpoint: "List",
                    page_index: 0,
                    received_items: 1,
                },
            )
            .expect("cursor advance");
        assert_eq!(decision, PageDecision::Stop);
        assert_eq!(stop_state.cursor, None);

        let continue_controller = CursorPagination {
            send_cursor_on_first: false,
            stop_when_cursor_missing: false,
            ..Default::default()
        };
        let mut continue_state =
            <CursorPagination as EndpointPaginationController<TestEndpoint, TestPage>>::init(
                &continue_controller,
                &bindings,
                &endpoint,
                PageApply {
                    endpoint: "List",
                    page_index: 0,
                    ctx: &ctx,
                },
            )
            .expect("valid cursor controller");
        let decision =
            <CursorPagination as EndpointPaginationController<TestEndpoint, TestPage>>::advance(
                &continue_controller,
                &bindings,
                &mut continue_state,
                &page,
                PageAdvance {
                    endpoint: "List",
                    page_index: 0,
                    received_items: 1,
                },
            )
            .expect("cursor advance");
        assert_eq!(decision, PageDecision::Continue);
        assert_eq!(continue_state.cursor, None);
    }

    #[test]
    fn cursor_controller_rejects_zero_per_page() {
        let controller = CursorPagination::default();
        let bindings = CursorBindings {
            cursor: EndpointField::new(
                |ep: &TestEndpoint| ep.cursor.clone(),
                |ep: &mut TestEndpoint, value| ep.cursor = value,
            ),
            per_page: EndpointField::new(
                |ep: &TestEndpoint| ep.per_page,
                |ep: &mut TestEndpoint, value| ep.per_page = value,
            ),
        };
        let ctx = ErrorContext {
            endpoint: "Items",
            method: Method::GET,
        };
        let endpoint = TestEndpoint {
            cursor: None,
            per_page: 0,
        };

        assert!(matches!(
            <CursorPagination as EndpointPaginationController<TestEndpoint, TestPage>>::init(
                &controller,
                &bindings,
                &endpoint,
                PageApply {
                    endpoint: "List",
                    page_index: 0,
                    ctx: &ctx,
                },
            ),
            Err(ApiClientError::Pagination { .. })
        ));
    }
}
