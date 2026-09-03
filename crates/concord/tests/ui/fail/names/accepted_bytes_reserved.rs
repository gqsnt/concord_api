use concord::api;

api! {
    client NamingApi {
        timeout off;
        base "https://example.test";

        GET accepted_bytes() -> concord::Bytes using concord::Bytes;
    }
}

fn main() {}
