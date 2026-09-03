#![forbid(unsafe_code)]

use runtime::{Bytes, api};

api! {
    client EmptyGeneratedApi {
        timeout off;
        base "http://no-local-limiter-empty.example.test/";


    GET read() -> Bytes using Bytes {
        path ["read"];
    }
}}

api! {
    client LimitedGeneratedApi {
        timeout off;
        base "http://no-local-limiter-limited.example.test/";
        rate_limits {
            fixture_limit() {
                cost 2;
                allow 3 per 1m;
                allow 30 per 10m;
            }
        }


    GET read() -> Bytes using Bytes {
        path ["read"];
        rate_limit fixture_limit();
    }
}}
