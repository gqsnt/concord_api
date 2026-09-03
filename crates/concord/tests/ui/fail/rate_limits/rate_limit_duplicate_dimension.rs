use concord::api;

api! {
    client Api {
        timeout off;
        base "https://example.test";
        values { tenant: String; }
        rate_limit { key [client.tenant, client.tenant]; allow 1 per 1s; }
    }
}

fn main() {}
