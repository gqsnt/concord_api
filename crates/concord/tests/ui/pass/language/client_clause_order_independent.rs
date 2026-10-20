// LANG-02 / §3.3: client preamble clauses of different families may appear in
// any order (here a use of `tenant` precedes its `values` declaration), and an
// owner with zero endpoints is a valid runtime-binding context whose value is
// live through the owner's own query program (VALUE-06).
use concord::api;

api! {
    client VarsOrderApi {
        timeout 30s;
        query { "tenant" = tenant; }
        base "https://example.test";
        credentials {}
        values {
            tenant: String;
        }
    }
}

fn main() {
    let _ = VarsOrderApi::builder().set_tenant("acme").build();
}
