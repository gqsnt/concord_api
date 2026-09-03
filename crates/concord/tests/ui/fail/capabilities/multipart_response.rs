use concord::api;

api! {
    client MultipartApi {
        timeout off;
        base "http://multipart.example.test";


    scope uploads() {
        path ["uploads"];
        GET invalid() -> () using Multipart;
    }
}}

fn main() {}
