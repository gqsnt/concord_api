use concord::api;

api! {
    client HeaderApi {
        timeout off;
        base "https://header.example.test";


    GET item(values: Vec<String>) -> Bytes using Bytes {
        headers { "X-Values" = endpoint.values; }
    }
}}

fn main() {}
