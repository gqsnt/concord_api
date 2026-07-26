use concord::api;

api! {
    client Api {
        base "https://example.test";
        rate_limit { by [scope.region]; 1 / 1s; }
    }

    scope regional(region: String) {
        GET read() -> Bytes;
    }
}

fn main() {}
