use concord::api;

api! {
    client Duplicate {
        timeout off;
        base "https://example.test";
        rate_limit {
            id "service.first";
            id "service.second";
            allow 1 per 1s;
        }
    }
}

api! {
    client Late {
        timeout off;
        base "https://example.test";
        rate_limit {
            allow 1 per 1s;
            id "service.late";
        }
    }
}

fn main() {}
