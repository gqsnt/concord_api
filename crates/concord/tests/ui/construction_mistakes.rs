// Seeded construction mistakes (M11): each is reported at the declaration that caused it.
concord::api! {
    pub client MissingBase(token: String) {
        base instance;
        headers { "x" = "{token}"; }
        GET a();
    }
}

concord::api! {
    pub client OptionalBase(instance?: concord::BaseUrl) {
        base instance;
        GET a();
    }
}

concord::api! {
    pub client MutRequest {
        base "https://x.test";
        GET a(query mut page: u32);
    }
}

concord::api! {
    pub client MutOptional(#[sensitive] header("authorization") mut token?: String) {
        base "https://x.test";
        GET a();
    }
}

concord::api! {
    pub client GetterCollision {
        base "https://x.test";
        scope s(query items: u32) {
            GET items();
        }
    }
}

fn main() {}
