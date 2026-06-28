use concord_core::advanced::OctetStream;
use concord_macros::api;

pub struct Mapped;

api! {
    client SseResponseMapApi {
        base "https://example.com"
    }

    GET Events
        path ["events"]
        -> Sse<OctetStream>
        map Mapped {
            Mapped
        }
}

fn main() {}
