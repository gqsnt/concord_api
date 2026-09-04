use concord::api;

api! {
    client TypedApi {
        timeout 30s;
        base "https://typed.example.test";


    scope health() {
        path ["health"];
        GET ping() -> () via Empty;
    }
}}

fn main() {
    let client = TypedApi::new().unwrap();
    let _ = client.request(42_u32);
}
