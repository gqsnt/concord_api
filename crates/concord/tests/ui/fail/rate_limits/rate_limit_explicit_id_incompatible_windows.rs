use concord::api;

api! {
    client Api {
        base "https://example.test";
    }

    scope first() {
        rate_limit {
            id "service.shared";
            10 / 1s;
            100 / 1m;
        }
        GET read() -> Bytes;
    }

    scope second() {
        rate_limit {
            id "service.shared";
            100 / 1m;
            10 / 1s;
        }
        GET write() -> Bytes;
    }
}

fn main() {}
