use concord::api;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";
        rate_limit { id "concord.rate_limit_unknown_namespace.1"; cost 1;  key [other.tenant]; allow 1 per 1s; }
    }
}

fn main() {}
