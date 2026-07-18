use std::fmt::Debug;

use concord_core::advanced::{
    ApiError, Bytes, ExecutionMeta, HeaderMap, MediaType, StatusCode, Url, Version,
};
use concord_core::prelude::StreamResponse;
use futures_core::Stream;

struct DownstreamMedia;

impl MediaType for DownstreamMedia {
    const MEDIA_TYPE: &'static str = "application/vnd.downstream.stream";
}

fn assert_stream_contract<S>()
where
    S: Stream<Item = Result<Bytes, ApiError>> + Send + Unpin + Debug + 'static,
{
}

fn accessors(response: &StreamResponse<DownstreamMedia>) -> &ExecutionMeta {
    let _: &Url = response.url();
    let _: StatusCode = response.status();
    let _: Version = response.version();
    let _: &HeaderMap = response.headers();
    let _: Option<u64> = response.content_length();
    response.metadata()
}

#[test]
fn downstream_stream_response_has_the_exact_public_stream_and_metadata_surface() {
    assert_stream_contract::<StreamResponse<DownstreamMedia>>();
    let _ = accessors as fn(&StreamResponse<DownstreamMedia>) -> &ExecutionMeta;
}
