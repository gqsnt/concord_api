use crate::ast::{RawApi, RawItem, RawScope};
use crate::model::norm::{NormApiTree, NormEndpoint, NormNode, NormScope};
use crate::sema::{
    AuthCredentialIr, AuthRequirementIr, PaginateResolved, PolicyBlocksResolved, PolicyOp,
    RateLimitPlanResolved, RateLimitResolved, ResolvedApi, ResolvedEndpoint, ResolvedPolicySpec,
};
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

pub(crate) fn scope_module_path(endpoint: &ResolvedEndpoint) -> Vec<String> {
    endpoint
        .scope_modules
        .iter()
        .map(ToString::to_string)
        .collect()
}

pub(crate) fn single_endpoint(api: &ResolvedApi) -> &ResolvedEndpoint {
    match api.endpoints.as_slice() {
        [endpoint] => endpoint,
        other => panic!("expected a single endpoint, got {other:?}"),
    }
}

pub(crate) fn endpoint_pagination(endpoint: &ResolvedEndpoint) -> &PaginateResolved {
    endpoint.paginate.as_ref().expect("pagination resolved")
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

pub(crate) fn credential_by_name<'a>(api: &'a ResolvedApi, name: &str) -> &'a AuthCredentialIr {
    api.client_auth_credentials
        .iter()
        .find(|credential| credential.name == name)
        .unwrap_or_else(|| panic!("missing credential `{name}`"))
}

pub(crate) fn auth_for_endpoint<'a>(
    api: &'a ResolvedApi,
    endpoint: &str,
) -> &'a [AuthRequirementIr] {
    &endpoint_by_name(api, endpoint).policy.auth
}

pub(crate) fn auth_requirement_names(auth: &[AuthRequirementIr]) -> Vec<String> {
    auth.iter().map(|req| req.credential.to_string()).collect()
}

pub(crate) fn auth_requirement_step_ids(auth: &[AuthRequirementIr]) -> Vec<String> {
    auth.iter().map(|req| req.step_id.clone()).collect()
}

pub(crate) fn auth_requirement_provenance_labels(auth: &[AuthRequirementIr]) -> Vec<String> {
    auth.iter()
        .map(|req| req.provenance.label.clone())
        .collect()
}

pub(crate) fn assert_auth_error_contains(source: &str, expected: &str) {
    let err = analyze_err(source);
    assert_error_contains(&err, expected);
}

pub(crate) fn client_policy(api: &ResolvedApi) -> &PolicyBlocksResolved {
    &api.client_policy
}

pub(crate) fn endpoint_policy<'a>(api: &'a ResolvedApi, endpoint: &str) -> &'a ResolvedPolicySpec {
    &endpoint_by_name(api, endpoint).policy
}

pub(crate) fn scope_policy(policy: &ResolvedPolicySpec, index: usize) -> &PolicyBlocksResolved {
    policy
        .scopes
        .get(index)
        .unwrap_or_else(|| panic!("missing scope policy at index {index}"))
}

pub(crate) fn query_ops(policy: &PolicyBlocksResolved) -> &[PolicyOp] {
    &policy.query
}

pub(crate) fn header_ops(policy: &PolicyBlocksResolved) -> &[PolicyOp] {
    &policy.headers
}

pub(crate) fn assert_policy_error_contains(source: &str, expected: &str) {
    let err = analyze_err(source);
    assert_error_contains(&err, expected);
}

pub(crate) fn assert_profile_error_contains(source: &str, expected: &str) {
    let err = analyze_err(source);
    assert_error_contains(&err, expected);
}

pub(crate) fn assert_route_error_contains(source: &str, expected: &str) {
    let err = analyze_err(source);
    assert_error_contains(&err, expected);
}

pub(crate) fn rate_limit_plan(rate_limit: &RateLimitResolved) -> &RateLimitPlanResolved {
    match rate_limit {
        RateLimitResolved::Add(plan) | RateLimitResolved::Replace(plan) => plan,
        RateLimitResolved::Clear => panic!("expected resolved rate limit"),
    }
}

pub(crate) fn effective_endpoint_rate_limit_bucket_names(
    api: &ResolvedApi,
    endpoint: &ResolvedEndpoint,
) -> Vec<String> {
    let mut bucket_names = Vec::new();
    let mut apply = |layer: &Option<RateLimitResolved>| match layer {
        Some(RateLimitResolved::Add(plan)) | Some(RateLimitResolved::Replace(plan)) => {
            bucket_names.extend(plan.buckets.iter().map(|bucket| bucket.name.clone()))
        }
        Some(RateLimitResolved::Clear) => bucket_names.clear(),
        None => {}
    };

    apply(&api.client_policy.rate_limit);
    for scope in &endpoint.policy.scopes {
        apply(&scope.rate_limit);
    }
    apply(&endpoint.policy.endpoint.rate_limit);

    bucket_names
}
