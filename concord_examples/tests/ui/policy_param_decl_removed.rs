use concord_macros::api;

api! {
    client UiPolicyParamDeclRemoved {
        base https "example.com"
    }

    GET Search -> Json<()> {
        query {
            page: u32
        }
    }
}

fn main() {}
