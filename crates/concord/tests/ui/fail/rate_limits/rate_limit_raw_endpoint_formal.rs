// §12.3: `origin` and `endpoint` are reserved built-in labels, including when
// the formal is spelled with a raw identifier.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client RawEndpointFormalApi {
        base "https://example.test";
        values { query value: String; }
        rate_limits {
            policy shared(r#endpoint) {
                id "concord.rate_limit_raw_endpoint_formal.1";
                key ["endpoint" = r#endpoint];
                allow 1 per 1s;
            }
        }

        GET read() -> Bytes via Binary {
            rate_limit shared(r#endpoint = client.value);
        }
    }
}

api! {
    client OriginLabelApi {
        base "https://example.test";
        values { query value: String; }
        rate_limit {
            id "concord.rate_limit_raw_endpoint_formal.2";
            key ["origin" = value];
            allow 1 per 1s;
        }
    }
}

fn main() {}
