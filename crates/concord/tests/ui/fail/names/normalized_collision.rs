use concord::api;

api! {
    client CollisionApi {
        timeout off;
        base "https://collision.example.test";


    scope health() {
        path ["health"];
        GET http_status() -> () using NoContent;
        GET http_status() -> () using NoContent;
    }
}}

fn main() {}
