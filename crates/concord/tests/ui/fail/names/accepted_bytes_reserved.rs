use concord::api;

api! {
    client NamingApi {
        timeout 30s;
        base "https://example.test";

        GET accepted_bytes() -> concord::Bytes via concord::Bytes;
    }
}

fn main() {}
