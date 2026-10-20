// IDENTITY-01: explicit operational endpoint IDs are unique within a client.
use concord::{Binary, Bytes, api};

api! {
    client DuplicateIdApi {
        base "http://identity.example.test/";

        GET first() -> Bytes via Binary {
            id "identity.same";
        }
        GET second() -> Bytes via Binary {
            id "identity.same";
        }
    }
}

// A navigation-derived ID collides with an explicit pin as well.
api! {
    client DerivedCollisionApi {
        base "http://identity.example.test/";

        scope users() {
            GET list() -> Bytes via Binary;
        }
        GET pinned() -> Bytes via Binary {
            id "derived_collision_api.users.list";
        }
    }
}

fn main() {}
