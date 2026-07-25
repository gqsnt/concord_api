use concord::api;

api! {
    client UnusedSecretApi {
        base "https://example.test/";
        credentials {
            secret unused: String;
        }
    }

    GET read() -> NoContent;
}

fn main() {}
