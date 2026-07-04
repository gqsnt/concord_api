use concord_macros::api;
use self::reserved_multipart_arity_zero_api::ReservedMultipartArityZeroApi;

api! {
    client ReservedMultipartArityZeroApi {
        base "https://example.com"
    }

    POST Upload(body: Multipart<>)
        path ["upload"]
        -> concord_core::prelude::Json<()>
}

fn main() {}
