use concord::prelude::*;

api! {
    client RawEndpointFormalApi {
        base "https://example.test";
        rate_limits {
            rule shared {
                by [r#endpoint];
                1 / 1s;
            }
        }
    }

    GET read() -> Bytes {
        rate_limit use shared;
    }
}

fn main() {}
