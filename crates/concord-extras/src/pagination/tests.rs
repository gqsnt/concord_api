//! V3-P08: every built-in controller through its public Init API, including
//! duplicate header/Link continuations and Link origin/path/query rejection.

use concord_core::contracts::{HeaderMap, HeaderValue, StatusCode};

use super::*;

const CURRENT: &str = "https://catalog.example.test/v1/items?view=summary&cursor=current&limit=2";

fn current() -> Url {
    Url::parse(CURRENT).expect("current URL")
}

fn response<P, C>(page: P, headers: &HeaderMap) -> PageResponse<P>
where
    C: PaginationController<P>,
{
    PageResponse::detached(page, StatusCode::OK, headers, &current(), C::DESCRIPTION, 0)
        .expect("declared interests are valid")
}

fn advance<P, C>(
    controller: C,
    page: P,
    headers: &HeaderMap,
    remaining: usize,
) -> Result<PageStep<C, C::Item>, PaginationError>
where
    C: PaginationController<P>,
{
    let page = response::<P, C>(page, headers);
    <C as PaginationController<P>>::advance(controller, page, PageDecisionLimits::new(remaining))
}

fn link_headers(values: &[&'static str]) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for value in values {
        headers.append("link", HeaderValue::from_static(value));
    }
    headers
}

#[derive(Clone)]
struct CursorPage {
    items: Vec<u8>,
    next: Option<String>,
}

