use runtime::api;

api! {
    client OAuthDisabledApi {
        base "http://oauth-disabled.example.test";
        credentials {
            secret client_id: String;
            secret client_secret: String;
            credential oauth = oauth2_client {
                token_url: "https://auth.example.test/token",
                client_id: secret.client_id,
                client_secret: secret.client_secret,
            };
        }
    }

    GET Protected() -> Bytes {
        auth bearer oauth;
    }
}
