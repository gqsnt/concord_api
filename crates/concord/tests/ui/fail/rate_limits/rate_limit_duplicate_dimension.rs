use concord::api;

api! {
    client Api {
        base "https://example.test";
        vars { tenant: String; }
        rate_limit { by [vars.tenant, vars.tenant]; 1 / 1s; }
    }
}

fn main() {}
