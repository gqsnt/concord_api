use concord::api;

api! {
    client Duplicate {
        base "https://example.test";
        rate_limit {
            id "service.first";
            id "service.second";
            1 / 1s;
        }
    }
}

api! {
    client Late {
        base "https://example.test";
        rate_limit {
            1 / 1s;
            id "service.late";
        }
    }
}

fn main() {}
