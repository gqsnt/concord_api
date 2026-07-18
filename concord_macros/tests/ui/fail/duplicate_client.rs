use concord_macros::api;

api! {
    client DuplicateApi {
        base "https://one.example.test";
    }
    client DuplicateApi {
        base "https://two.example.test";
    }
}

fn main() {}
