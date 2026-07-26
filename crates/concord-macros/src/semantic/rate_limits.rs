use super::Resolver;
use super::SpanError;
use super::model::{
    NormalizedConfig, NormalizedRateLimitDimension, NormalizedRateLimitRule,
    NormalizedRateLimitWindow, ResolvedExpr, ValueReference, ValueShape,
};
use super::pagination::PaginationBoundValue;
use super::symbols::ScopeFrame;
use crate::syntax::ast::{Layer, RateLimitDecl, RateLimitDimensionDecl, SourceChild};

#[derive(Clone, Copy)]
pub(super) enum RateLimitDeclarationLayer {
    Client,
    Scope,
    Endpoint,
}

impl RateLimitDeclarationLayer {
    const fn name(self) -> &'static str {
        match self {
            Self::Client => "client",
            Self::Scope => "scope",
            Self::Endpoint => "endpoint",
        }
    }
}

pub(super) struct RateLimitResolutionContext<'a> {
    pub(super) layer: RateLimitDeclarationLayer,
    pub(super) id_path: &'a str,
    pub(super) lexical_origin: &'a str,
}

pub(super) fn source_tree_declares_rate_limits(layer: &Layer, children: &[SourceChild]) -> bool {
    let local = layer.rate_limit.is_some();
    local
        || children.iter().any(|child| match child {
            SourceChild::Scope(scope) => {
                source_tree_declares_rate_limits(&scope.layer, &scope.children)
            }
            SourceChild::Endpoint(endpoint) => endpoint.layer.rate_limit.is_some(),
        })
}

impl Resolver<'_> {
    pub(super) fn resolve_rate_limit(
        &self,
        declaration: &RateLimitDecl,
        scopes: &[ScopeFrame],
        endpoint: Option<&[super::model::Value]>,
        context: &RateLimitResolutionContext<'_>,
    ) -> syn::Result<NormalizedRateLimitRule> {
        let dimensions = declaration
            .dimensions
            .iter()
            .map(|dimension| match dimension {
                RateLimitDimensionDecl::Host { span } => {
                    Ok(NormalizedRateLimitDimension::Host { span: *span })
                }
                RateLimitDimensionDecl::Endpoint { span } => {
                    Ok(NormalizedRateLimitDimension::Endpoint { span: *span })
                }
                RateLimitDimensionDecl::Value {
                    namespace, name, ..
                } => {
                    let public_name = dimension.public_name();
                    match (namespace.to_string().as_str(), context.layer) {
                        ("scope", RateLimitDeclarationLayer::Client) => {
                            return Err(name.span().error(format!(
                                "`{public_name}` is unavailable in a client rate-limit declaration"
                            )));
                        }
                        (
                            "ep",
                            RateLimitDeclarationLayer::Client | RateLimitDeclarationLayer::Scope,
                        ) => {
                            return Err(name.span().error(format!(
                                "`{public_name}` is unavailable in a {} rate-limit declaration",
                                context.layer.name()
                            )));
                        }
                        _ => {}
                    }
                    let value =
                        self.resolve_value_reference(namespace, name, scopes, endpoint)?;
                    match &value.shape {
                        ValueShape::Option { .. } | ValueShape::OptionVec { .. } => {
                            return Err(name.span().error(format!(
                                "rate-limit dimension `{public_name}` must be a required or defaulted scalar; optional values are unsupported"
                            )));
                        }
                        ValueShape::Vec { .. } => {
                            return Err(name.span().error(format!(
                                "rate-limit dimension `{public_name}` must be a scalar; vector values are unsupported"
                            )));
                        }
                        ValueShape::Scalar => {}
                    }
                    Ok(NormalizedRateLimitDimension::Value {
                        public_name,
                        reference: value.reference,
                        span: name.span(),
                    })
                }
            })
            .collect::<syn::Result<Vec<_>>>()?;
        let windows = declaration
            .windows
            .iter()
            .map(|window| {
                let _ = window.duration.span;
                NormalizedRateLimitWindow {
                    capacity: window.capacity,
                    duration: window.duration.value,
                }
            })
            .collect();
        Ok(NormalizedRateLimitRule {
            id: format!("{}.rate_limit", context.id_path),
            origin: context.lexical_origin.to_owned(),
            dimensions,
            cost: declaration.cost,
            windows,
        })
    }
}

