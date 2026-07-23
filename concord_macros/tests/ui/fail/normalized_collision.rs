use concord_macros::api;

api! {
    client CollisionApi {
        base "https://collision.example.test";
    }

    scope health() {
        path ["health"];
        GET http_status() -> NoContent;
        GET http_status() -> NoContent;
    }
}

fn main() {}
