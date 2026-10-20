// §10.1: a custom method is a bounded HTTP token; CONNECT/TRACE are rejected
// case-insensitively, including through `METHOD(...)`.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client MethodApi {
        base "https://method.example.test";

        METHOD("BAD METHOD") item() -> Bytes via Binary;
    }
}

api! {
    client ConnectApi {
        base "https://method.example.test";

        METHOD("connect") tunnel() -> Bytes via Binary;
    }
}

api! {
    client TraceApi {
        base "https://method.example.test";

        TRACE trace_route() -> Bytes via Binary;
    }
}

fn main() {}
