use concord_macros::api;

api! {
    client SetterApi {
        base "http://setter.example.test";
    }

    GET item(verbose: Option<bool>) -> Bytes;
}

fn main() {
    let client = SetterApi::new().unwrap();
    let _: setter_api::ItemRequest = client.item().verbose(true).clear_verbose();
}
