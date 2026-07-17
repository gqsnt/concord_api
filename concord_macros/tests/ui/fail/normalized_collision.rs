use concord_macros::api;

api! {
    client CollisionApi {
        base "https://collision.example.test";
        scope health() {
            path ["health"];
            GET HTTPStatus() -> NoContent;
            GET HttpStatus() -> NoContent;
        }
    }
}

fn main() {}
