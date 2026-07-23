#![forbid(unsafe_code)]

use concord::api;
use concord::endpoint::{EndpointId, RuntimeEndpoint};
use concord::http::Method;
use concord::prelude::{Bytes, PageLimit, StreamBody};
use concord::{PaginatedRequest, Request};

api! {
    client DeterministicApi {
        base "https://deterministic.example.test/";
    }

    POST custom(#[body] body: concord_examples::ExampleCodec) -> concord_examples::ExampleCodec;
    POST upload(#[body] body: Stream<OctetStream>) -> Bytes;
    GET pages(offset: u64, limit: std::num::NonZeroU32) -> Json<Vec<u32>> {
        paginate concord::pagination::OffsetPagination {
            offset: bind ep.offset,
            page_size: bind ep.limit,
        }
    }
}

#[test]
fn canonical_construction_is_deterministic_and_performs_no_transport() {
    let client = DeterministicApi::builder()
        .build()
        .expect("deterministic example client");
    let _: deterministic_api::CustomRequest = client.custom("payload".to_owned());
    let _: deterministic_api::UploadRequest =
        client.upload(StreamBody::from_bytes(Bytes::from_static(b"stream")));
    let _: PaginatedRequest<_> = client
        .pages(
            0,
            std::num::NonZeroU32::new(25).expect("positive page size"),
        )
        .paginate(PageLimit::Take(2));
    let runtime = RuntimeEndpoint::builder(
        EndpointId::new("deterministic_api.runtime").expect("bounded runtime endpoint ID"),
        Method::GET,
    )
    .path_segment("record")
    .response_bytes()
    .build()
    .expect("deterministic runtime endpoint");
    let _: Request<_> = client.request(runtime);
}
