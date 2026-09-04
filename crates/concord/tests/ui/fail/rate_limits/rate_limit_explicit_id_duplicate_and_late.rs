use concord::api;

api! {
    client Duplicate {
        timeout 30s;
        base "https://example.test";
        rate_limit { key [origin]; cost 1;
            id "service.first";
            id "service.second";
            allow 1 per 1s;
        }
    }
}

api! {
    client Late {
        timeout 30s;
        base "https://example.test";
        rate_limit { key [origin]; cost 1;
            allow 1 per 1s;
            id "service.late";
        }
    }
}

fn main() {}
