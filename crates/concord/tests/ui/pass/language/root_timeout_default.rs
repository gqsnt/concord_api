// §2.1 / §3.3 / MIGRATE-04: `timeout` is optional in v3 (v2.1 required it on
// every client). Without any owner clause the visible root default of 30
// seconds applies and is recorded as `RootDefault` in the contract; the
// nearest owner clause still selects a different finite timeout.
use std::time::Duration;

use concord::inspection::TimeoutSource;
use concord::{Binary, Bytes, api};

api! {
    client RootTimeoutApi {
        base "http://example.test";

        GET read() -> Bytes via Binary;
        GET quick() -> Bytes via Binary {
            timeout 2s;
        }
    }
}

fn main() {
    let contract = RootTimeoutApi::contract();
    assert_eq!(contract.root_timeout().duration(), Duration::from_secs(30));
    assert_eq!(contract.root_timeout().source(), TimeoutSource::RootDefault);

    let read = contract.endpoint("root_timeout_api.read").expect("navigation-derived ID");
    assert_eq!(read.timeout().duration(), Duration::from_secs(30));
    assert_eq!(read.timeout().source(), TimeoutSource::RootDefault);

    let quick = contract.endpoint("root_timeout_api.quick").expect("navigation-derived ID");
    assert_eq!(quick.timeout().duration(), Duration::from_secs(2));

    let client = RootTimeoutApi::new().expect("client");
    assert_eq!(client.read().control_summary().declared_timeout(), Some(Duration::from_secs(30)));
}
