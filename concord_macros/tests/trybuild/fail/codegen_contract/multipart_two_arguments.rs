use concord_macros::api;

api! {
    client MultipartTwoArgumentsApi { base "https://example.com" }
    POST Upload(body: Multipart<(), concord_core::advanced::FormData>)
        path ["upload"]
        -> concord_core::prelude::Json<Response>
}

pub struct Response;

fn main() {}
