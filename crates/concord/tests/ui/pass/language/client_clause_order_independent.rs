use concord::api;

api! {
    client VarsOrderApi {
        timeout 30s;
        base "https://example.test";
        credentials {}
        values {
            tenant: String;
        }
    }
}

fn main() {}
