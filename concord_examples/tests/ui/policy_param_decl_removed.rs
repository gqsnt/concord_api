use concord_macros::api;

api! {
    client UiPolicyParamDeclRemoved {
        scheme: https,
        host: "example.com",
    }

    GET Search -> Json<()> {
        query {
            page: u32
        }
    }
}

fn main() {}
