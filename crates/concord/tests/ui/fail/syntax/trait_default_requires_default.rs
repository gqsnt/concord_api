use concord::api;

pub struct NoDefault;

api! {
    client MissingDefaultApi {
        base "https://example.test";
        vars {
            value: NoDefault = default;
        }
    }
}

fn main() {}
