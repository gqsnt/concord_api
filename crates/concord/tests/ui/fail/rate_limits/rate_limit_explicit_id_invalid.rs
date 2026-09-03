use concord::api;

api! {
    client Empty {
        timeout off;
        base "https://example.test";
        rate_limit {
            id "";
            allow 1 per 1s;
        }
    }
}

api! {
    client InvalidSegments {
        timeout off;
        base "https://example.test";
        rate_limit {
            id "Service..quota-";
            allow 1 per 1s;
        }
    }
}

api! {
    client Oversized {
        timeout off;
        base "https://example.test";
        rate_limit {
            id "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
            allow 1 per 1s;
        }
    }
}

fn main() {}
