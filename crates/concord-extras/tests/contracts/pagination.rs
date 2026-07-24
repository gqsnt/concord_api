use std::num::{NonZeroU32, NonZeroU64, NonZeroUsize};

use concord_core::pagination::{PaginationController, PaginationErrorKind};
use concord_extras::pagination::{
    CursorPagination, HasNextCursor, OffsetPagination, PagePagination,
};

struct CursorPage {
    next: Option<String>,
}

impl HasNextCursor<String> for CursorPage {
    fn next_cursor(&self) -> Option<&String> {
        self.next.as_ref()
    }
}

fn assert_send_sync_static<T: Send + Sync + 'static>() {}

#[test]
fn public_controllers_progress_checked_and_preserve_cursor_presence() {
    let offset = OffsetPagination::new(9, NonZeroU32::new(3).expect("nonzero"));
    assert_eq!(offset.offset(), 9);
    assert_eq!(offset.page_size().get(), 3);
    assert_eq!(
        <OffsetPagination as PaginationController<()>>::short_page_size(&offset),
        NonZeroUsize::new(3)
    );
    let next = <OffsetPagination as PaginationController<()>>::next(offset, &())
        .expect("offset next")
        .expect("offset continues");
    assert_eq!(next.offset(), 12);
    let Err(error) = <OffsetPagination as PaginationController<()>>::next(
        OffsetPagination::new(u64::MAX, NonZeroU32::MIN),
        &(),
    ) else {
        panic!("offset overflow must fail");
    };
    assert_eq!(error.kind(), PaginationErrorKind::Overflow);

    let page = PagePagination::new(
        NonZeroU64::new(2).expect("nonzero"),
        NonZeroU32::new(25).expect("nonzero"),
    );
    assert_eq!(page.page().get(), 2);
    assert_eq!(page.page_size().get(), 25);
    let Err(error) = <PagePagination as PaginationController<()>>::next(
        PagePagination::new(NonZeroU64::new(u64::MAX).expect("nonzero"), NonZeroU32::MIN),
        &(),
    ) else {
        panic!("page overflow must fail");
    };
    assert_eq!(error.kind(), PaginationErrorKind::Overflow);

    let cursor = CursorPagination::new(None::<String>);
    let next = cursor
        .next(&CursorPage {
            next: Some(String::new()),
        })
        .expect("cursor next")
        .expect("present empty cursor continues");
    assert_eq!(next.cursor().map(String::as_str), Some(""));
    assert!(
        next.next(&CursorPage { next: None })
            .expect("absent cursor")
            .is_none()
    );

    assert_send_sync_static::<OffsetPagination>();
    assert_send_sync_static::<PagePagination>();
    assert_send_sync_static::<CursorPagination<String>>();
}
