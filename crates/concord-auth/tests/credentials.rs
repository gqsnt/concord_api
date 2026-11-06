//! Deterministic credential and cache contract tests.
use concord_auth::{CredentialError, authorization, basic, bearer};

#[test]
fn standard_basic_vector_and_sensitive_flag() {
    let header = basic("Aladdin", "open sesame").unwrap();
    assert_eq!(header, "Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ==");
    assert!(header.is_sensitive());
}

#[test]
fn empty_password_empty_user_and_colons_are_explicit() {
    for (user, password, expected) in [
        ("", "", "Basic Og=="),
        ("u", "", "Basic dTo="),
        ("", "p", "Basic OnA="),
        ("u", "p:q", "Basic dTpwOnE="),
    ] {
        assert_eq!(basic(user, password).unwrap(), expected);
    }
    assert_eq!(basic("u:x", "p").unwrap_err(), CredentialError::Username);
}

#[test]
fn basic_uses_utf8_and_refuses_every_unicode_control() {
    assert_eq!(basic("ü", "é").unwrap(), "Basic w7w6w6k=");
    for c in (0..=0x10ffff)
        .filter_map(char::from_u32)
        .filter(|c| c.is_control())
    {
        let input = format!("sentinel{c}");
        assert_eq!(basic(&input, "p").unwrap_err(), CredentialError::Username);
        assert_eq!(basic("u", &input).unwrap_err(), CredentialError::Password);
    }
}

#[test]
fn bearer_and_other_schemes_are_sensitive() {
    assert_eq!(bearer("abc+/=_-.").unwrap(), "Bearer abc+/=_-.");
    let header = authorization("Token", "sentinel").unwrap();
    assert_eq!(header, "Token sentinel");
    assert!(header.is_sensitive());
}

#[test]
fn every_control_and_whitespace_is_refused_in_token() {
    for byte in (0u8..=32).chain([127]) {
        let token = format!("sentinel{}", char::from(byte));
        assert_eq!(bearer(&token).unwrap_err(), CredentialError::Token);
    }
    for invalid in ["", "sécret", "a b", "a\u{85}", "a\u{a0}"] {
        assert_eq!(bearer(invalid).unwrap_err(), CredentialError::Token);
    }
}

#[test]
fn scheme_must_be_an_http_token() {
    for scheme in ["", "Bearer bad", "Bearer\r\n", "secret:", "ü", "Token\t"] {
        assert_eq!(
            authorization(scheme, "secret").unwrap_err(),
            CredentialError::Scheme
        );
    }
    assert!(authorization("custom!#$%&'*+-.^_`|~", "token").is_ok());
}

#[test]
fn credentials_and_errors_never_format_inputs() {
    let basic = basic("sentinel-user", "sentinel-password").unwrap();
    let encoded = basic.to_str().unwrap().to_owned();
    let bearer = bearer("sentinel-token").unwrap();
    let other = authorization("sentinel-scheme", "sentinel-other").unwrap();
    let errors = [
        CredentialError::Username,
        CredentialError::Password,
        CredentialError::Scheme,
        CredentialError::Token,
    ];
    let mut rendered = format!("{basic:?} {bearer:?} {other:?}");
    for error in errors {
        rendered.push_str(&format!("{error:?} {error}"));
    }
    for secret in [
        "sentinel-user",
        "sentinel-password",
        "sentinel-token",
        "sentinel-scheme",
        "sentinel-other",
        &encoded,
    ] {
        assert!(!rendered.contains(secret));
    }
}
