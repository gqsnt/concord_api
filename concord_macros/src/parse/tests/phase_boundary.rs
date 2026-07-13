use super::helpers::{endpoint_at_top_level, parse_ok};
use crate::ast::{AuthUseDecl, AuthUseKind, PolicyStmt, PolicyValue, RateLimitSpec};
use syn::Expr;

#[test]
fn parser_preserves_raw_names_for_auth_and_rate_limit() {
    let ast = parse_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
            }

            GET Ping
                path ["ping"]
                auth bearer missing_credential
                rate_limit missing_rate_limit
                -> Json<String>
        }
        "#,
    );

    let endpoint = endpoint_at_top_level(&ast, 0);
    match &endpoint.auth_uses[0] {
        AuthUseDecl::Single(kind) => match &**kind {
            AuthUseKind::Bearer { credential } => {
                assert_eq!(credential.to_string(), "missing_credential")
            }
            other => panic!("expected bearer auth use, got {other:?}"),
        },
    }

    match endpoint.rate_limit.as_ref().expect("rate_limit") {
        RateLimitSpec::Profiles { only, profiles } => {
            assert!(!only);
            assert_eq!(profiles.len(), 1);
            assert_eq!(profiles[0].to_string(), "missing_rate_limit");
        }
        other => panic!("expected raw rate_limit profile, got {other:?}"),
    }
}

#[test]
fn policy_values_parse_raw_secret_references() {
    let ast = parse_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
                secret token: String
            }

            GET HeaderRef
                path ["header"]
                headers {
                    "X-Token" = secret.token
                }
                -> Json<String>
        }
        "#,
    );

    let endpoint = endpoint_at_top_level(&ast, 0);
    let header = endpoint.policy.headers.as_ref().expect("headers parsed");
    match &header.stmts[0] {
        PolicyStmt::Set {
            value: PolicyValue::Expr(Expr::Field(field)),
            ..
        } => match &*field.base {
            Expr::Path(path) => {
                assert_eq!(path.path.segments[0].ident, "secret");
            }
            other => panic!("expected raw secret path, got {other:?}"),
        },
        other => panic!("expected raw secret expression, got {other:?}"),
    }
}
