use concord_macros::api;

api! {
    client StreamingApi {
        base "http://streaming.example.test";
    }

    scope transfers() {
        path ["transfers"];
        GET download() -> Stream<OctetStream>;
    }
}

fn main() {
    let client = StreamingApi::new().unwrap();
    let _ = client.transfers().download().paginate(1_u32);
}
