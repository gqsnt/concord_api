use concord::api;

api! {
    client HeaderNameApi {
        base "https://example.test/";
        credentials {
            secret key: String;
            credential key = api_key(secret.key);
        }
        auth header "abcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopq" = key;
    }

    GET read() -> NoContent;
}

fn main() {}
