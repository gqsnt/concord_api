use concord_macros::api;

api! {
    client JsonDisabledApi {
        base "http://json-disabled.example.test";
        credentials {
            secret client_id: String;
            secret client_secret: String;
            credential oauth = oauth2_client {
                token_url: "https://auth.example.test/token",
                client_id: secret.client_id,
                client_secret: secret.client_secret,
            };
        }
        scope records() {
            path ["records"];
            POST Create(body: Json<Vec<u32>>) -> Json<Vec<u32>> {
                auth bearer oauth;
            }
        }
    }
}
