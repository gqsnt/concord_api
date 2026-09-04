use concord::api;
use concord::endpoint::{EndpointId, RuntimeEndpoint};
use concord::http::Method;

api! {
    client FixedOriginApi {
        timeout 30s;
        base "https://fixed.example.test";

        GET ordinary() -> () via Empty;
    }
}

fn main() {
    let _ = RuntimeEndpoint::builder(
        EndpointId::new("runtime.origin.escape").unwrap(),
        Method::GET,
    )
    .subdomain("attacker-controlled")
    .build()
    .unwrap();
}
