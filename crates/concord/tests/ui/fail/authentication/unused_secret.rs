use concord::api;

api! {
    client UnusedSecretApi {
        timeout off;
        base "https://example.test/";
        credentials {
            secret unused;
        }


    GET read() -> () using NoContent;
}}

fn main() {}
