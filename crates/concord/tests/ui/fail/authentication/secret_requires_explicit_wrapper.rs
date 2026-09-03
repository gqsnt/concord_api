use concord::api;

api! {
    client ConfiguredApi {
        timeout off;
        base "http://configured.example.test/";
        credentials {
            secret api_key;
            credential application = api_key(secret.api_key);
        }


    GET read() -> () using NoContent;
}}

fn main() {
    let _ = ConfiguredApi::builder().api_key("protected");
}
