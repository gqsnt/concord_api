use concord_macros::api;

api! {
    client UserInfoBaseApi {
        base "https://user@example.com"
    }
}

api! {
    client UserPassBaseApi {
        base "https://user:pass@example.com"
    }
}

fn main() {}
