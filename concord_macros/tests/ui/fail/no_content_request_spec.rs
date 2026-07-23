use concord_macros::api;

api! {
    client InvalidNoContentRequestApi {
        base "http://invalid-no-content.example.test/";
    }

    POST invalid(#[body] body: NoContent) -> Bytes;
}

fn main() {}
