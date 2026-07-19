use concord_core::advanced::{Method, PreparedEndpoint};
use concord_macros::api;

api! {
    client PreparedPaginationApi {
        base "https://example.test";
        GET Pages(offset: u64, limit: u32) -> Json<Vec<u8>> {
            paginate OffsetPagination {
                offset = ep.offset,
                page_size = ep.limit
            }
        }
    }
}

fn main() {
    use prepared_pagination_api::prelude::*;

    let client = PreparedPaginationApi::new().unwrap();
    let endpoint = PreparedEndpoint::builder("Prepared::pages", Method::GET)
        .response::<concord_core::advanced::Json<Vec<u8>>>()
        .build()
        .unwrap();
    let _ = client.request(endpoint).paginate(1_u32);
}
