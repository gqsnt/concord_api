use concord::api;

api! {
    client StreamingApi {
        timeout off;
        base "http://streaming.example.test";


    scope transfers() {
        path ["transfers"];
        GET download() -> ::concord::response::ByteStream using Stream;
    }
}}

fn main() {
    let client = StreamingApi::new().unwrap();
    let _ = client.transfers().download().paginate(1_u32);
}
