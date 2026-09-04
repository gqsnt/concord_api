use concord::{Binary, api};

api! {
    client StreamingApi {
        timeout 30s;
        base "http://streaming.example.test";


    scope transfers() {
        path ["transfers"];
        GET download() -> ::concord::response::ByteStream via Binary;
    }
}}

fn main() {
    let client = StreamingApi::new().unwrap();
    let _ = client.transfers().download().paginate(1_u32);
}
