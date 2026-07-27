use concord::api;

api! {
    client NamingApi {
        base "https://example.test";
    }

    GET response_bytes() -> concord::Bytes;
}

fn main() {}
