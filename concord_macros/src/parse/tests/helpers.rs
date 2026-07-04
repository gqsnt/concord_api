use crate::ast::{RawApi, RawEndpoint, RawItem, RawScope};

pub(crate) fn parse_ok(source: &str) -> RawApi {
    syn::parse_str(source).expect("source should parse")
}

pub(crate) fn parse_err(source: &str) -> syn::Error {
    syn::parse_str::<RawApi>(source).expect_err("source should fail")
}

pub(crate) fn endpoint_at_top_level(ast: &RawApi, index: usize) -> &RawEndpoint {
    match ast.items.get(index).expect("missing top-level item") {
        RawItem::Endpoint(endpoint) => endpoint,
        other => panic!("expected top-level endpoint at index {index}, got {other:?}"),
    }
}

pub(crate) fn scope_at_top_level(ast: &RawApi, index: usize) -> &RawScope {
    match ast.items.get(index).expect("missing top-level item") {
        RawItem::Layer(scope) => scope,
        other => panic!("expected top-level scope at index {index}, got {other:?}"),
    }
}

pub(crate) fn endpoint_in_scope(scope: &RawScope, index: usize) -> &RawEndpoint {
    match scope.items.get(index).expect("missing scoped item") {
        RawItem::Endpoint(endpoint) => endpoint,
        other => panic!("expected scoped endpoint at index {index}, got {other:?}"),
    }
}
