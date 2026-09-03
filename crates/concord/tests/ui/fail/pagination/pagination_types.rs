use concord::api;

#[path = "../../../support/mod.rs"]
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

    impl concord::pagination::PaginationController<Vec<u8>> for Controller {
        fn next(
            self,
            _page: &Vec<u8>,
        ) -> Result<Option<Self>, concord::pagination::PaginationError> {
            Ok(None)
        }
    }
}

api! {
    client UndefinedField {
        timeout off;
        base "https://example.test";


    GET read(offset: u64, limit: std::num::NonZeroU32) -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate with crate::support::OffsetController {
            offset -> endpoint.missing;
            page_size -> endpoint.limit;
        }
    }
}}

api! {
    client WrongOffsetType {
        timeout off;
        base "https://example.test";


    GET read(offset: String, limit: std::num::NonZeroU32) -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate with crate::support::OffsetController {
            offset -> endpoint.offset;
            page_size -> endpoint.limit;
        }
    }
}}

api! {
    client WrongCursorType {
        timeout off;
        base "https://example.test";


    GET read(cursor: String) -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate with crate::support::CursorController<String> {
            cursor -> endpoint.cursor;
        }
    }
}}

api! {
    client MissingControllerField {
        timeout off;
        base "https://example.test";


    GET read(offset: u64) -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate with crate::support::OffsetController {
            offset -> endpoint.offset;
        }
    }
}}

api! {
    client UnknownControllerField {
        timeout off;
        base "https://example.test";


    GET read(offset: u64, limit: std::num::NonZeroU32) -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate with crate::support::OffsetController {
            offset -> endpoint.offset;
            page_size -> endpoint.limit;
            unknown = 0;
        }
    }
}}

api! {
    client NonControllerType {
        timeout off;
        base "https://example.test";


    GET read(value: u64) -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate with crate::NotController {
            value -> endpoint.value;
        }
    }
}}

api! {
    client InaccessibleControllerField {
        timeout off;
        base "https://example.test";


    GET read(state: u64) -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate with crate::private_controller::Controller {
            state -> endpoint.state;
        }
    }
}}

fn main() {}
