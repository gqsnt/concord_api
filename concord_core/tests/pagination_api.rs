use std::error::Error;
use std::num::{NonZeroU32, NonZeroU64};

use concord_core::advanced::{
    CursorPagination, HasNextCursor, HeaderName, OffsetPagination, PagePagination,
    PaginationController, PaginationError, PaginationErrorKind, PaginationRequest,
};
use concord_core::prelude::PageLimit;

struct CustomPage<T> {
    items: Vec<T>,
    cursor: Option<String>,
}

impl<T> IntoIterator for CustomPage<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl<T> HasNextCursor<String> for CustomPage<T> {
    fn next_cursor(&self) -> Option<&String> {
        self.cursor.as_ref()
    }
}

#[derive(Clone)]
struct PrivateController {
    private_cursor: Option<String>,
    private_header: HeaderName,
}

impl PaginationController<CustomPage<u64>> for PrivateController {
    fn apply(&self, request: &mut PaginationRequest<'_>) -> Result<(), PaginationError> {
        match &self.private_cursor {
            Some(cursor) => request.set_query("cursor", cursor)?,
            None => request.remove_query("cursor")?,
        }
        request.set_header(self.private_header.clone(), "page")
    }

    fn next(&self, page: &CustomPage<u64>) -> Result<Option<Self>, PaginationError> {
        Ok(page.cursor.as_ref().map(|cursor| Self {
            private_cursor: Some(cursor.clone()),
            private_header: self.private_header.clone(),
        }))
    }
}

fn assert_send_sync_static<T: Send + Sync + 'static>() {}

#[test]
fn stable_pagination_contracts_compile_without_private_imports_or_item_traits() {
    assert_eq!(PageLimit::from(8), PageLimit::Take(8));
    assert_eq!(
        PageLimit::HardCap(NonZeroU32::new(4).expect("nonzero")),
        PageLimit::HardCap(NonZeroU32::new(4).expect("nonzero"))
    );

    let offset = OffsetPagination::new(9, NonZeroU32::new(3).expect("nonzero"));
    assert_eq!(offset.offset(), 9);
    assert_eq!(offset.page_size().get(), 3);
    let page = PagePagination::new(
        NonZeroU64::new(2).expect("nonzero"),
        NonZeroU32::new(25).expect("nonzero"),
    );
    assert_eq!(page.page().get(), 2);
    assert_eq!(page.page_size().get(), 25);
    let cursor = CursorPagination::new(Some("opaque".to_owned()));
    assert_eq!(cursor.cursor().map(String::as_str), Some("opaque"));

    assert_send_sync_static::<OffsetPagination>();
    assert_send_sync_static::<PagePagination>();
    assert_send_sync_static::<CursorPagination<String>>();
    assert_send_sync_static::<PrivateController>();

    let ordinary_vec_page = vec![1_u64, 2, 3];
    let ordinary_items: Vec<_> = ordinary_vec_page.into_iter().collect();
    assert_eq!(ordinary_items, [1, 2, 3]);
    let custom_items: Vec<_> = CustomPage {
        items: vec![4_u64, 5],
        cursor: None,
    }
    .into_iter()
    .collect();
    assert_eq!(custom_items, [4, 5]);

    let controller = PrivateController {
        private_cursor: None,
        private_header: HeaderName::from_static("x-page"),
    };
    assert!(
        controller
            .next(&CustomPage::<u64> {
                items: Vec::new(),
                cursor: None,
            })
            .expect("controller next")
            .is_none()
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
