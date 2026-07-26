use concord::api;

api! {
    client Api {
        base "https://example.test";
        rate_limit { by [other.tenant]; 1 / 1s; }
    }
}

fn main() {}
