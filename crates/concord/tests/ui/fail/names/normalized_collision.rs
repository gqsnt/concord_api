use concord::api;

api! {
    client CollisionApi {
        timeout 30s;
        base "https://collision.example.test";


    scope health() {
        path ["health"];
        GET http_status() -> () via Empty;
        GET http_status() -> () via Empty;
    }
}}

fn main() {}