impl IntoIterator for CursorPage {
    type Item = u8;
    type IntoIter = std::vec::IntoIter<u8>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl HasNextCursor<String> for CursorPage {
    fn next_cursor(&self) -> Option<&String> {
        self.next.as_ref()
    }
}

fn offset_start(offset: u64, size: u32) -> OffsetPagination {
    <OffsetPagination as PaginationController<Vec<u8>>>::start(OffsetInit {
        offset,
        page_size: NonZeroU32::new(size).expect("nonzero"),
    })
    .expect("offset start")
}

#[test]
fn offset_pagination_continues_full_pages_and_proves_done_on_short_pages() {
    let controller = offset_start(7, 2);
    assert_eq!(
        <OffsetPagination as PaginationController<Vec<u8>>>::cursor(&controller),
        7
    );
    let Ok(PageStep::Continue { next, items }) =
        advance(controller, vec![1_u8, 2], &HeaderMap::new(), 10)
    else {
        panic!("a full page continues");
    };
    assert_eq!(items, [1, 2]);
    assert_eq!(
        <OffsetPagination as PaginationController<Vec<u8>>>::cursor(&next),
        9
    );
    let Ok(PageStep::Done { items }) = advance(next, vec![3_u8], &HeaderMap::new(), 10) else {
        panic!("a short page proves exhaustion");
    };
    assert_eq!(items, [3]);

    let overflow = advance(
        offset_start(u64::MAX, 2),
        vec![1_u8, 2],
        &HeaderMap::new(),
        10,
    )
    .err()
    .expect("checked offset arithmetic");
    assert_eq!(overflow.kind(), PaginationErrorKind::Overflow);

    let ceiling = advance(offset_start(0, 3), vec![1_u8, 2, 3], &HeaderMap::new(), 2)
        .err()
        .expect("batch larger than the remaining budget is not truncated");
    assert_eq!(
        ceiling.kind(),
        PaginationErrorKind::CollectedItemCeilingExceeded
    );
}

#[test]
fn page_pagination_uses_checked_one_based_pages() {
    let start = |page: u64| {
        <PagePagination as PaginationController<Vec<u8>>>::start(PageInit {
            page: NonZeroU64::new(page).expect("nonzero"),
            page_size: NonZeroU32::new(2).expect("nonzero"),
        })
        .expect("page start")
    };
    let controller = start(4);
    assert_eq!(
        <PagePagination as PaginationController<Vec<u8>>>::cursor(&controller).get(),
        4
    );
    let Ok(PageStep::Continue { next, .. }) =
        advance(controller, vec![1_u8, 2], &HeaderMap::new(), 10)
    else {
        panic!("full page continues");
    };
    assert_eq!(
        <PagePagination as PaginationController<Vec<u8>>>::cursor(&next).get(),
        5
    );
    assert!(matches!(
        advance(next, Vec::<u8>::new(), &HeaderMap::new(), 10),
        Ok(PageStep::Done { .. })
    ));
    let overflow = advance(start(u64::MAX), vec![1_u8, 2], &HeaderMap::new(), 10)
        .err()
        .expect("checked page arithmetic");
    assert_eq!(overflow.kind(), PaginationErrorKind::Overflow);
}

#[test]
fn cursor_pagination_distinguishes_absent_from_present_empty_cursor() {
    let controller =
        <CursorPagination<String> as PaginationController<CursorPage>>::start(CursorInit {
            cursor: None,
        })
        .expect("cursor start");
    assert_eq!(
        <CursorPagination<String> as PaginationController<CursorPage>>::cursor(&controller),
        None
    );
    let page = CursorPage {
        items: vec![1, 2],
        next: Some(String::new()),
    };
    let Ok(PageStep::Continue { next, items }) = advance(controller, page, &HeaderMap::new(), 10)
    else {
        panic!("a present empty cursor remains a present candidate");
    };
    assert_eq!(items, [1, 2]);
    assert_eq!(
        <CursorPagination<String> as PaginationController<CursorPage>>::cursor(&next),
        Some(String::new())
    );
    let last = CursorPage {
        items: vec![3],
        next: None,
    };
    assert!(matches!(
        advance(next, last, &HeaderMap::new(), 10),
        Ok(PageStep::Done { items }) if items == [3]
    ));
}

#[test]
fn next_header_pagination_rejects_duplicate_and_invalid_values() {
    let start = || {
        <NextHeaderPagination as PaginationController<Vec<u8>>>::start(CursorInit {
            cursor: Some("first".to_owned()),
        })
        .expect("header start")
    };
    assert_eq!(
        <NextHeaderPagination as PaginationController<Vec<u8>>>::cursor(&start()),
        Some("first".to_owned())
    );

    let mut one = HeaderMap::new();
    one.insert("x-next-cursor", HeaderValue::from_static("second"));
    let Ok(PageStep::Continue { next, .. }) = advance(start(), vec![1_u8], &one, 10) else {
        panic!("one header value continues");
    };
    assert_eq!(
        <NextHeaderPagination as PaginationController<Vec<u8>>>::cursor(&next),
        Some("second".to_owned())
    );

    assert!(matches!(
        advance(start(), vec![1_u8], &HeaderMap::new(), 10),
        Ok(PageStep::Done { .. })
    ));

    let mut duplicate = HeaderMap::new();
    duplicate.append("x-next-cursor", HeaderValue::from_static("a"));
    duplicate.append("x-next-cursor", HeaderValue::from_static("b"));
    let error = advance(start(), vec![1_u8], &duplicate, 10)
        .err()
        .expect("duplicate header values");
    assert_eq!(error.kind(), PaginationErrorKind::InvalidContinuation);

    let mut invalid = HeaderMap::new();
    invalid.insert(
        "x-next-cursor",
        HeaderValue::from_bytes(b"caf\xe9").expect("obs-text header value"),
    );
    let error = advance(start(), vec![1_u8], &invalid, 10)
        .err()
        .expect("non-text header value");
    assert_eq!(error.kind(), PaginationErrorKind::InvalidContinuation);
}

fn link_start() -> LinkCursorPagination {
    <LinkCursorPagination as PaginationController<Vec<u8>>>::start(CursorInit {
        cursor: Some("current".to_owned()),
    })
    .expect("link start")
}

fn link_cursor(values: &[&'static str]) -> Result<Option<String>, PaginationErrorKind> {
    match advance(link_start(), vec![1_u8], &link_headers(values), 10) {
        Ok(PageStep::Continue { next, .. }) => {
            Ok(<LinkCursorPagination as PaginationController<Vec<u8>>>::cursor(&next))
        }
        Ok(PageStep::Done { .. }) => Ok(None),
        Err(error) => Err(error.kind()),
    }
}

#[test]
fn link_pagination_accepts_one_same_origin_next_link_with_one_cursor() {
    assert_eq!(
        <LinkCursorPagination as PaginationController<Vec<u8>>>::cursor(&link_start()),
        Some("current".to_owned())
    );
    assert_eq!(
        link_cursor(&[
            "</v1/items?view=summary&cursor=next%20page&limit=2>; rel=\"next\", </v1/items?view=summary&cursor=prev&limit=2>; rel=prev"
        ]),
        Ok(Some("next page".to_owned()))
    );
    // Commas inside the target and inside quoted parameters do not split.
    assert_eq!(
        link_cursor(&[
            "<?view=summary&cursor=a,b&limit=2>; title=\"x, y; z\"; rel=\"prefetch next\""
        ]),
        Ok(Some("a,b".to_owned()))
    );
    assert_eq!(
        link_cursor(&["</v1/items?view=summary&cursor=prev&limit=2>; rel=prev"]),
        Ok(None),
        "no rel=next proves exhaustion"
    );
    assert_eq!(link_cursor(&[]), Ok(None));
}

#[test]
fn link_pagination_rejects_duplicates_and_every_forbidden_change() {
    use PaginationErrorKind::{CrossOriginContinuation, InvalidContinuation};

    let cases: &[(&[&'static str], PaginationErrorKind)] = &[
        (
            &[
                "</v1/items?view=summary&cursor=a&limit=2>; rel=next, </v1/items?view=summary&cursor=b&limit=2>; rel=next",
            ],
            InvalidContinuation,
        ),
        (
            &[
                "</v1/items?view=summary&cursor=a&limit=2>; rel=next",
                "</v1/items?view=summary&cursor=b&limit=2>; rel=next",
            ],
            InvalidContinuation,
        ),
        (
            &["<https://attacker.example/v1/items?view=summary&cursor=x&limit=2>; rel=next"],
            CrossOriginContinuation,
        ),
        (
            &["<http://catalog.example.test/v1/items?view=summary&cursor=x&limit=2>; rel=next"],
            CrossOriginContinuation,
        ),
        (
            &["</v1/other?view=summary&cursor=x&limit=2>; rel=next"],
            InvalidContinuation,
        ),
        (
            &[
                "<https://user:pw@catalog.example.test/v1/items?view=summary&cursor=x&limit=2>; rel=next",
            ],
            InvalidContinuation,
        ),
        (
            &["</v1/items?view=summary&cursor=x&limit=2#frag>; rel=next"],
            InvalidContinuation,
        ),
        (
            &["</v1/items?view=full&cursor=x&limit=2>; rel=next"],
            InvalidContinuation,
        ),
        (
            &["</v1/items?view=summary&cursor=x&limit=2&extra=1>; rel=next"],
            InvalidContinuation,
        ),
        (
            &["</v1/items?view=summary&limit=2>; rel=next"],
            InvalidContinuation,
        ),
        (
            &["</v1/items?view=summary&cursor=x&cursor=y&limit=2>; rel=next"],
            InvalidContinuation,
        ),
        (&["</v1/items?cursor=x; rel=next"], InvalidContinuation),
        (&["garbage; rel=next"], InvalidContinuation),
        (
            &["</v1/items?view=summary&cursor=x&limit=2>; rel=\"next"],
            InvalidContinuation,
        ),
    ];
    for (values, expected) in cases {
        assert_eq!(
            link_cursor(values),
            Err(*expected),
            "Link values {values:?}"
        );
    }
}

#[test]
fn link_controller_requires_its_declared_current_uri() {
    // A view built without the current URI interest cannot be interpreted.
    let page = PageResponse::detached(
        vec![1_u8],
        StatusCode::OK,
        &link_headers(&["</v1/items?view=summary&cursor=x&limit=2>; rel=next"]),
        &current(),
        PaginationDescription::new(&["link"], false),
        0,
    )
    .expect("view without current URI");
    let error = <LinkCursorPagination as PaginationController<Vec<u8>>>::advance(
        link_start(),
        page,
        PageDecisionLimits::new(10),
    )
    .err()
    .expect("current URI is required");
    assert_eq!(
        error.kind(),
        PaginationErrorKind::InvalidControllerDescription
    );
}

#[test]
fn controller_descriptions_declare_bounded_interests() {
    assert_eq!(
        <NextHeaderPagination as PaginationController<Vec<u8>>>::DESCRIPTION.response_headers(),
        [NEXT_CURSOR_HEADER]
    );
    let link = <LinkCursorPagination as PaginationController<Vec<u8>>>::DESCRIPTION;
    assert_eq!(link.response_headers(), ["link"]);
    assert!(link.includes_current_uri());
    assert_eq!(
        <OffsetPagination as PaginationController<Vec<u8>>>::DESCRIPTION,
        PaginationDescription::NONE
    );
}
