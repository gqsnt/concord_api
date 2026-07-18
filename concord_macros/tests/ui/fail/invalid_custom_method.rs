use concord_macros::api;

api! {
    client MethodApi {
        base "https://method.example.test";
        METHOD("BAD METHOD") Item() -> Bytes;
    }
}

fn main() {}
