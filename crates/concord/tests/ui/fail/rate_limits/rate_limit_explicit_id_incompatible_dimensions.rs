use concord::api;

api! {
    client Api {
        base "https://example.test";
    }

    scope first() {
        rate_limit {
            id "service.shared";
            by [host, endpoint];
            10 / 1s;
        }
        GET read() -> Bytes;
    }

    scope second() {
        rate_limit {
            id "service.shared";
            by [endpoint, host];
            10 / 1s;
        }
        GET write() -> Bytes;
    }
}

fn main() {}
