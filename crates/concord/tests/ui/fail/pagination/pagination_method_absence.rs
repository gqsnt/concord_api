use concord::api;

#[path = "../../../support/mod.rs"]
mod support;
#[allow(unused_imports)]
use support::Json;

api! {
    client MethodSurface {
        base "https://example.test";
    }

    GET ordinary() -> crate::support::Json<Vec<u8>>;
    GET paginated(
        offset: u64,
        limit: std::num::NonZeroU32,
    ) -> crate::support::Json<Vec<u8>> {
        paginate crate::support::OffsetController {
            offset: bind ep.offset,
            page_size: bind ep.limit,
        }
    }
}

fn main() {
    let client = MethodSurface::new().unwrap();
    let _ = client.ordinary().paginate(1_u32);
    let _ = client
        .paginated(0, std::num::NonZeroU32::MIN)
        .paginate(1_u32)
        .collect::<Vec<_>>();
}
