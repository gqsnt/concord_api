use concord::api;
use concord::Binary;

api! {
    client StreamApi {
        timeout 30s;
        base "http://stream.example.test/";

        GET download() -> concord::response::ByteStream via Binary {
            expose accepted_bytes;
        }
    }
}

fn main() {}
