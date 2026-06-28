use concord_core::advanced::NdJson;
use concord_macros::api;

pub struct RecordMapped;

api! {
    client RecordResponseMapApi {
        base "https://example.com"
    }

    GET Download
        path ["download"]
        -> Records<String, NdJson>
        map RecordMapped {
            RecordMapped
        }
}

fn main() {}
