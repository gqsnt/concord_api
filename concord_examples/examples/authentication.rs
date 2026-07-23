#![forbid(unsafe_code)]

use concord::Request;
use concord::api;
use concord::auth::OAuth2ClientCredentials;

api! {
    client AuthenticationApi {
        base "https://authentication.example.test/";
        credentials {
            secret api_key: String;
            secret bearer_token: String;
            secret username: String;
            secret password: String;
            credential application_key = api_key(secret.api_key);
            credential static_bearer = bearer(secret.bearer_token);
            credential login = basic(secret.username, secret.password);
            credential oauth = custom OAuth2ClientCredentials;
            credential acquired = endpoint Login;
            credential custom = custom concord_examples::ExampleProvider;
        }
    }

    GET Login() -> concord_examples::ExampleTokenCodec;
    GET ApiKey() -> NoContent { auth header "X-Api-Key" = application_key; }
    GET Bearer() -> NoContent { auth bearer static_bearer; }
    GET Basic() -> NoContent { auth basic login; }
    GET OAuth() -> NoContent { auth bearer oauth challenge unauthorized; }
    GET EndpointBacked() -> NoContent { auth query "session" = acquired challenge never; }
    GET ProviderBacked() -> NoContent { auth bearer custom challenge never; }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    use authentication_api::prelude::*;

    let oauth = OAuth2ClientCredentials::builder("https://identity.example.test/oauth/token")
        .client_id("example-client")
        .client_secret("example-client-secret")
        .scope("records.read")
        .build()?;
    let client = AuthenticationApi::builder()
        .api_key("example-api-key")
        .bearer_token("example-bearer")
        .username("example-user")
        .password("example-password")
        .oauth_provider(oauth)
        .custom_provider(concord_examples::ExampleProvider)
        .build()?;
    let _: Request<_> = client.api_key();
    let _: Request<_> = client.bearer();
    let _: Request<_> = client.basic();
    let _: Request<_> = client.o_auth();
    let _: Request<_> = client.endpoint_backed();
    let _: Request<_> = client.provider_backed();
    let _acquisition = client.login().acquire_as_acquired();
    Ok(())
}
