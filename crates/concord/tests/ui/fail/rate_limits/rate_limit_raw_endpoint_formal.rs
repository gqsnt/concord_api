use concord::prelude::*;

api! {
    client RawEndpointFormalApi {
        timeout 30s;
        base "https://example.test";
        rate_limits {
            policy shared(r#endpoint) { id "concord.rate_limit_raw_endpoint_formal.1"; cost 1;
                key [r#endpoint];
                allow 1 per 1s;
            }
        }


    GET read() -> Bytes via Binary {
        rate_limit shared();
    }
}}

fn main() {}
