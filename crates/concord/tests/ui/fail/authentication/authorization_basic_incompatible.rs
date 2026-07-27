use concord::api;

api! {
    client Api {
        base "https://example.test";
        credentials {
            secret username: String;
            secret password: String;
            credential login = basic(secret.username, secret.password);
        }
    }

    GET protected() -> Bytes {
        auth authorization = login;
    }
}

fn main() {}
