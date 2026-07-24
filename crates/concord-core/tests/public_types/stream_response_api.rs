use std::fmt::Debug;

use concord_core::contracts::{
    ApiError, ApiResponse, ByteStream, Bytes, ExecutionMeta, HeaderMap, StatusCode, Url, Version,
};
use futures_core::Stream;

fn assert_stream_contract<S>()
where
    S: Stream<Item = Result<Bytes, ApiError>> + Send + Unpin + Debug + 'static,
{
}

fn accessors(response: &ApiResponse<ByteStream>) -> &ExecutionMeta {
    let _: &Url = response.url();
    let _: StatusCode = response.status();
    let _: Version = response.version();
    let _: &HeaderMap = response.headers();
    let _: &ByteStream = response.value();
    response.metadata()
}

#[test]
fn downstream_byte_stream_has_media_independent_stream_and_universal_metadata_surfaces() {
    assert_stream_contract::<ByteStream>();
    let _ = accessors as fn(&ApiResponse<ByteStream>) -> &ExecutionMeta;
}
