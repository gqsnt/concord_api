use concord_macros::api;

api! {
    client Api {
        base "https://example.test";
        vars {
            tenant: Option<String>;
        }
        rate_limit { by [vars.tenant]; 1 / 1s; }
    }

    GET read() -> Bytes;
}

fn main() {}
