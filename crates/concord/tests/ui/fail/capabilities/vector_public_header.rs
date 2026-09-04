use concord::api;

api! {
    client HeaderApi {
        timeout 30s;
        base "https://header.example.test";


    GET item(values: Vec<String>) -> Bytes via Binary {
        headers { "X-Values" = endpoint.values; }
    }
}}

fn main() {}
