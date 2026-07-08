use super::helpers::{analyze_ok, client_policy, endpoint_policy, scope_policy};
use crate::sema::PublicValueKind;
#[test]
fn timeout_resolution_lowers_client_scope_endpoint_timeouts() {
    let api = analyze_ok(
        r#"
        api! {
            client Api {
                base "https://example.com"
                var client_timeout: u64
                timeout: 30
            }

            scope outer {
                path ["outer"]
                timeout: vars.client_timeout

                GET Ping(endpoint_timeout: u64)
                    path ["ping"]
                    timeout: endpoint_timeout
                    -> Json<()>
            }
        }
        "#,
    );
    let policy = endpoint_policy(&api, "Ping");

    assert!(matches!(
        &client_policy(&api).timeout,
        Some(PublicValueKind::OtherExpr(_))
    ));
    assert!(matches!(
        &scope_policy(policy, 0).timeout,
        Some(PublicValueKind::CxField(field)) if *field == "client_timeout"
    ));
    assert!(matches!(
        &policy.endpoint.timeout,
        Some(PublicValueKind::EpField(field)) if *field == "endpoint_timeout"
    ));
}
