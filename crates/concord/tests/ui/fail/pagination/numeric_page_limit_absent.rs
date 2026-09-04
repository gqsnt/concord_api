use std::num::NonZeroU32;

use concord::api;

#[path = "../../../support/mod.rs"]
mod support;

api! {
    client ExplicitPageLimitApi {
        timeout 30s;
        base "https://example.test";

        GET read(offset: u64, limit: NonZeroU32) -> Vec<u8> via crate::support::Json {
            paginate via crate::support::OffsetController {
                offset -> endpoint.offset;
                page_size -> endpoint.limit;
            }
        }
    }
}

fn main() {
    let client = ExplicitPageLimitApi::new().unwrap();
    let _ = client
        .read(0, NonZeroU32::new(10).unwrap())
        .paginate(10_u32);
}
