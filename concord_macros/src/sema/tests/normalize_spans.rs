use super::helpers::{assert_same_span, normalize_ok, parse_raw, top_raw_scope};
use crate::model::norm::NormNode;

#[test]
fn normalize_spans_preserve_client_scope_and_endpoint_spans() {
    let source = r#"
        api! {
            client SpanApi {
                base "https://example.com"
                secret token: String
                credential key = api_key(secret.token)
            }

            scope tenant(tenant_id: String) {
                host [fmt["tenant-", tenant_id]]
                path ["v1"]

                GET Show
                    path ["profile"]
                    -> Json<String>
            }
        }
        "#;
    let raw = parse_raw(source);
    let norm = normalize_ok(source);

    assert_same_span(norm.client.span, raw.client.span);
    let raw_scope = top_raw_scope(&raw, 0);
    let outer = match &norm.items[0] {
        NormNode::Layer(scope) => scope,
        other => panic!("expected top-level normalized scope, got {other:?}"),
    };
    assert_same_span(outer.span, raw_scope.span);
    let inner = match &outer.items[0] {
        NormNode::Layer(scope) => scope,
        other => panic!("expected synthetic inner scope, got {other:?}"),
    };
    assert_same_span(inner.span, raw_scope.scope_span);
    let raw_endpoint = match &raw_scope.items[0] {
        crate::ast::RawItem::Endpoint(endpoint) => endpoint,
        other => panic!("expected raw endpoint, got {other:?}"),
    };
    let endpoint = match &inner.items[0] {
        NormNode::Endpoint(endpoint) => endpoint,
        other => panic!("expected normalized endpoint, got {other:?}"),
    };
    assert_same_span(endpoint.span, raw_endpoint.span);
}
