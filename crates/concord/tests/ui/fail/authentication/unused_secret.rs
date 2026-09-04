use concord::api;

api! {
    client UnusedSecretApi {
        timeout 30s;
        base "https://example.test/";
        credentials {
            secret unused;
        }


    GET read() -> () via Empty;
}}

fn main() {}
