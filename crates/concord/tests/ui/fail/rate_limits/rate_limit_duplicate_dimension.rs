use concord::api;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";
        values { tenant: String; }
        rate_limit { id "concord.rate_limit_duplicate_dimension.1"; cost 1;  key [client.tenant, client.tenant]; allow 1 per 1s; }
    }
}

fn main() {}
