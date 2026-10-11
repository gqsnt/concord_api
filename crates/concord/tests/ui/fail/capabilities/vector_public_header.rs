// §6.2 / VALUE-04: a sequence is not a scalar atom; repeating a header needs
// an explicit `each` emission (there is no implicit Vec-to-atom conversion).
use concord::{Binary, Bytes, api};

api! {
    client HeaderApi {
        timeout 30s;
        base "https://header.example.test";

        GET item(values: Vec<String>) -> Bytes via Binary {
            headers { "X-Values" = values; }
        }
    }
}

fn main() {}
