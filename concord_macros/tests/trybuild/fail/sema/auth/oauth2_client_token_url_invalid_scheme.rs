use concord_macros::api;

api! {
    client Api {
        base "https://example.com"
        secret client_id: String
        secret client_secret: String

        credential oauth = oauth2_client {
            token_url: "http://auth.example.com/token",
            client_id: secret.client_id,
            client_secret: secret.client_secret,
        }
    }
}

fn main() {}
