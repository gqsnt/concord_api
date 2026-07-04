use concord_macros::api;

api! {
    client BackslashBaseApi {
        base "https://example.com\\evil"
    }
}

api! {
    client BackslashSchemeApi {
        base "https:\\\\example.com"
    }
}

fn main() {}
