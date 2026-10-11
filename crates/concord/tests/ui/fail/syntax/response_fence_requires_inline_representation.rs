// §9.1: every result needs an effective response representation; the
// diagnostic names the `response explicit` fence that removed the default.
#![allow(unused_imports)]

use concord::api;

api! {
    client ResponseFenceApi {
        base "https://fence.example.test/";
        response concord::Text;

        scope fenced() {
            response explicit;
            GET missing() -> String;
        }
    }
}

// No response default anywhere: no codec is guessed from the output type.
api! {
    client NoResponseDefaultApi {
        base "https://fence.example.test/";

        GET missing() -> String;
    }
}

fn main() {}
