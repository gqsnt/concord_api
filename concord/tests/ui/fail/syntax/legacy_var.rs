use concord::api;

api! {
    client LegacyVarApi {
        base "https://example.test";
        var tenant: String;
    }
}

fn main() {}
