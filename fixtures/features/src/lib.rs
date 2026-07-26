#![forbid(unsafe_code)]

use runtime::{Bytes, api};

api! {
    client EmptyGeneratedApi {
        base "http://no-local-limiter-empty.example.test/";
    }

    GET read() -> Bytes {
        path ["read"];
    }
}

api! {
    client LimitedGeneratedApi {
        base "http://no-local-limiter-limited.example.test/";
        rate_limit {
            cost 2;
            3 / 1m;
            30 / 10m;
        }
    }

    GET read() -> Bytes {
        path ["read"];
    }
}
