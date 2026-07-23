use concord_macros::api;

#[path = "../../support/mod.rs"]
mod support;
#[allow(unused_imports)]
use support::Json;

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
    }

    GET read(offset: u64, limit: std::num::NonZeroU32) -> Json<Vec<u8>> {
        paginate crate::support::OffsetController {
            offset: bind ep.missing,
            page_size: bind ep.limit,
        }
    }
}

api! {
    client WrongOffsetType {
        base "https://example.test";
    }

    GET read(offset: String, limit: std::num::NonZeroU32) -> Json<Vec<u8>> {
        paginate crate::support::OffsetController {
            offset: bind ep.offset,
            page_size: bind ep.limit,
        }
    }
}

api! {
    client WrongCursorType {
        base "https://example.test";
    }

    GET read(cursor: String) -> Json<Vec<u8>> {
        paginate crate::support::CursorController<String> {
            cursor: bind ep.cursor,
        }
    }
}

api! {
    client MissingControllerField {
        base "https://example.test";
    }

    GET read(offset: u64) -> Json<Vec<u8>> {
        paginate crate::support::OffsetController {
            offset: bind ep.offset,
        }
    }
}

api! {
    client UnknownControllerField {
        base "https://example.test";
    }

    GET read(offset: u64, limit: std::num::NonZeroU32) -> Json<Vec<u8>> {
        paginate crate::support::OffsetController {
            offset: bind ep.offset,
            page_size: bind ep.limit,
            unknown: 0,
        }
    }
}

api! {
    client NonControllerType {
        base "https://example.test";
    }

    GET read(value: u64) -> Json<Vec<u8>> {
        paginate crate::NotController {
            value: bind ep.value,
        }
    }
}

api! {
    client InaccessibleControllerField {
        base "https://example.test";
    }

    GET read(state: u64) -> Json<Vec<u8>> {
        paginate crate::private_controller::Controller {
            state: bind ep.state,
        }
    }
}

fn main() {}
