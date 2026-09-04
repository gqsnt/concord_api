use std::num::{NonZeroU32, NonZeroU64};

use concord_core::pagination::PaginationController;
use concord_extras::pagination::{
    CursorPagination, HasNextCursor, OffsetPagination, PagePagination,
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

fn assert_send_sync_static<T: Send + Sync + 'static>() {}

#[test]
fn public_controllers_expose_initial_state_and_semantic_contracts() {
    let offset = OffsetPagination::new(9, NonZeroU32::new(3).expect("nonzero"));
    assert_eq!(offset.offset(), 9);
    assert_eq!(offset.page_size().get(), 3);

    let page = PagePagination::new(
        NonZeroU64::new(2).expect("nonzero"),
        NonZeroU32::new(25).expect("nonzero"),
    );
    assert_eq!(page.page().get(), 2);
    assert_eq!(page.page_size().get(), 25);
    assert_eq!(page.page_size().get(), 25);

    let cursor = CursorPagination::new(None::<String>);
    assert!(cursor.cursor().is_none());

    fn assert_controller<Page, C: PaginationController<Page>>() {}
    assert_controller::<Vec<u8>, OffsetPagination>();
    assert_controller::<Vec<u8>, PagePagination>();
    assert_controller::<CursorPage, CursorPagination<String>>();

    assert_send_sync_static::<OffsetPagination>();
    assert_send_sync_static::<PagePagination>();
    assert_send_sync_static::<CursorPagination<String>>();
}
