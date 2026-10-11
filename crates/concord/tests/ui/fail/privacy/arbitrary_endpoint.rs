// V3-L12 / §7.3: `client.request` accepts only this client's generated
// intents (`GeneratedEndpoint`), never an arbitrary value.
use concord::{Empty, api};

api! {
    client TypedApi {
        timeout 30s;
        base "https://typed.example.test";

        scope health() {
            path "health";

            GET ping() -> () via Empty;
        }
    }
}

fn main() {
    let client = TypedApi::new().unwrap();
    let _ = client.request(42_u32);
}
