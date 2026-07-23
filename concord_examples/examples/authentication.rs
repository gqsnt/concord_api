#![forbid(unsafe_code)]

use concord::Request;
use concord::api;

api! {
    client AuthenticationApi {
        base "https://authentication.example.test/";
        credentials {
            secret api_key: String;
            secret bearer_token: String;
            secret username: String;
            secret password: String;
            secret client_id: String;
            secret client_secret: String;
            credential application_key = api_key(secret.api_key);
            credential static_bearer = bearer(secret.bearer_token);
            credential login = basic(secret.username, secret.password);
            credential oauth = oauth2_client {
                token_url: "https://identity.example.test/oauth/token",
                client_id: secret.client_id,
                client_secret: secret.client_secret,
                scope: "records.read",
            };
            credential acquired = endpoint Login;
            credential custom = custom concord_examples::ExampleToken;
        }
    }

    GET Login() -> concord_examples::ExampleTokenCodec;
    GET ApiKey() -> NoContent { auth header "X-Api-Key" = application_key; }
    GET Bearer() -> NoContent { auth bearer static_bearer; }
    GET Basic() -> NoContent { auth basic login; }
    GET OAuth() -> NoContent { auth bearer oauth; }
    GET EndpointBacked() -> NoContent { auth query "session" = acquired challenge never; }
    GET ProviderBacked() -> NoContent { auth bearer custom challenge never; }
}

fn main() -> Result<(), concord::prelude::ClientBuildError> {
    use authentication_api::prelude::*;

    let client = AuthenticationApi::new(
        "example-api-key",
        "example-bearer",
        "example-user",
        "example-password",
        "example-client",
        "example-client-secret",
        concord_examples::ExampleProvider,
    )?;
    let _: Request<_> = client.api_key();
    let _: Request<_> = client.bearer();
    let _: Request<_> = client.basic();
    let _: Request<_> = client.o_auth();
    let _: Request<_> = client.endpoint_backed();
    let _: Request<_> = client.provider_backed();
    let _acquisition = client.login().acquire_as_acquired();
    Ok(())
}
