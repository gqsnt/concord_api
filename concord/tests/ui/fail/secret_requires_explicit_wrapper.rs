use concord::api;

api! {
    client ConfiguredApi {
        base "http://configured.example.test/";
        credentials {
            secret api_key: String;
            credential application = api_key(secret.api_key);
        }
    }

    GET Read() -> NoContent;
}

fn main() {
    let _ = ConfiguredApi::builder().api_key("protected");
}
