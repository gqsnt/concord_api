use concord::api;

api! {
    client ReferenceApi {
        timeout 30s;
        base "https://reference.example.test";


    GET item() -> Bytes via Binary {
        path [endpoint.missing];
    }
}}

fn main() {}
