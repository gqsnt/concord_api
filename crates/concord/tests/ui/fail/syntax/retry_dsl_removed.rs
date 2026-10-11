// §25.2 / §2 summary: the retry DSL clause is retired; install a managed
// `RetryPolicy` through the runtime configuration.
#![allow(unused_imports)]

use concord::api;

api! {
    client RemovedRetryApi {
        base "https://example.test/";
        retry reqwest_default;
    }
}

fn main() {}
