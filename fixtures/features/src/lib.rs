#![forbid(unsafe_code)]

use runtime::{Bytes, api};

api! {
    client EmptyGeneratedApi {
        timeout 30s;
        base "http://no-local-limiter-empty.example.test/";


    GET read() -> Bytes via Binary {
        path ["read"];
    }
}}

api! {
    client LimitedGeneratedApi {
        timeout 30s;
        base "http://no-local-limiter-limited.example.test/";
        rate_limits {
            policy fixture_limit() {
                id "fixture.generated.limit";
                key [origin];
                cost 2;
                allow 3 per 1m;
                allow 30 per 10m;
            }
        }


    GET read() -> Bytes via Binary {
        path ["read"];
        rate_limit fixture_limit();
    }
}}
