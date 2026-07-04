use crate::ast::{RawApi, RawItem, RawScope};
use crate::model::norm::{NormApiTree, NormEndpoint, NormNode, NormScope};
use crate::sema::{ResolvedApi, ResolvedEndpoint};
use syn::Type;

pub(crate) fn parse_raw(source: &str) -> RawApi {
    syn::parse_str(source).expect("source should parse")
}

pub(crate) fn analyze_ok(source: &str) -> ResolvedApi {
    super::super::analyze(parse_raw(source)).expect("analysis should succeed")
}

pub(crate) fn analyze_err(source: &str) -> syn::Error {
    super::super::analyze(parse_raw(source)).expect_err("analysis should fail")
}

pub(crate) fn normalize_ok(source: &str) -> NormApiTree {
    super::super::normalize_api(parse_raw(source)).expect("normalization should succeed")
}

pub(crate) fn top_scope(tree: &NormApiTree, index: usize) -> &NormScope {
    match tree.items.get(index).expect("missing top-level item") {
        NormNode::Layer(scope) => scope,
        other => panic!("expected top-level scope at index {index}, got {other:?}"),
    }
}

pub(crate) fn top_endpoint(tree: &NormApiTree, index: usize) -> &NormEndpoint {
    match tree.items.get(index).expect("missing top-level item") {
        NormNode::Endpoint(endpoint) => endpoint,
        other => panic!("expected top-level endpoint at index {index}, got {other:?}"),
    }
}

pub(crate) fn endpoint_by_name<'a>(api: &'a ResolvedApi, name: &str) -> &'a ResolvedEndpoint {
    api.endpoints
        .iter()
        .find(|endpoint| endpoint.name == name)
        .unwrap_or_else(|| panic!("missing endpoint `{name}`"))
}

pub(crate) fn single_endpoint(api: &ResolvedApi) -> &ResolvedEndpoint {
    match api.endpoints.as_slice() {
        [endpoint] => endpoint,
        other => panic!("expected a single endpoint, got {other:?}"),
    }
}

pub(crate) fn single_child_scope(scope: &NormScope) -> &NormScope {
    match scope.items.as_slice() {
        [NormNode::Layer(child)] => child,
        other => panic!("expected single child scope, got {other:?}"),
    }
}

pub(crate) fn single_child_endpoint(scope: &NormScope) -> &NormEndpoint {
    match scope.items.as_slice() {
        [NormNode::Endpoint(endpoint)] => endpoint,
        other => panic!("expected single child endpoint, got {other:?}"),
    }
}

pub(crate) fn top_raw_scope(api: &RawApi, index: usize) -> &RawScope {
    match api.items.get(index).expect("missing top-level item") {
        RawItem::Layer(scope) => scope,
        other => panic!("expected top-level raw scope at index {index}, got {other:?}"),
    }
}

pub(crate) fn span_debug(span: proc_macro2::Span) -> String {
    format!("{span:?}")
}

pub(crate) fn assert_same_span(left: proc_macro2::Span, right: proc_macro2::Span) {
    assert_eq!(span_debug(left), span_debug(right));
}

pub(crate) fn ty_string(ty: &Type) -> String {
    quote::quote!(#ty).to_string().replace(' ', "")
}

pub(crate) fn assert_error_contains(err: &syn::Error, expected: &str) {
    assert!(
        err.to_string().contains(expected),
        "expected error to contain `{expected}`, got `{err}`"
    );
}
