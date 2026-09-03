use concord::api;

api! {
    client InvalidNoContentRequestApi {
        timeout off;
        base "http://invalid-no-content.example.test/";


    POST invalid(body body: () using NoContent) -> Bytes using Bytes;
}}

fn main() {}
