#![forbid(unsafe_code)]

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
    let oauth = OAuth2ClientCredentials::builder("https://identity.example.test/oauth/token")
        .client_id("example-client")
        .client_secret("example-client-secret")
        .scope("records.read")
        .build()?;
    let client = AuthenticationApi::builder()
        .api_key(concord::SecretString::new("example-api-key"))
        .bearer_token(concord::SecretString::new("example-bearer"))
        .username(concord::SecretString::new("example-user"))
        .password(concord::SecretString::new("example-password"))
        .oauth_provider(oauth)
        .custom_provider(concord_examples::ExampleProvider)
        .build()?;
    let _: authentication_api::ApiKeyRequest = client.api_key();
    let _: authentication_api::BearerRequest = client.bearer();
    let _: authentication_api::BasicRequest = client.basic();
    let _: authentication_api::OAuthRequest = client.o_auth();
    let _: authentication_api::EndpointBackedRequest = client.endpoint_backed();
    let _: authentication_api::ProviderBackedRequest = client.provider_backed();
    let _acquisition = client.login().store_as_acquired();
    Ok(())
}
