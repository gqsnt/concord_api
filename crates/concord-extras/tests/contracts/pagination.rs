use std::num::{NonZeroU32, NonZeroU64};

use concord_core::pagination::PaginationController;
use concord_extras::pagination::{
    CursorInit, CursorPagination, HasNextCursor, LinkCursorPagination, NextHeaderPagination,
    OffsetInit, OffsetPagination, PageInit, PagePagination,
};

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

fn assert_send_static<T: Send + 'static>() {}

#[test]
fn public_controllers_start_from_public_init_types_and_map_whole_cursors() {
    let offset = <OffsetPagination as PaginationController<Vec<u8>>>::start(OffsetInit {
        offset: 9,
        page_size: NonZeroU32::new(3).expect("nonzero"),
    })
    .expect("offset start");
    let cursor: u64 = <OffsetPagination as PaginationController<Vec<u8>>>::cursor(&offset);
    assert_eq!(cursor, 9);

    let page = <PagePagination as PaginationController<Vec<u8>>>::start(PageInit {
        page: NonZeroU64::new(2).expect("nonzero"),
        page_size: NonZeroU32::new(25).expect("nonzero"),
    })
    .expect("page start");
    let cursor: NonZeroU64 = <PagePagination as PaginationController<Vec<u8>>>::cursor(&page);
    assert_eq!(cursor.get(), 2);

    let cursor_controller =
        <CursorPagination<String> as PaginationController<CursorPage>>::start(CursorInit {
            cursor: None,
        })
        .expect("cursor start");
    let cursor: Option<String> =
        <CursorPagination<String> as PaginationController<CursorPage>>::cursor(&cursor_controller);
    assert!(cursor.is_none());

    let header = <NextHeaderPagination as PaginationController<Vec<u8>>>::start(CursorInit {
        cursor: Some("h".to_owned()),
    })
    .expect("header start");
    assert_eq!(
        <NextHeaderPagination as PaginationController<Vec<u8>>>::cursor(&header),
        Some("h".to_owned())
    );
    let link = <LinkCursorPagination as PaginationController<Vec<u8>>>::start(CursorInit {
        cursor: Some("l".to_owned()),
    })
    .expect("link start");
    assert_eq!(
        <LinkCursorPagination as PaginationController<Vec<u8>>>::cursor(&link),
        Some("l".to_owned())
    );

    fn assert_controller<Page, C: PaginationController<Page>>() {}
    assert_controller::<Vec<u8>, OffsetPagination>();
    assert_controller::<Vec<u8>, PagePagination>();
    assert_controller::<CursorPage, CursorPagination<String>>();
    assert_controller::<Vec<u8>, NextHeaderPagination>();
    assert_controller::<Vec<u8>, LinkCursorPagination>();

    assert_send_static::<OffsetPagination>();
    assert_send_static::<PagePagination>();
    assert_send_static::<CursorPagination<String>>();
    assert_send_static::<OffsetInit>();
    assert_send_static::<CursorInit<String>>();
}
