use concord::api;

api! {
    client MethodApi {
        base "https://method.example.test";
    }

    METHOD("BAD METHOD") item() -> Bytes;
}

fn main() {}
