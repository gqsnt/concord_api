use concord::api;

api! {
    client QueryNameApi {
        base "https://example.test/";
        credentials {
            secret key: String;
            credential key = api_key(secret.key);
        }
        auth query "abcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopabcdefghijklmnopq" = key;
    }

    GET read() -> NoContent;
}

fn main() {}
