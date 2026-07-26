use concord::api;

api! {
    client Api {
        base "https://example.test";
        rate_limit {
            1 / 1s;
            by [host];
        }
    }
}

fn main() {}
