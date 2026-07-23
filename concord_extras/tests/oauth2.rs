#![cfg(feature = "oauth2-client")]

use concord_extras::auth::{OAuth2ClientCredentials, OAuth2ClientCredentialsBuildErrorKind};

#[test]
fn oauth2_builder_is_public_named_and_secret_safe() {
    const URL_SENTINEL: &str = "token-path-sentinel-f741";
    const ID_SENTINEL: &str = "client-id-sentinel-190e";
    const SECRET_SENTINEL: &str = "client-secret-sentinel-925c";
    const SCOPE_SENTINEL: &str = "scope-sentinel-9a2d";

    let builder =
        OAuth2ClientCredentials::builder(format!("https://auth.example.test/{URL_SENTINEL}"))
            .client_id(ID_SENTINEL)
            .client_secret(SECRET_SENTINEL)
            .scope(SCOPE_SENTINEL);
    let provider = builder.build().expect("valid named OAuth2 builder");
    for rendered in [format!("{provider:?}")] {
        for sentinel in [URL_SENTINEL, ID_SENTINEL, SECRET_SENTINEL, SCOPE_SENTINEL] {
            assert!(!rendered.contains(sentinel));
        }
    }

    let error = OAuth2ClientCredentials::builder("http://auth.example.test/token")
        .client_id(ID_SENTINEL)
        .client_secret(SECRET_SENTINEL)
        .build()
        .expect_err("OAuth token URL must require HTTPS");
    assert_eq!(
        error.kind(),
        OAuth2ClientCredentialsBuildErrorKind::InvalidTokenUrl
    );
    for rendered in [error.to_string(), format!("{error:?}")] {
        assert!(!rendered.contains(ID_SENTINEL));
        assert!(!rendered.contains(SECRET_SENTINEL));
    }
}
