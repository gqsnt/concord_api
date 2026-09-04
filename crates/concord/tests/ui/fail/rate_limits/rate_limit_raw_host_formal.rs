use concord::prelude::*;

api! {
    client RawHostFormalApi {
        timeout 30s;
        base "https://example.test";
        rate_limits {
            policy shared(r#host) { id "concord.rate_limit_raw_host_formal.1"; cost 1;
                key [r#host];
                allow 1 per 1s;
            }
        }


    GET read() -> Bytes via Binary {
        rate_limit shared();
    }
}}

fn main() {}
