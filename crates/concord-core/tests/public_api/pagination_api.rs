use std::cell::Cell;
use std::error::Error;
use std::num::NonZeroU32;

use concord_core::PageLimit;
use concord_core::contracts::{PaginationController, PaginationError, PaginationErrorKind};

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

struct PrivateController {
    private_cursor: Option<String>,
    _not_sync: Cell<()>,
}

impl PaginationController<CustomPage<u64>> for PrivateController {
    fn next(self, page: &CustomPage<u64>) -> Result<Option<Self>, PaginationError> {
        let _previous_cursor = self.private_cursor;
        Ok(page.cursor.as_ref().map(|cursor| Self {
            private_cursor: Some(cursor.clone()),
            _not_sync: Cell::new(()),
        }))
    }
}

fn assert_send_static<T: Send + 'static>() {}

#[test]
fn stable_pagination_contracts_compile_without_private_imports_or_item_traits() {
    assert_eq!(PageLimit::from(8), PageLimit::Take(8));
    assert_eq!(
        PageLimit::HardCap(NonZeroU32::new(4).expect("nonzero")),
        PageLimit::HardCap(NonZeroU32::new(4).expect("nonzero"))
    );

    assert_send_static::<PrivateController>();

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
        _not_sync: Cell::new(()),
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
