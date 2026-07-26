use concord::api;

api! {
    client Api {
        base "https://example.test";
        vars {
            tenants: Vec<String>;
        }
        rate_limit { by [vars.tenants]; 1 / 1s; }
    }

    GET read() -> Bytes;
}

fn main() {}
