use concord_macros::api;

api! {
    client Api {
        base "https://example.test";
        var tenant: Option<String>;
        rate_limit { by [vars.tenant]; 1 / 1s; }
        GET Read() -> Bytes;
    }
}

fn main() {}
