use concord_macros::api;

api! {
    client MethodApi {
        base "https://method.example.test";
    }

    METHOD("BAD METHOD") item() -> Bytes;
}

fn main() {}
