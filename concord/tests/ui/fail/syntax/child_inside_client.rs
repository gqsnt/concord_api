use concord::api;

api! {
    client NestedChildApi {
        base "https://example.test";

        scope users() {
            GET list() -> Bytes;
        }
    }
}

fn main() {}
