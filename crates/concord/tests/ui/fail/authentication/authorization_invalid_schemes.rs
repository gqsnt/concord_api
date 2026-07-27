use concord::api;

api! {
    client Empty {
        base "https://example.test";
        credentials {
            secret key: String;
            credential key = api_key(secret.key);
        }
    }
    GET protected() -> Bytes {
        auth authorization "" = key;
    }
}

api! {
    client Space {
        base "https://example.test";
        credentials {
            secret key: String;
            credential key = api_key(secret.key);
        }
    }
    GET protected() -> Bytes {
        auth authorization "Token value" = key;
    }
}

api! {
    client NonAscii {
        base "https://example.test";
        credentials {
            secret key: String;
            credential key = api_key(secret.key);
        }
    }
    GET protected() -> Bytes {
        auth authorization "Tökén" = key;
    }
}

api! {
    client Oversized {
        base "https://example.test";
        credentials {
            secret key: String;
            credential key = api_key(secret.key);
        }
    }
    GET protected() -> Bytes {
        auth authorization "ttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttt" = key;
    }
}

fn main() {}
