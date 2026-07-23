use concord_macros::api;

api! {
    client LegacyVarApi {
        base "https://example.test";
        var tenant: String;
    }
}

fn main() {}
