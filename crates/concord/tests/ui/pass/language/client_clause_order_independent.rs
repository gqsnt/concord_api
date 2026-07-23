use concord::api;

api! {
    client VarsOrderApi {
        base "https://example.test";
        credentials {}
        vars {
            tenant: String;
        }
    }
}

fn main() {}
