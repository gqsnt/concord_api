use runtime::api;

api! {
    client JsonDisabledApi {
        base "http://json-disabled.example.test";
    }

    scope records() {
        path ["records"];
        POST create(#[body] body: Json<Vec<u32>>) -> Json<Vec<u32>>;
    }
}
