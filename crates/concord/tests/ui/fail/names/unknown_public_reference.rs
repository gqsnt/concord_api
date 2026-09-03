use concord::api;

api! {
    client ReferenceApi {
        timeout off;
        base "https://reference.example.test";


    GET item() -> Bytes using Bytes {
        path [endpoint.missing];
    }
}}

fn main() {}
