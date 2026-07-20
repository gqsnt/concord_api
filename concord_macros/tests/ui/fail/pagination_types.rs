use concord_macros::api;

pub struct NotController {
    pub value: u64,
}

mod private_controller {
    pub struct Controller {
        state: u64,
    }

    impl concord_core::advanced::PaginationController<Vec<u8>> for Controller {
        fn next(
            self,
            _page: &Vec<u8>,
        ) -> Result<Option<Self>, concord_core::advanced::PaginationError> {
            Ok(None)
        }
    }
}

api! {
    client UndefinedField {
        base "https://example.test";
        GET Read(offset: u64, limit: std::num::NonZeroU32) -> Json<Vec<u8>> {
            paginate concord_core::advanced::OffsetPagination {
                offset: bind ep.missing,
                page_size: bind ep.limit,
            }
        }
    }
}

api! {
    client WrongOffsetType {
        base "https://example.test";
        GET Read(offset: String, limit: std::num::NonZeroU32) -> Json<Vec<u8>> {
            paginate concord_core::advanced::OffsetPagination {
                offset: bind ep.offset,
                page_size: bind ep.limit,
            }
        }
    }
}

api! {
    client WrongCursorType {
        base "https://example.test";
        GET Read(cursor: String) -> Json<Vec<u8>> {
            paginate concord_core::advanced::CursorPagination<String> {
                cursor: bind ep.cursor,
            }
        }
    }
}

api! {
    client MissingControllerField {
        base "https://example.test";
        GET Read(offset: u64) -> Json<Vec<u8>> {
            paginate concord_core::advanced::OffsetPagination {
                offset: bind ep.offset,
            }
        }
    }
}

api! {
    client UnknownControllerField {
        base "https://example.test";
        GET Read(offset: u64, limit: std::num::NonZeroU32) -> Json<Vec<u8>> {
            paginate concord_core::advanced::OffsetPagination {
                offset: bind ep.offset,
                page_size: bind ep.limit,
                unknown: 0,
            }
        }
    }
}

api! {
    client NonControllerType {
        base "https://example.test";
        GET Read(value: u64) -> Json<Vec<u8>> {
            paginate crate::NotController {
                value: bind ep.value,
            }
        }
    }
}

api! {
    client InaccessibleControllerField {
        base "https://example.test";
        GET Read(state: u64) -> Json<Vec<u8>> {
            paginate crate::private_controller::Controller {
                state: bind ep.state,
            }
        }
    }
}

fn main() {}
