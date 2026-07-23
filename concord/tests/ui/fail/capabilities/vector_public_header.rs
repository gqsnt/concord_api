use concord::api;

api! {
    client HeaderApi {
        base "https://header.example.test";
    }

    GET item(values: Vec<String>) -> Bytes {
        headers { "X-Values" = ep.values; }
    }
}

fn main() {}
