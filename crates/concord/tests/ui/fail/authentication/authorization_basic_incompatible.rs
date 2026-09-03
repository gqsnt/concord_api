use concord::api;

api! {
    client Api {
        timeout off;
        base "https://example.test";
        credentials {
            secret username;
            secret password;
            credential login = basic(secret.username, secret.password);
        }


    GET protected() -> Bytes using Bytes {
        auth { authorization = login; }
    }
}}

fn main() {}
