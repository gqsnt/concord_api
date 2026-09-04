use concord::api;

api! {
    client NestedChildApi {
        timeout 30s;
        base "https://example.test";
    }

    scope users() {
        GET list() -> Bytes via Binary;
    }
}

fn main() {}
