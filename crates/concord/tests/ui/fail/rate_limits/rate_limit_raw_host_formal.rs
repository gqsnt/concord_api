use concord::prelude::*;

api! {
    client RawHostFormalApi {
        timeout off;
        base "https://example.test";
        rate_limits {
            shared(r#host) {
                key [r#host];
                allow 1 per 1s;
            }
        }


    GET read() -> Bytes using Bytes {
        rate_limit shared();
    }
}}

fn main() {}
