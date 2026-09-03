use concord::api;

api! {
    client MethodApi {
        timeout off;
        base "https://method.example.test";


    METHOD("BAD METHOD") item() -> Bytes using Bytes;
}}

fn main() {}
