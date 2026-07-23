use concord::api;

api! {
    client SetterApi {
        base "http://setter.example.test";
    }

    GET item(verbose: Option<bool>) -> Bytes;
}

fn main() {
    let client = SetterApi::new().unwrap();
    let request = client.item();
    let _ = request.__concord_inner;
}
