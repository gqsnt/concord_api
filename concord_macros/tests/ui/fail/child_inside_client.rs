use concord_macros::api;

api! {
    client NestedChildApi {
        base "https://example.test";

        scope Users() {
            GET List() -> Bytes;
        }
    }
}

fn main() {}
