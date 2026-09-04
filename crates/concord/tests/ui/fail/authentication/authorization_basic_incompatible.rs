use concord::api;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";
        credentials {
            secret username;
            secret password;
            credential login = basic(secret.username, secret.password);
        }


    GET protected() -> Bytes via Binary {
        auth { authorization = login; }
    }
}}

fn main() {}
