use concord::api;
use concord::body::Stream;

api! {
    client StreamApi {
        timeout off;
        base "http://stream.example.test/";

        GET download() -> concord::response::ByteStream using Stream {
            expose accepted_bytes;
        }
    }
}

fn main() {}
