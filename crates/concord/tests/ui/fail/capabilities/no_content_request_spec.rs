use concord::api;

api! {
    client InvalidNoContentRequestApi {
        timeout 30s;
        base "http://invalid-no-content.example.test/";


    POST invalid(body body: () via Empty) -> Bytes via Binary;
}}

fn main() {}
