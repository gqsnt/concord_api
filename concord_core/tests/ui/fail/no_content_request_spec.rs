use concord_macros::api;

api! {
    client InvalidNoContentRequestApi {
        base "https://example.test";
        POST Invalid(body: NoContent) -> Bytes;
    }
}

fn main() {}
