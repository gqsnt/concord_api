#[cfg(feature = "fixture-absence")]
use runtime::api;

#[cfg(feature = "fixture-absence")]
api! {
    client JsonAbsentApi {
        base "http://json-absent.example.test";
    }

    POST create(
        #[body] body: Json<Vec<u32>>,
    ) -> Json<Vec<u32>>;
}

fn main() {}
