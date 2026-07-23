use concord_macros::api;

api! {
    client RawResponseDisabledApi {
        base "http://raw-response-disabled.example.test/";
    }

    GET Read() -> Bytes;
}

fn main() {
    let client = RawResponseDisabledApi::new().expect("client construction");
    let _ = client.read().raw_response();
}
