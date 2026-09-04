use concord::api;

api! {
    client MethodApi {
        timeout 30s;
        base "https://method.example.test";


    METHOD("BAD METHOD") item() -> Bytes via Binary;
}}

fn main() {}