pub(super) fn validate_effective_rate_limit_pagination_dependencies(
    config: &NormalizedConfig,
    pagination_values: &[PaginationBoundValue],
) -> syn::Result<()> {
    if pagination_values.is_empty() {
        return Ok(());
    }

    let uses_host = config.rate_limits.iter().any(|rule| {
        rule.dimensions
            .iter()
            .any(|dimension| matches!(dimension, NormalizedRateLimitDimension::Host { .. }))
    });
    let host_dependencies = if uses_host {
        resolved_expression_dependencies(&config.host)
    } else {
        Vec::new()
    };

    for rule in &config.rate_limits {
        for dimension in &rule.dimensions {
            match dimension {
                NormalizedRateLimitDimension::Host { span } => {
                    if let Some(pagination_value) = host_dependencies.iter().find_map(|reference| {
                        pagination_values
                            .iter()
                            .find(|value| value.reference == *reference)
                    }) {
                        return Err(span.error(format!(
                            "`host` cannot partition this rate-limit rule because final host shaping depends on pagination-controlled value `{}`",
                            pagination_value.public_name
                        )));
                    }
                }
                NormalizedRateLimitDimension::Endpoint { .. } => {}
                NormalizedRateLimitDimension::Value {
                    public_name,
                    reference,
                    span,
                } => {
                    if pagination_values
                        .iter()
                        .any(|value| value.reference == *reference)
                    {
                        return Err(span.error(format!(
                            "pagination-controlled value `{public_name}` cannot partition a rate-limit rule"
                        )));
                    }
                }
            }
        }
    }
    Ok(())
}

fn resolved_expression_dependencies(expressions: &[ResolvedExpr]) -> Vec<ValueReference> {
    let mut dependencies = Vec::new();
    for expression in expressions {
        collect_resolved_expression_dependencies(expression, &mut dependencies);
    }
    dependencies
}

