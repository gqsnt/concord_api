use concord_macros::api;

api! {
    client MethodSurface {
        base "https://example.test";
        GET Ordinary() -> Json<Vec<u8>>;
        GET Paginated(offset: u64, limit: u32) -> Json<Vec<u8>> {
            paginate OffsetPagination {
                offset = ep.offset,
                page_size = ep.limit
            }
        }
    }
}

fn main() {
    use method_surface::prelude::*;

    let client = MethodSurface::new().unwrap();
    let _ = client.ordinary().paginate(1_u32);
    let _ = client.paginated(0, 1).paginate(1_u32).collect::<Vec<_>>();
}
