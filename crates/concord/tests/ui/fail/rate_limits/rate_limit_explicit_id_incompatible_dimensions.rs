// §12.1 / RATE-03: one policy ID has one key schema within the closed
// declaration; ordered labels are part of that schema.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client Api {
        base "https://example.test";

        scope first() {
            rate_limit {
                id "service.shared";
                key [origin, endpoint];
                allow 10 per 1s;
            }
            GET read() -> Bytes via Binary { id "service.first.read"; }
        }

        scope second() {
            rate_limit {
                id "service.shared";
                key [endpoint, origin];
                allow 10 per 1s;
            }
            GET write() -> Bytes via Binary { id "service.second.write"; }
        }
    }
}

fn main() {}