fn collect_resolved_expression_dependencies(
    expression: &ResolvedExpr,
    dependencies: &mut Vec<ValueReference>,
) {
    match expression {
        ResolvedExpr::Value(value) => {
            if !dependencies.contains(&value.reference) {
                dependencies.push(value.reference);
            }
        }
        ResolvedExpr::Format(values) => {
            for value in values {
                collect_resolved_expression_dependencies(value, dependencies);
            }
        }
        ResolvedExpr::String(_) | ResolvedExpr::Integer(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::super::model::{NormalizedRateLimitDimension, ValueReference};
    use super::super::test_support;

    fn resolve_error(tokens: proc_macro2::TokenStream) -> String {
        test_support::try_resolve_tokens(tokens)
            .err()
            .expect("rate-limit semantic error")
            .to_string()
    }

    #[test]
    fn composes_rate_rules_in_lexical_order_with_contract_identity() {
        let api = test_support::resolve_tokens(quote! {
            client RateApi {
                base "https://example.test";
                rate_limit { 100 / 1m; }
            }
            scope resources() {
                rate_limit { cost 2; 20 / 1s; }
                GET read() -> Bytes {
                    id "stable.read";
                    rate_limit { 5 / 1s; }
                }
            }
        });
        let endpoint = test_support::endpoint(&api.client.children, "read");

        assert_eq!(
            endpoint
                .config
                .rate_limits
                .iter()
                .map(|rule| rule.id.as_str())
                .collect::<Vec<_>>(),
            [
                "rate_api.rate_limit",
                "rate_api.resources.rate_limit",
                "stable.read.rate_limit",
            ]
        );
        assert_eq!(endpoint.config.rate_limits[1].cost, 2);
        assert!(
            endpoint
                .config
                .rate_limits
                .iter()
                .all(|rule| rule.dimensions.is_empty())
        );
    }

    #[test]
    fn client_rule_rejects_scope_reference() {
        let error = resolve_error(quote! {
            client RateApi {
                base "https://example.test";
                rate_limit { by [scope.region]; 1 / 1s; }
            }
            scope regional(region: String) {
                GET read() -> Bytes;
            }
        });

        assert!(error.contains("`scope.region` is unavailable in a client"));
    }

    #[test]
    fn client_rule_rejects_endpoint_reference() {
        let error = resolve_error(quote! {
            client RateApi {
                base "https://example.test";
                rate_limit { by [ep.account]; 1 / 1s; }
            }
            GET read(account: String) -> Bytes;
        });

        assert!(error.contains("`ep.account` is unavailable in a client"));
    }

    #[test]
    fn scope_rule_accepts_visible_scope_value() {
        let api = test_support::resolve_tokens(quote! {
            client RateApi {
                base "https://example.test";
                vars { tenant: String; }
            }
            scope regional(region: String, shard: u32 = 1) {
                rate_limit {
                    by [host, endpoint, vars.tenant, scope.region, scope.shard];
                    1 / 1s;
                }
                GET read() -> Bytes;
            }
        });
        let endpoint = test_support::endpoint(&api.client.children, "read");
        let dimensions = &endpoint.config.rate_limits[0].dimensions;

        assert!(matches!(
            dimensions[0],
            NormalizedRateLimitDimension::Host { .. }
        ));
        assert!(matches!(
            dimensions[1],
            NormalizedRateLimitDimension::Endpoint { .. }
        ));
        assert!(matches!(
            &dimensions[2],
            NormalizedRateLimitDimension::Value {
                public_name,
                reference: ValueReference::Client(0),
                ..
            } if public_name == "vars.tenant"
        ));
        assert!(matches!(
            &dimensions[3],
            NormalizedRateLimitDimension::Value {
                public_name,
                reference: ValueReference::Scope {
                    scope_id: 0,
                    parameter: 0,
                },
                ..
            } if public_name == "scope.region"
        ));
        assert!(matches!(
            &dimensions[4],
            NormalizedRateLimitDimension::Value {
                public_name,
                reference: ValueReference::Scope {
                    scope_id: 0,
                    parameter: 1,
                },
                ..
            } if public_name == "scope.shard"
        ));
    }

    #[test]
    fn scope_rule_rejects_endpoint_reference() {
        let error = resolve_error(quote! {
            client RateApi { base "https://example.test"; }
            scope regional(region: String) {
                rate_limit { by [ep.account]; 1 / 1s; }
                GET read(account: String) -> Bytes;
            }
        });

        assert!(error.contains("`ep.account` is unavailable in a scope"));
    }

    #[test]
    fn endpoint_rule_accepts_client_scope_and_endpoint_values() {
        let api = test_support::resolve_tokens(quote! {
            client RateApi {
                base "https://example.test";
                vars { tenant: String; }
            }
            scope regional(region: String) {
                GET read(account: String, weight: u32 = 1) -> Bytes {
                    rate_limit {
                        by [vars.tenant, scope.region, ep.account, ep.weight];
                        1 / 1s;
                    }
                }
            }
        });
        let endpoint = test_support::endpoint(&api.client.children, "read");
        let dimensions = &endpoint.config.rate_limits[0].dimensions;

        assert!(matches!(
            dimensions[0],
            NormalizedRateLimitDimension::Value {
                reference: ValueReference::Client(0),
                ..
            }
        ));
        assert!(matches!(
            dimensions[1],
            NormalizedRateLimitDimension::Value {
                reference: ValueReference::Scope {
                    scope_id: 0,
                    parameter: 0,
                },
                ..
            }
        ));
        assert!(matches!(
            dimensions[2],
            NormalizedRateLimitDimension::Value {
                reference: ValueReference::Endpoint(0),
                ..
            }
        ));
        assert!(matches!(
            dimensions[3],
            NormalizedRateLimitDimension::Value {
                reference: ValueReference::Endpoint(1),
                ..
            }
        ));
    }

    #[test]
    fn ambiguous_scope_dimension_is_rejected() {
        let error = resolve_error(quote! {
            client RateApi { base "https://example.test"; }
            scope outer(account: String) {
                scope inner(account: String) {
                    rate_limit { by [scope.account]; 1 / 1s; }
                    GET read() -> Bytes;
                }
            }
        });

        assert!(error.contains("ambiguous visible scope value `account`"));
    }

    #[test]
    fn optional_dimension_is_rejected() {
        let error = resolve_error(quote! {
            client RateApi {
                base "https://example.test";
                vars { tenant: Option<String>; }
                rate_limit { by [vars.tenant]; 1 / 1s; }
            }
        });

        assert!(error.contains("optional values are unsupported"));
    }

    #[test]
    fn vector_dimension_is_rejected() {
        let error = resolve_error(quote! {
            client RateApi {
                base "https://example.test";
                vars { tenants: Vec<String>; }
                rate_limit { by [vars.tenants]; 1 / 1s; }
            }
        });

        assert!(error.contains("vector values are unsupported"));
    }

    #[test]
    fn endpoint_rule_rejects_pagination_bound_endpoint_value() {
        let error = resolve_error(quote! {
            client RateApi { base "https://example.test"; }
            GET list(cursor: String) -> crate::PageSpec {
                rate_limit { by [ep.cursor]; 1 / 1s; }
                paginate crate::Cursor {
                    cursor: bind ep.cursor,
                }
            }
        });

        assert!(error.contains(
            "pagination-controlled value `ep.cursor` cannot partition a rate-limit rule"
        ));
    }

    #[test]
    fn endpoint_rule_rejects_pagination_bound_scope_value() {
        let error = resolve_error(quote! {
            client RateApi { base "https://example.test"; }
            scope pages(cursor: String) {
                GET list() -> crate::PageSpec {
                    rate_limit { by [scope.cursor]; 1 / 1s; }
                    paginate crate::Cursor {
                        cursor: bind scope.cursor,
                    }
                }
            }
        });

        assert!(error.contains(
            "pagination-controlled value `scope.cursor` cannot partition a rate-limit rule"
        ));
    }

    #[test]
    fn endpoint_rule_rejects_pagination_bound_client_value() {
        let error = resolve_error(quote! {
            client RateApi {
                base "https://example.test";
                vars { cursor: String; }
            }
            GET list() -> crate::PageSpec {
                rate_limit { by [vars.cursor]; 1 / 1s; }
                paginate crate::Cursor {
                    cursor: bind vars.cursor,
                }
            }
        });

        assert!(error.contains(
            "pagination-controlled value `vars.cursor` cannot partition a rate-limit rule"
        ));
    }

    #[test]
    fn inherited_scope_rule_rejects_pagination_bound_scope_value() {
        let error = resolve_error(quote! {
            client RateApi { base "https://example.test"; }
            scope pages(cursor: String) {
                rate_limit { by [scope.cursor]; 1 / 1s; }
                GET list() -> crate::PageSpec {
                    paginate crate::Cursor {
                        cursor: bind scope.cursor,
                    }
                }
            }
        });

        assert!(error.contains(
            "pagination-controlled value `scope.cursor` cannot partition a rate-limit rule"
        ));
    }

    #[test]
    fn inherited_client_rule_rejects_pagination_bound_client_value() {
        let error = resolve_error(quote! {
            client RateApi {
                base "https://example.test";
                vars { cursor: String; }
                rate_limit { by [vars.cursor]; 1 / 1s; }
            }
            GET list() -> crate::PageSpec {
                paginate crate::Cursor {
                    cursor: bind vars.cursor,
                }
            }
        });

        assert!(error.contains(
            "pagination-controlled value `vars.cursor` cannot partition a rate-limit rule"
        ));
    }

    #[test]
    fn unrelated_pagination_leaves_client_scope_and_endpoint_dimensions_valid() {
        let api = test_support::resolve_tokens(quote! {
            client RateApi {
                base "https://example.test";
                vars {
                    tenant: String;
                }
            }
            scope pages(region: String) {
                GET list(account: String, cursor: String) -> crate::PageSpec {
                    rate_limit {
                        by [vars.tenant, scope.region, ep.account];
                        1 / 1s;
                    }
                    paginate crate::Cursor {
                        endpoint_cursor: bind ep.cursor,
                    }
                }
            }
        });
        let endpoint = test_support::endpoint(&api.client.children, "list");

        assert_eq!(endpoint.config.rate_limits[0].dimensions.len(), 3);
    }

    #[test]
    fn host_dimension_accepts_static_host_with_unrelated_pagination() {
        let api = test_support::resolve_tokens(quote! {
            client RateApi {
                base "https://example.test";
                rate_limit { by [host]; 1 / 1s; }
            }
            GET list(cursor: String) -> crate::PageSpec {
                paginate crate::Cursor {
                    cursor: bind ep.cursor,
                }
            }
        });

        assert_eq!(
            test_support::endpoint(&api.client.children, "list")
                .config
                .rate_limits
                .len(),
            1
        );
    }

    #[test]
    fn host_dimension_accepts_dynamic_non_pagination_dependency() {
        let api = test_support::resolve_tokens(quote! {
            client RateApi { base "https://example.test"; }
            scope pages(region: String, cursor: String) {
                host [scope.region, "api"];
                rate_limit { by [host]; 1 / 1s; }
                GET list() -> crate::PageSpec {
                    paginate crate::Cursor {
                        cursor: bind scope.cursor,
                    }
                }
            }
        });

        assert_eq!(
            test_support::endpoint(&api.client.children, "list")
                .config
                .rate_limits
                .len(),
            1
        );
    }

    #[test]
    fn host_dimension_rejects_pagination_bound_client_host_input() {
        let error = resolve_error(quote! {
            client RateApi {
                base "https://example.test";
                vars { region: String; }
                host [vars.region, "api"];
                rate_limit { by [host]; 1 / 1s; }
            }
            GET list() -> crate::PageSpec {
                paginate crate::Cursor {
                    region: bind vars.region,
                }
            }
        });

        assert!(error.contains(
            "`host` cannot partition this rate-limit rule because final host shaping depends on pagination-controlled value `vars.region`"
        ));
    }

    #[test]
    fn host_dimension_rejects_pagination_bound_scope_host_input() {
        let error = resolve_error(quote! {
            client RateApi { base "https://example.test"; }
            scope pages(region: String) {
                host [scope.region, "api"];
                rate_limit { by [host]; 1 / 1s; }
                GET list() -> crate::PageSpec {
                    paginate crate::Cursor {
                        region: bind scope.region,
                    }
                }
            }
        });

        assert!(error.contains(
            "`host` cannot partition this rate-limit rule because final host shaping depends on pagination-controlled value `scope.region`"
        ));
    }

    #[test]
    fn host_dimension_rejects_pagination_bound_endpoint_host_input() {
        let error = resolve_error(quote! {
            client RateApi { base "https://example.test"; }
            GET list(region: String) -> crate::PageSpec {
                host [ep.region, "api"];
                rate_limit { by [host]; 1 / 1s; }
                paginate crate::Cursor {
                    region: bind ep.region,
                }
            }
        });

        assert!(error.contains(
            "`host` cannot partition this rate-limit rule because final host shaping depends on pagination-controlled value `ep.region`"
        ));
    }

    #[test]
    fn formatted_host_dependency_is_detected_recursively() {
        let error = resolve_error(quote! {
            client RateApi { base "https://example.test"; }
            scope pages(region: String) {
                host [fmt["edge-", fmt["regional-", scope.region]]];
                rate_limit { by [host]; 1 / 1s; }
                GET list() -> crate::PageSpec {
                    paginate crate::Cursor {
                        region: bind scope.region,
                    }
                }
            }
        });

        assert!(error.contains("pagination-controlled value `scope.region`"));
    }

    #[test]
    fn inherited_host_dimension_is_revalidated_at_endpoint() {
        let error = resolve_error(quote! {
            client RateApi {
                base "https://example.test";
                rate_limit { by [host]; 1 / 1s; }
            }
            scope pages(region: String) {
                host [scope.region, "api"];
                GET list() -> crate::PageSpec {
                    paginate crate::Cursor {
                        region: bind scope.region,
                    }
                }
            }
        });

        assert!(error.contains(
            "`host` cannot partition this rate-limit rule because final host shaping depends on pagination-controlled value `scope.region`"
        ));
    }

    #[test]
    fn secret_dimension_is_rejected() {
        let error = resolve_error(quote! {
            client RateApi {
                base "https://example.test";
                credentials {
                    secret token: String;
                    credential bearer = bearer(secret.token);
                }
                rate_limit { by [secret.token]; 1 / 1s; }
            }
        });

        assert!(error.contains("unknown rate-limit dimension namespace `secret`"));
    }
}
