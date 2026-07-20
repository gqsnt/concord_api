use concord_macros::api;

api! {
    client JsonDisabledApi {
        base "http://json-disabled.example.test";
        scope records() {
            path ["records"];
            POST Create(body: Json<Vec<u32>>) -> Json<Vec<u32>>;
        }
    }
}
