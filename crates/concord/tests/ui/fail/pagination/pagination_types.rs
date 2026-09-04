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
        type Item = u8;

        const DESCRIPTION: concord::pagination::PaginationDescription =
            concord::pagination::PaginationDescription::NONE;

        fn advance(
            self,
            _page: concord::pagination::PageResponse<Vec<u8>>,
            _limits: concord::pagination::PageDecisionLimits,
        ) -> Result<
            concord::pagination::PageStep<Self, Self::Item>,
            concord::pagination::PaginationError,
        > {
            Ok(concord::pagination::PageStep::Done { items: Vec::new() })
        }
    }
}

api! {
    client UndefinedField {
        timeout 30s;
        base "https://example.test";


    GET read(offset: u64, limit: std::num::NonZeroU32) -> Vec<u8> via crate::support::Json {
        paginate via crate::support::OffsetController {
            offset -> endpoint.missing;
            page_size -> endpoint.limit;
        }
    }
}}

api! {
    client WrongOffsetType {
        timeout 30s;
        base "https://example.test";


    GET read(offset: String, limit: std::num::NonZeroU32) -> Vec<u8> via crate::support::Json {
        paginate via crate::support::OffsetController {
            offset -> endpoint.offset;
            page_size -> endpoint.limit;
        }
    }
}}

api! {
    client WrongCursorType {
        timeout 30s;
        base "https://example.test";


    GET read(cursor: String) -> Vec<u8> via crate::support::Json {
        paginate via crate::support::CursorController<String> {
            cursor -> endpoint.cursor;
        }
    }
}}

api! {
    client MissingControllerField {
        timeout 30s;
        base "https://example.test";


    GET read(offset: u64) -> Vec<u8> via crate::support::Json {
        paginate via crate::support::OffsetController {
            offset -> endpoint.offset;
        }
    }
}}

api! {
    client UnknownControllerField {
        timeout 30s;
        base "https://example.test";


    GET read(offset: u64, limit: std::num::NonZeroU32) -> Vec<u8> via crate::support::Json {
        paginate via crate::support::OffsetController {
            offset -> endpoint.offset;
            page_size -> endpoint.limit;
            unknown = 0;
        }
    }
}}

api! {
    client NonControllerType {
        timeout 30s;
        base "https://example.test";


    GET read(value: u64) -> Vec<u8> via crate::support::Json {
        paginate via crate::NotController {
            value -> endpoint.value;
        }
    }
}}

api! {
    client InaccessibleControllerField {
        timeout 30s;
        base "https://example.test";


    GET read(state: u64) -> Vec<u8> via crate::support::Json {
        paginate via crate::private_controller::Controller {
            state -> endpoint.state;
        }
    }
}}

fn main() {}
