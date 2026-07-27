use concord::api;

api! {
    client Empty {
        base "https://example.test";
        rate_limit {
            id "";
            1 / 1s;
        }
    }
}

api! {
    client InvalidSegments {
        base "https://example.test";
        rate_limit {
            id "Service..quota-";
            1 / 1s;
        }
    }
}

api! {
    client Oversized {
        base "https://example.test";
        rate_limit {
            id "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
            1 / 1s;
        }
    }
}

fn main() {}
