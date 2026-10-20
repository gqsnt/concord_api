// CALL-01 (replaces the v2 `accepted_bytes` client-method reservation: v3
// terminals live on the request, so an endpoint may be named
// `accepted_bytes`; see pass/language/accepted_bytes_endpoint_name.rs).
//
// Value controls share the request namespace with the core controls and
// terminals, and scope value controls share the scope handle namespace with
// its navigation methods.
use concord::{Binary, Bytes, api};

// An initialized request-local `timeout` generates `reset_timeout`, the core
// timeout-reset control.
api! {
    client TimeoutValueApi {
        base "https://example.test";

        GET read(query timeout: u32 = 5) -> Bytes via Binary;
    }
}

// An endpoint `set_locale` under a scope with a configurable `locale`
// collides with the scope's generated value control.
api! {
    client ScopeControlApi {
        base "https://example.test";

        scope regional(query locale?: String) {
            GET set_locale() -> Bytes via Binary;
        }
    }
}

fn main() {}
