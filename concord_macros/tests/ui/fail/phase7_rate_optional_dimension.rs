use concord_macros::api;

api! {
    client Api {
        base "https://example.test";
        var tenant: Option<String>;
        rate_limits { tenant { by [vars.tenant]; 1 / 1s; } }
        rate_limit tenant;
        GET Read() -> Bytes;
    }
}

fn main() {}
