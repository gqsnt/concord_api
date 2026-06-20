use concord_macros::api;

api! {
    client ReservedScopeApi { base "https://example.com" }

    scope configure {
        path ["configure"]

        GET Ping
            path ["ping"]
            -> Json<String>
    }
}

fn main() {}
