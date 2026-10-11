// V3-L12: an intent generated for one client cannot be bound to a different
// client type, even when the endpoint shapes are identical.
use concord::{Binary, Bytes, api};

api! {
    client FirstApi {
        timeout 30s;
        base "http://first.example.test/";

        GET read() -> Bytes via Binary;
    }
}

api! {
    client SecondApi {
        timeout 30s;
        base "http://second.example.test/";

        GET read() -> Bytes via Binary;
    }
}

fn main() {
    let client = SecondApi::new().unwrap();
    let _ = client.request(FirstApi::unbound().read());
    let _ = client.plan(&FirstApi::unbound().read());
}
