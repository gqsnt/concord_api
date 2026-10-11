// LANG-02 / §3.2 endpoint_clause: unknown endpoint clauses are rejected.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client UnsupportedEndpointClauseApi {
        base "https://unsupported.example.test";

        GET item() -> Bytes via Binary {
            unsupported;
        }
    }
}

fn main() {}
