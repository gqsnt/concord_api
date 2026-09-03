use concord::api;

api! {
    client VarsOrderApi {
        timeout off;
        base "https://example.test";
        credentials {}
        values {
            tenant: String;
        }
    }
}

fn main() {}
