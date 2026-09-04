use concord::api;

api! {
    client Empty {
        timeout 30s;
        base "https://example.test";
        credentials {
            secret key;
            credential key = api_key(secret.key);
        }

    GET protected() -> Bytes via Binary {
        auth { authorization "" = key; }
    }
}}

api! {
    client Space {
        timeout 30s;
        base "https://example.test";
        credentials {
            secret key;
            credential key = api_key(secret.key);
        }

    GET protected() -> Bytes via Binary {
        auth { authorization "Token value" = key; }
    }
}}

api! {
    client NonAscii {
        timeout 30s;
        base "https://example.test";
        credentials {
            secret key;
            credential key = api_key(secret.key);
        }

    GET protected() -> Bytes via Binary {
        auth { authorization "Tökén" = key; }
    }
}}

api! {
    client Oversized {
        timeout 30s;
        base "https://example.test";
        credentials {
            secret key;
            credential key = api_key(secret.key);
        }

    GET protected() -> Bytes via Binary {
        auth { authorization "ttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttt" = key; }
    }
}}

fn main() {}
