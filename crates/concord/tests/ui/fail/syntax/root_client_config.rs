// v2.1 required every client to declare `timeout`; v3 selects a visible
// 30-second default when it is omitted (§2.1, MIGRATE-04; see
// pass/language/root_timeout_default.rs). What remains an error is client
// configuration written outside the closed client declaration, and an
// invalid timeout literal.
#![allow(unused_imports)]

use concord::api;

api! {
    client RootConfigApi {
        base "https://example.test";
    }

    timeout 5s;
}

api! {
    client MissingBaseApi {
        timeout 5s;
    }
}

api! {
    client ZeroTimeoutApi {
        base "https://example.test";
        timeout 0s;
    }
}

api! {
    client FractionalTimeoutApi {
        base "https://example.test";
        timeout 1.5s;
    }
}

api! {
    client UnitlessTimeoutApi {
        base "https://example.test";
        timeout 30;
    }
}

api! {
    client TimeoutOffApi {
        base "https://example.test";
        timeout off;
    }
}

fn main() {}
