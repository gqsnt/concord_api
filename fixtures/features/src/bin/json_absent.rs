use runtime::api;

api! {
    client JsonAbsentApi {
        base "http://json-absent.example.test";
    }

    POST create(
        #[body] body: Json<Vec<u32>>,
    ) -> Json<Vec<u32>>;
}

fn main() {}
