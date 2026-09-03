use concord::api;

pub struct NoDefault;

api! {
    client MissingDefaultApi {
        timeout off;
        base "https://example.test";
        values {
            #[default]
            value: NoDefault;
        }
    }
}

fn main() {}
