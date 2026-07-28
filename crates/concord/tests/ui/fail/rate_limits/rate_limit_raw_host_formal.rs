use concord::prelude::*;

api! {
    client RawHostFormalApi {
        base "https://example.test";
        rate_limits {
            rule shared {
                by [r#host];
                1 / 1s;
            }
        }
    }

    GET read() -> Bytes {
        rate_limit use shared;
    }
}

fn main() {}
