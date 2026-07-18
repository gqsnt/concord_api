use concord_macros::api;

api! {
    client HeaderApi {
        base "https://header.example.test";
        GET Item(values: Vec<String>) -> Bytes {
            headers { "X-Values" = ep.values; }
        }
    }
}

fn main() {}
