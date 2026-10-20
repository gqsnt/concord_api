// §25.2: endpoint `accept` is retired; negotiation is the response
// representation's static `ACCEPT` constant.
#![allow(unused_imports)]

use concord::{Text, api};

api! {
    client RemovedAcceptApi {
        base "https://example.test";

        GET read() -> String via Text {
            accept "application/json";
        }
    }
}

fn main() {}
