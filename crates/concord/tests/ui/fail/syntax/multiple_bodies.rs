// LANG-02: one body per endpoint; a body is endpoint-only.
#![allow(unused_imports)]

use concord::{Binary, Bytes, Text, api};

api! {
    client MultipleBodiesApi {
        base "http://body.example.test/";

        POST submit(body first: String via Text, body second: Bytes via Binary) -> Bytes via Binary;
    }
}

api! {
    client ScopeBodyApi {
        base "http://body.example.test/";

        scope uploads(body payload: Bytes via Binary) {
            POST submit() -> Bytes via Binary;
        }
    }
}

api! {
    client RootBodyApi {
        base "http://body.example.test/";
        values { body payload: Bytes; }
    }
}

fn main() {}
