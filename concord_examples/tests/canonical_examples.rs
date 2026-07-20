#![forbid(unsafe_code)]

use concord_core::advanced::{Method, PreparedEndpoint};
use concord_core::prelude::{Bytes, PageLimit, StreamBody};
use concord_core::{PaginatedRequest, Request};
use concord_macros::api;

api! {
    client DeterministicApi {
        base "https://deterministic.example.test/";
        POST Custom(body: concord_examples::ExampleCodec) -> concord_examples::ExampleCodec;
        POST Upload(body: Stream<OctetStream>) -> Bytes;
        GET Pages(offset: u64, limit: std::num::NonZeroU32) -> Json<Vec<u32>> {
            paginate concord_core::advanced::OffsetPagination {
                offset: bind ep.offset,
                page_size: bind ep.limit,
            }
        }
    }
}

#[test]
fn canonical_construction_is_deterministic_and_performs_no_transport() {
    use deterministic_api::prelude::*;

    let client = DeterministicApi::builder()
        .build()
        .expect("deterministic example client");
    let _: Request<_> = client.custom("payload".to_owned());
    let _: Request<_> = client.upload(StreamBody::from_bytes(Bytes::from_static(b"stream")));
    let _: PaginatedRequest<_> = client
        .pages(
            0,
            std::num::NonZeroU32::new(25).expect("positive page size"),
        )
        .paginate(PageLimit::Take(2));
    let prepared = PreparedEndpoint::builder("DeterministicApi::prepared", Method::GET)
        .path_segment("record")
        .response_bytes()
        .build()
        .expect("deterministic prepared endpoint");
    let _: Request<_> = client.request(prepared);
}
