use concord_core::advanced::{Transport, TransportError, TransportRequest, TransportResponse};
use concord_core::prelude::*;
use concord_macros::api;
use std::future::Future;
use std::pin::Pin;
use self::both_config_api::BothConfigApi;
use self::declaration_order_api::DeclarationOrderApi;
use self::o_auth_config_api::OAuthConfigApi;
use self::secret_config_api::SecretConfigApi;
use self::vars_config_api::VarsConfigApi;

#[derive(Clone)]
struct FailingTransport;

impl Transport for FailingTransport {
    fn send(
        &self,
        _req: TransportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        Box::pin(async {
            Err(TransportError::new(std::io::Error::other(
                "intentional test transport",
            )))
        })
    }
}

api! {
    client VarsConfigApi {
        base "https://example.com"
        var tenant: String
    }

    GET VarsPing
        path ["ping"]
        -> Json<String>
}

api! {
    client SecretConfigApi {
        base "https://example.com"
        secret api_key: String
        credential key = api_key(secret.api_key)
    }

    GET SecretPing
        path ["ping"]
        auth header "X-Api-Key" = key
        -> Json<String>
}

api! {
    client BothConfigApi {
        base "https://example.com"
        var tenant: String
        secret api_key: String
        credential key = api_key(secret.api_key)
    }

    GET BothPing
        path ["ping"]
        auth header "X-Api-Key" = key
        -> Json<String>
}

api! {
    client OAuthConfigApi {
        base "https://example.com"
        secret client_id: String
        secret client_secret: String
        credential oauth = oauth2_client {
            token_url: "https://auth.example.com/oauth/token",
            client_id: secret.client_id,
            client_secret: secret.client_secret,
            scope: "read:ping",
        }
    }

    GET OAuthPing
        path ["oauth-ping"]
        auth bearer oauth
        -> Json<String>
}

api! {
    client DeclarationOrderApi {
        base "https://example.com"
        var tenant: String
        var region: String
        secret username: String
        secret password: String
        credential login = basic(secret.username, secret.password)
    }

    GET OrderedPing
        path ["ordered-ping"]
        auth basic login
        -> Json<String>
}

fn constructor_shape_is_stable() -> Result<(), ApiClientError> {
    let _vars = VarsConfigApi::new("tenant".to_string());
    let _vars = VarsConfigApi::builder()
        .tenant("tenant".to_string())
        .build()?;

    let _secret = SecretConfigApi::new("secret".to_string());
    let _secret = SecretConfigApi::builder()
        .api_key("secret".to_string())
        .build()?;

    let _both = BothConfigApi::new("tenant".to_string(), "secret".to_string());
    let _both = BothConfigApi::builder()
        .tenant("tenant".to_string())
        .api_key("secret".to_string())
        .build()?;

    let _with_transport = BothConfigApi::new_with_transport(
        "tenant".to_string(),
        "secret".to_string(),
        FailingTransport,
    );

    let _oauth = OAuthConfigApi::new("client-id".to_string(), "client-secret".to_string());
    let _oauth = OAuthConfigApi::builder()
        .client_id("client-id".to_string())
        .client_secret("client-secret".to_string())
        .build()?;
    let _oauth_with_transport = OAuthConfigApi::new_with_transport(
        "client-id".to_string(),
        "client-secret".to_string(),
        FailingTransport,
    );

    let _ordered = DeclarationOrderApi::new(
        "tenant".to_string(),
        "region".to_string(),
        "username".to_string(),
        "password".to_string(),
    );
    let _ordered = DeclarationOrderApi::builder()
        .tenant("tenant".to_string())
        .region("region".to_string())
        .username("username".to_string())
        .password("password".to_string())
        .build()?;
    let _ordered_with_transport = DeclarationOrderApi::new_with_transport(
        "tenant".to_string(),
        "region".to_string(),
        "username".to_string(),
        "password".to_string(),
        FailingTransport,
    );

    Ok(())
}

fn normal_use_does_not_name_endpoint_markers(api: BothConfigApi<FailingTransport>) {
    let _pending = api.both_ping();
}

fn oauth_normal_use_does_not_name_endpoint_markers(api: OAuthConfigApi<FailingTransport>) {
    let _pending = api.oauth_ping();
}

fn main() {}
