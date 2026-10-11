// §7.1: `raw_response` is an ordinary endpoint name in v3; the accepted-bytes
// terminal is only `accepted_bytes()` and only with `expose accepted_bytes`.
use concord::{Binary, Bytes, api};

api! {
    client NamingApi {
        timeout 30s;
        base "http://example.test";

        GET raw_response() -> Bytes via Binary {
            expose accepted_bytes;
        }
    }
}

fn main() {
    let client = NamingApi::new().expect("client");
    let _ = client.raw_response().accepted_bytes();
}
