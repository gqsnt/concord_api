use concord::api;

api! {
    client Api {
        timeout off;
        base "https://example.test";
        rate_limit {
            allow 1 per 1s;
            key [host];
        }
    }
}

fn main() {}
