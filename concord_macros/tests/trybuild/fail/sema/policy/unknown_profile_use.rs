use concord_macros::api;

api! {
    client UnknownProfileUseApi {
        base "https://example.com"
    }

    scope users {
        path ["users"]
        profile missing
    }
}

fn main() {}
