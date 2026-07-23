use concord::{Bytes, api};

api! {
    client PlainApi {
        base "http://plain.example.test/";
    }

    GET Read() -> Bytes;
}

fn main() {
    let client = PlainApi::new().unwrap();
    let _ = client.read().paginate(1_u32);
}
