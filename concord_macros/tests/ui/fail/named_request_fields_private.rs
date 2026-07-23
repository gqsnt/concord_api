use concord_macros::api;

api! {
    client SetterApi {
        base "http://setter.example.test";
    }

    GET Item(verbose: Option<bool>) -> Bytes;
}

fn main() {
    let client = SetterApi::new().unwrap();
    let request = client.item();
    let _ = request.__concord_inner;
}
