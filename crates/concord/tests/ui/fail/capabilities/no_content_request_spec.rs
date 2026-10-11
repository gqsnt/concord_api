// §9.5: `Empty` is a response-only representation; it is not a request body
// representation for `()`.
use concord::{Binary, Bytes, Empty, api};

api! {
    client InvalidNoContentRequestApi {
        timeout 30s;
        base "http://invalid-no-content.example.test/";

        POST invalid(body body: () via Empty) -> Bytes via Binary;
    }
}

fn main() {}
