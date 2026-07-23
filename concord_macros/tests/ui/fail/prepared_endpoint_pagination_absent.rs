use concord_core::advanced::{Method, PreparedEndpoint};
use concord_macros::api;

#[path = "../../support/mod.rs"]
mod support;
#[allow(unused_imports)]
use support::Json;

api! {
    client PreparedPaginationApi {
        base "https://example.test";
    }

    GET Pages(offset: u64, limit: std::num::NonZeroU32) -> Json<Vec<u8>> {
        paginate crate::support::OffsetController {
            offset: bind ep.offset,
            page_size: bind ep.limit,
        }
    }
}

fn main() {
    use prepared_pagination_api::prelude::*;

    let client = PreparedPaginationApi::new().unwrap();
    let endpoint = PreparedEndpoint::builder("Prepared::pages", Method::GET)
        .response::<crate::support::Json<Vec<u8>>>()
        .build()
        .unwrap();
    let _ = client.request(endpoint).paginate(1_u32);
}
