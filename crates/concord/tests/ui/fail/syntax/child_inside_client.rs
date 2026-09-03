use concord::api;

api! {
    client NestedChildApi {
        timeout off;
        base "https://example.test";
    }

    scope users() {
        GET list() -> Bytes using Bytes;
    }
}

fn main() {}
