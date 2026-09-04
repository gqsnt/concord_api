use concord::api;

api! {
    client Empty {
        timeout 30s;
        base "https://example.test";
        rate_limit { key [origin]; cost 1;
            id "";
            allow 1 per 1s;
        }
    }
}

api! {
    client InvalidSegments {
        timeout 30s;
        base "https://example.test";
        rate_limit { key [origin]; cost 1;
            id "Service..quota-";
            allow 1 per 1s;
        }
    }
}

api! {
    client Oversized {
        timeout 30s;
        base "https://example.test";
        rate_limit { key [origin]; cost 1;
            id "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
            allow 1 per 1s;
        }
    }
}

fn main() {}
