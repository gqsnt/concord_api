use concord::api;

api! {
    client Empty {
        timeout off;
        base "https://example.test";
        credentials {
            secret key;
            credential key = api_key(secret.key);
        }

    GET protected() -> Bytes using Bytes {
        auth { authorization "" = key; }
    }
}}

api! {
    client Space {
        timeout off;
        base "https://example.test";
        credentials {
            secret key;
            credential key = api_key(secret.key);
        }

    GET protected() -> Bytes using Bytes {
        auth { authorization "Token value" = key; }
    }
}}

api! {
    client NonAscii {
        timeout off;
        base "https://example.test";
        credentials {
            secret key;
            credential key = api_key(secret.key);
        }

    GET protected() -> Bytes using Bytes {
        auth { authorization "Tökén" = key; }
    }
}}

api! {
    client Oversized {
        timeout off;
        base "https://example.test";
        credentials {
            secret key;
            credential key = api_key(secret.key);
        }

    GET protected() -> Bytes using Bytes {
        auth { authorization "ttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttttt" = key; }
    }
}}

fn main() {}
