use concord::prelude::*;

api! {
    client RawEndpointFormalApi {
        timeout off;
        base "https://example.test";
        rate_limits {
            shared(r#endpoint) {
                key [r#endpoint];
                allow 1 per 1s;
            }
        }


    GET read() -> Bytes using Bytes {
        rate_limit shared();
    }
}}

fn main() {}
