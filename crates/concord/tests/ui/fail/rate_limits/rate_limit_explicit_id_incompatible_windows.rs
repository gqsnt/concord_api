// §12.1: one policy ID has one window schema within the closed declaration.
// Windows compare canonically (by duration, then capacity), so merely
// reordering identical windows is accepted (pass/language/explicit_rate_limit_ids.rs);
// a different capacity is a conflicting schema.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client Api {
        base "https://example.test";

        scope first() {
            rate_limit {
                id "service.shared";
                key [origin];
                allow 10 per 1s;
                allow 100 per 1m;
            }
            GET read() -> Bytes via Binary;
        }

        scope second() {
            rate_limit {
                id "service.shared";
                key [origin];
                allow 100 per 1m;
                allow 20 per 1s;
            }
            GET write() -> Bytes via Binary;
        }
    }
}

fn main() {}
