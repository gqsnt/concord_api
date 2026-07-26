use concord::api;

api! {
    client Api {
        base "https://example.test";
    }

    scope regional(region: String) {
        rate_limit { by [ep.account]; 1 / 1s; }
        GET read(account: String) -> Bytes;
    }
}

fn main() {}
