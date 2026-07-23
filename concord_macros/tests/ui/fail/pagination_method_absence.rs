use concord_macros::api;

#[path = "../../support/mod.rs"]
mod support;
#[allow(unused_imports)]
use support::Json;

api! {
    client MethodSurface {
        base "https://example.test";
    }

    GET Ordinary() -> Json<Vec<u8>>;
    GET Paginated(offset: u64, limit: std::num::NonZeroU32) -> Json<Vec<u8>> {
        paginate crate::support::OffsetController {
            offset: bind ep.offset,
            page_size: bind ep.limit,
        }
    }
}

fn main() {
    use method_surface::prelude::*;

    let client = MethodSurface::new().unwrap();
    let _ = client.ordinary().paginate(1_u32);
    let _ = client
        .paginated(0, std::num::NonZeroU32::MIN)
        .paginate(1_u32)
        .collect::<Vec<_>>();
}
