use concord::api;

api! {
    client Api {
        timeout off;
        base "https://example.test";
        rate_limit { key [other.tenant]; allow 1 per 1s; }
    }
}

fn main() {}
