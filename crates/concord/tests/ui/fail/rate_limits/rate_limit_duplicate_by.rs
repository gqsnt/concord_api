use concord::api;

api! {
    client Api {
        timeout off;
        base "https://example.test";
        rate_limit {
            key [host];
            key [endpoint];
            allow 1 per 1s;
        }
    }
}

fn main() {}
