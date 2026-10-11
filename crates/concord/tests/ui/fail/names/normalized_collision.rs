// CALL-01: child names are unique within an owner, and distinct source
// identifiers that normalize to the same generated Rust symbol are rejected
// before emission instead of being suffixed.
use concord::{Empty, api};

api! {
    client CollisionApi {
        base "https://collision.example.test";

        scope health() {
            path "health";
            GET http_status() -> () via Empty;
            GET http_status() -> () via Empty;
        }
    }
}

api! {
    client NormalizedApi {
        base "https://collision.example.test";

        GET item_list() -> () via Empty;
        GET item__list() -> () via Empty {
            id "normalized.other";
        }
    }
}

fn main() {}
