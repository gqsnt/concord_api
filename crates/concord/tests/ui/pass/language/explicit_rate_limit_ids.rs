use concord::api;

api! {
    client Api {
        base "https://example.test";
    }

    scope first() {
        rate_limit {
            id "service.shared.quota";
            by [host, endpoint];
            cost 1;
            10 / 1s;
            100 / 1m;
        }
        GET read() -> Bytes;
    }

    scope renamed() {
        rate_limit {
            cost 2;
            by [host, endpoint];
            id "service.shared.quota";
            10 / 1s;
            100 / 1m;
        }
        GET write() -> Bytes;
    }
}

fn main() {}
