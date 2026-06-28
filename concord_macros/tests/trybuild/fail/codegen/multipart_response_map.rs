use concord_macros::api;
use self::multipart_response_map_api::MultipartResponseMapApi;

pub struct MultipartMapped;

api! {
    client MultipartResponseMapApi {
        base "https://example.com"
    }

    GET Download
        path ["download"]
        -> Multipart<concord_core::advanced::RawResponsePart>
        map MultipartMapped {
            MultipartMapped
        }
}

fn main() {}
