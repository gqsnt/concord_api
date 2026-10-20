// §9.1: a scope's `request explicit` fence removes the inherited request
// default; a body below it needs a new default or an inline `via`.
#![allow(unused_imports)]

use concord::api;

api! {
    client RequestFenceApi {
        base "https://fence.example.test/";
        request concord::Text;
        response concord::Text;

        scope fenced() {
            request explicit;
            POST missing(body input: String) -> String;
        }
    }
}

fn main() {}
