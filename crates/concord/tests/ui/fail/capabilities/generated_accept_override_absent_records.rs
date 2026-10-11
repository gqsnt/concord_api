// §9.2 / §7.4: the Accept header is owned by the selected response
// representation (HEAD-01); generated requests have no per-call `accept`
// override.
use concord::{Text, api};

api! {
    client GeneratedAcceptApi {
        timeout 30s;
        base "https://example.test";

        GET read() -> String via Text;
    }
}

fn main() {
    let client = GeneratedAcceptApi::new().unwrap();
    let _ = client.read().accept("application/json");
}
