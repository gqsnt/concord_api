use concord_macros::api;

api! {
    client ReferenceApi {
        base "https://reference.example.test";
        GET Item() -> Bytes {
            path [ep.missing];
        }
    }
}

fn main() {}
