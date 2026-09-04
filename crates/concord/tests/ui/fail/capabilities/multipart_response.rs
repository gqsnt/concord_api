use concord::api;

api! {
    client MultipartApi {
        timeout 30s;
        base "http://multipart.example.test";


    scope uploads() {
        path ["uploads"];
        GET invalid() -> () via Multipart;
    }
}}

fn main() {}
