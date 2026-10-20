// §25.2: v2's implicit `host` dimension was replaced by the normalized logical
// `origin`; a bare dimension name is a retired implicit label and gets a
// migration diagnostic (labels are explicit and state-affecting in v3).
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client RawHostFormalApi {
        base "https://example.test";
        rate_limits {
            policy shared(r#host) {
                id "concord.rate_limit_raw_host_formal.1";
                key [r#host];
                allow 1 per 1s;
            }
        }
    }
}

api! {
    client InlineHostApi {
        base "https://example.test";
        rate_limit {
            id "concord.rate_limit_raw_host_formal.2";
            key [origin, host];
            allow 1 per 1s;
        }
    }
}

fn main() {}
