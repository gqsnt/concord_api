use concord_macros::api;

api! {
    client UiRemovedAuthVars {
        scheme: https,
        host: "example.com",
        auth_vars {
            token: String
        }
    }
}

fn main() {}
