// §5.2 / §16.2: a runtime endpoint cannot select DNS labels; the fixed base
// origin is not reachable from the runtime builder.
use concord::endpoint::{EndpointId, RuntimeEndpoint};
use concord::http::Method;
use concord::{Empty, api};

api! {
    client FixedOriginApi {
        timeout 30s;
        base "https://fixed.example.test";

        GET ordinary() -> () via Empty;
    }
}

fn main() {
    // RUNTIME-01 / ORIGIN-01: a runtime endpoint can never select a host
    // label or another origin; only the owner projection supplies them.
    let _ = RuntimeEndpoint::builder(
        EndpointId::new("runtime.origin.escape").unwrap(),
        Method::GET,
    )
    .response::<Empty, ()>()
    .subdomain("attacker-controlled");
}
