use concord_core::advanced::OctetStream;
use concord_macros::api;

pub struct StreamMapped;

api! {
    client StreamResponseMapApi {
        base "https://example.com"
    }

    GET Download
        path ["download"]
        -> Stream<OctetStream>
        map StreamMapped {
            StreamMapped
        }
}

fn main() {}
