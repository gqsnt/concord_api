// CALL-01 / §7.4: v3 terminals live on the bound request, so `accepted_bytes`
// is an ordinary endpoint name on the client and scope handles (this replaces
// the v2 client-method reservation; the v3 namespace rules are
// fail/names/request_control_names_reserved.rs). The endpoint's own request
// still has the explicitly exposed `accepted_bytes()` terminal.
use concord::{Binary, Bytes, api};

api! {
    client AcceptedBytesNameApi {
        base "http://example.test";

        GET accepted_bytes() -> Bytes via Binary {
            expose accepted_bytes;
        }
    }
}

fn main() {
    let client = AcceptedBytesNameApi::new().expect("client");
    let _: accepted_bytes_name_api::AcceptedBytesRequest = client.accepted_bytes();
    let _: concord::endpoint::AcceptedBytesRequest = client.accepted_bytes().accepted_bytes();
    let _: accepted_bytes_name_api::AcceptedBytesEndpoint = AcceptedBytesNameApi::unbound().accepted_bytes();
}
