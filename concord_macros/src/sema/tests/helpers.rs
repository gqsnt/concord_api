use crate::ast::{RawApi, RawItem, RawScope};
use crate::model::norm::{NormApiTree, NormEndpoint, NormNode, NormScope};

pub(crate) fn parse_raw(source: &str) -> RawApi {
    syn::parse_str(source).expect("source should parse")
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
