use concord_macros::api;

api! {
    client OldAuthBlockApi {
        base https "example.com"
        auth {
            credential key = api_key(secret.token)
        }
    }
}

fn main() {}
