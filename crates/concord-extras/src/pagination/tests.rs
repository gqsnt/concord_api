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

fn round_trip<P, C>(controller: &C) -> C
where
    C: ResumableController<P>,
    C::Cursor: std::fmt::Debug + PartialEq,
{
    let state = controller.save_state();
    let restored = C::restore_state(&state).expect("valid saved state");
    assert_eq!(restored.cursor(), controller.cursor());
    assert_eq!(restored.save_state(), state, "canonical encoding");
    restored
}

fn rejects_state<P, C>(state: &[u8])
where
    C: ResumableController<P>,
{
    let error = C::restore_state(state).err().expect("invalid state");
    assert_eq!(error.kind(), PaginationErrorKind::Controller);
    assert_eq!(error.to_string(), "pagination failed");
    assert_eq!(
        format!("{error:?}"),
        "PaginationError { kind: Controller, safe_message: None }"
    );
}

#[test]
fn resumed_offset_and_page_preserve_position_size_and_checked_progression() {
    let controller = round_trip::<Vec<u8>, _>(&offset_start(7, 2));
    let Ok(PageStep::Continue { next, items }) =
        advance(controller, vec![1_u8, 2], &HeaderMap::new(), 10)
    else {
        panic!("restored full offset page continues");
    };
    assert_eq!(items, [1, 2]);
    assert_eq!(next.offset, 9);
    assert!(matches!(
        advance(
            round_trip::<Vec<u8>, _>(&next),
            vec![3_u8],
            &HeaderMap::new(),
            10
        ),
        Ok(PageStep::Done { .. })
    ));
    let controller = round_trip::<Vec<u8>, _>(&PagePagination {
        page: NonZeroU64::new(4).unwrap(),
        page_size: NonZeroU32::new(2).unwrap(),
    });
    let Ok(PageStep::Continue { next, .. }) =
        advance(controller, vec![1_u8, 2], &HeaderMap::new(), 10)
    else {
        panic!("restored full numbered page continues");
    };
    assert_eq!(next.page.get(), 5);
    for controller in [offset_start(u64::MAX, 1), offset_start(0, u32::MAX)] {
        round_trip::<Vec<u8>, _>(&controller);
    }
    let controller = round_trip::<Vec<u8>, _>(&PagePagination {
        page: NonZeroU64::MAX,
        page_size: NonZeroU32::new(1).unwrap(),
    });
    let error = advance(controller, vec![1_u8], &HeaderMap::new(), 10)
        .err()
        .expect("overflow remains checked after restore");
    assert_eq!(error.kind(), PaginationErrorKind::Overflow);
}

#[test]
fn resumed_string_controllers_preserve_optional_cursors_and_advance() {
    for cursor in [
        None,
        Some(String::new()),
        Some("secret-雪\0cursor".to_owned()),
    ] {
        round_trip::<CursorPage, _>(&CursorPagination {
            cursor: cursor.clone(),
        });
        round_trip::<Vec<u8>, _>(&NextHeaderPagination {
            cursor: cursor.clone(),
        });
        round_trip::<Vec<u8>, _>(&LinkCursorPagination { cursor });
    }
    let cursor = round_trip::<CursorPage, _>(&CursorPagination { cursor: None });
    let Ok(PageStep::Continue { next, items }) = advance(
        cursor,
        CursorPage {
            items: vec![1],
            next: Some("next".to_owned()),
        },
        &HeaderMap::new(),
        10,
    ) else {
        panic!("restored cursor follows the next page");
    };
    assert_eq!(items, [1]);
    assert_eq!(
        round_trip::<CursorPage, _>(&next).cursor.as_deref(),
        Some("next")
    );

    let header = round_trip::<Vec<u8>, _>(&NextHeaderPagination { cursor: None });
    let mut headers = HeaderMap::new();
    headers.insert("x-next-cursor", HeaderValue::from_static("next-header"));
    let Ok(PageStep::Continue { next, .. }) = advance(header, vec![1_u8], &headers, 10) else {
        panic!("restored header controller advances");
    };
    assert_eq!(next.cursor.as_deref(), Some("next-header"));
    let link = round_trip::<Vec<u8>, _>(&link_start());
    let headers = link_headers(&["<?view=summary&cursor=next-link&limit=2>; rel=next"]);
    let Ok(PageStep::Continue { next, .. }) = advance(link, vec![1_u8], &headers, 10) else {
        panic!("restored link controller advances");
    };
    assert_eq!(next.cursor.as_deref(), Some("next-link"));
}

struct IntegerCursorPage<C>(Option<C>);

impl<C> IntoIterator for IntegerCursorPage<C> {
    type Item = u8;
    type IntoIter = std::iter::Empty<u8>;

    fn into_iter(self) -> Self::IntoIter {
        std::iter::empty()
    }
}

impl<C> HasNextCursor<C> for IntegerCursorPage<C> {
    fn next_cursor(&self) -> Option<&C> {
        self.0.as_ref()
    }
}

