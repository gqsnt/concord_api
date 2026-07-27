use concord::api;

api! {
    client Api {
        base "https://example.test";
        rate_limit {
            id "service.shared";
            10 / 1s;
        }
    }

    GET read() -> Bytes {
        rate_limit {
            id "service.shared";
            cost 2;
            10 / 1s;
        }
    }
}

fn main() {}
