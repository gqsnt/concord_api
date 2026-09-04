use concord::api;

pub struct NoDefault;

api! {
    client MissingDefaultApi {
        timeout 30s;
        base "https://example.test";
        values {
            #[default]
            value: NoDefault;
        }
    }
}

fn main() {}