#[test]
fn all_primitive_integer_cursors_round_trip_boundaries_and_continue() {
    macro_rules! check {
        ($($cursor:ty),+ $(,)?) => {$(
            for cursor in [None, Some(<$cursor>::MIN), Some(0), Some(<$cursor>::MAX)] {
                let controller = round_trip::<IntegerCursorPage<$cursor>, _>(
                    &CursorPagination { cursor },
                );
                let Ok(PageStep::Continue { next, .. }) = advance(
                    controller, IntegerCursorPage(Some(<$cursor>::MAX)), &HeaderMap::new(), 10,
                ) else {
                    panic!("restored integer cursor advances");
                };
                assert_eq!(next.cursor, Some(<$cursor>::MAX));
            }
        )+};
    }
    check!(
        u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize
    );
}

#[test]
fn saved_state_has_explicit_version_and_big_endian_representation() {
    assert_eq!(
        <OffsetPagination as ResumableController<Vec<u8>>>::save_state(&offset_start(7, 2)),
        b"CCP\x01\x01\0\0\0\0\0\0\0\x07\0\0\0\x02"
    );
    assert_eq!(
        <CursorPagination<String> as ResumableController<CursorPage>>::save_state(
            &CursorPagination { cursor: None }
        ),
        b"CCP\x01\x03\x00"
    );
    assert_eq!(
        <CursorPagination<String> as ResumableController<CursorPage>>::save_state(
            &CursorPagination {
                cursor: Some(String::new())
            }
        ),
        b"CCP\x01\x03\x01"
    );
    assert_eq!(
        <CursorPagination<i16> as ResumableController<IntegerCursorPage<i16>>>::save_state(
            &CursorPagination { cursor: Some(-2) }
        ),
        b"CCP\x01\x17\x01\xff\xfe"
    );
}

#[test]
fn saved_state_rejects_truncation_wrong_versions_types_and_invalid_values() {
    let state = <OffsetPagination as ResumableController<Vec<u8>>>::save_state(&offset_start(7, 2));
    for length in 0..state.len() {
        rejects_state::<Vec<u8>, OffsetPagination>(&state[..length]);
    }
    let mut malformed = state.clone();
    malformed.push(0);
    rejects_state::<Vec<u8>, OffsetPagination>(&malformed);
    malformed = state.clone();
    malformed[3] = 2;
    rejects_state::<Vec<u8>, OffsetPagination>(&malformed);
    malformed = state.clone();
    malformed[0] = b'X';
    rejects_state::<Vec<u8>, OffsetPagination>(&malformed);
    rejects_state::<Vec<u8>, PagePagination>(&state);
    malformed = state;
    malformed[13..17].fill(0);
    rejects_state::<Vec<u8>, OffsetPagination>(&malformed);
    malformed[4] = 2;
    rejects_state::<Vec<u8>, PagePagination>(&malformed);
    malformed[5..13].fill(0);
    malformed[16] = 1;
    rejects_state::<Vec<u8>, PagePagination>(&malformed);

    for payload in [&b""[..], &[2], &[0, 1], &[1, 0xff]] {
        rejects_state::<CursorPage, CursorPagination<String>>(&encode_state(3, payload));
        rejects_state::<Vec<u8>, NextHeaderPagination>(&encode_state(4, payload));
        rejects_state::<Vec<u8>, LinkCursorPagination>(&encode_state(5, payload));
    }
    rejects_state::<Vec<u8>, NextHeaderPagination>(&encode_string_state(5, &None));
    rejects_state::<Vec<u8>, LinkCursorPagination>(&encode_string_state(4, &None));
    macro_rules! check_integer {
        ($cursor:ty, $tag:literal, $width:expr) => {
            for payload in [
                vec![],
                vec![2],
                vec![0, 1],
                vec![1; $width],
                vec![1; $width + 2],
            ] {
                rejects_state::<IntegerCursorPage<$cursor>, CursorPagination<$cursor>>(
                    &encode_state($tag, &payload),
                );
            }
        };
    }
    check_integer!(u8, 16, 1);
    check_integer!(u16, 17, 2);
    check_integer!(u32, 18, 4);
    check_integer!(u64, 19, 8);
    check_integer!(u128, 20, 16);
    check_integer!(usize, 21, 16);
    check_integer!(i8, 22, 1);
    check_integer!(i16, 23, 2);
    check_integer!(i32, 24, 4);
    check_integer!(i64, 25, 8);
    check_integer!(i128, 26, 16);
    check_integer!(isize, 27, 16);
    rejects_state::<IntegerCursorPage<usize>, CursorPagination<usize>>(&encode_state(
        21,
        &[&[1][..], &u128::MAX.to_be_bytes()].concat(),
    ));
    rejects_state::<IntegerCursorPage<isize>, CursorPagination<isize>>(&encode_state(
        27,
        &[&[1][..], &i128::MAX.to_be_bytes()].concat(),
    ));
}
