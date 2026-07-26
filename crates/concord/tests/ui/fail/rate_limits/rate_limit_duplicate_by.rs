use concord::api;

api! {
    client Api {
        base "https://example.test";
        rate_limit {
            by [host];
            by [endpoint];
            1 / 1s;
        }
    }
}

fn main() {}
