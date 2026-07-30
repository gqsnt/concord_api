use super::Resolver;
use super::SpanError;
use super::model::{
    NormalizedConfig, NormalizedRateLimitDimension, NormalizedRateLimitRule,
    NormalizedRateLimitWindow, ResolvedExpr, ValueReference, ValueShape,
};
use super::pagination::PaginationBoundValue;
use super::surface::{canonical_ident, to_snake_case};
use super::symbols::ScopeFrame;
use crate::syntax::ast::{
    Layer, RateLimitAttachmentDecl, RateLimitDecl, RateLimitDefinitionDecl,
    RateLimitDefinitionDimensionDecl, RateLimitDimensionDecl, RateLimitUseDecl, SourceChild,
};
use crate::{MAX_RATE_LIMIT_ID_BYTES, MAX_RATE_LIMIT_KEY_PART_NAME_BYTES};

#[derive(Clone)]
pub(super) struct RateLimitDefinition {
    name: String,
    name_span: proc_macro2::Span,
    id: String,
    explicit_id: bool,
    id_span: proc_macro2::Span,
    dimensions: Vec<ResolvedDefinitionDimension>,
    cost: u32,
    windows: Vec<NormalizedRateLimitWindow>,
    uses: usize,
}

#[derive(Clone)]
enum ResolvedDefinitionDimension {
    Host {
        span: proc_macro2::Span,
    },
    Endpoint {
        span: proc_macro2::Span,
    },
    Formal {
        public_name: String,
        span: proc_macro2::Span,
    },
}

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
    let local = !layer.rate_limits.is_empty();
    local
        || children.iter().any(|child| match child {
            SourceChild::Scope(scope) => {
                source_tree_declares_rate_limits(&scope.layer, &scope.children)
            }
            SourceChild::Endpoint(endpoint) => !endpoint.layer.rate_limits.is_empty(),
        })
}

pub(super) fn resolve_rate_limit_definitions(
    client_name: &syn::Ident,
    definitions: Vec<RateLimitDefinitionDecl>,
) -> syn::Result<Vec<RateLimitDefinition>> {
    let client_name = to_snake_case(&canonical_ident(client_name));
    definitions
        .into_iter()
        .map(|definition| {
            let name = canonical_ident(&definition.name);
            let name_span = definition.name.span();
            let (id, explicit_id, id_span) = definition.id.as_ref().map_or_else(
                || {
                    (
                        format!("{client_name}.rate_limit.{name}"),
                        false,
                        definition.name.span(),
                    )
                },
                |id| (id.value.value(), true, id.value.span()),
            );
            if id.len() > MAX_RATE_LIMIT_ID_BYTES {
                return Err(id_span.error(format!(
                    "a rate-limit rule ID permits at most {MAX_RATE_LIMIT_ID_BYTES} UTF-8 bytes"
                )));
            }
            let dimensions = definition
                .dimensions
                .iter()
                .map(|dimension| match dimension {
                    RateLimitDefinitionDimensionDecl::Host { span } => {
                        ResolvedDefinitionDimension::Host { span: *span }
                    }
                    RateLimitDefinitionDimensionDecl::Endpoint { span } => {
                        ResolvedDefinitionDimension::Endpoint { span: *span }
                    }
                    RateLimitDefinitionDimensionDecl::Formal { name } => {
                        ResolvedDefinitionDimension::Formal {
                            public_name: canonical_ident(name),
                            span: name.span(),
                        }
                    }
                })
                .collect();
            let windows = definition
                .windows
                .iter()
                .map(|window| NormalizedRateLimitWindow {
                    capacity: window.capacity,
                    duration: window.duration.value,
                })
                .collect();
            let _ = definition.cost_span;
            Ok(RateLimitDefinition {
                name,
                name_span,
                id,
                explicit_id,
                id_span,
                dimensions,
                cost: definition.cost,
                windows,
                uses: 0,
            })
        })
        .collect()
}

pub(super) fn validate_rate_limit_definitions_used(
    definitions: &[RateLimitDefinition],
) -> syn::Result<()> {
    if let Some(definition) = definitions.iter().find(|definition| definition.uses == 0) {
        return Err(definition.name_span.error(format!(
            "rate-limit profile `{}` is never attached",
            definition.name
        )));
    }
    Ok(())
}

impl Resolver<'_> {
    pub(super) fn resolve_rate_limit(
        &mut self,
        attachment: &RateLimitAttachmentDecl,
        declaration_span: proc_macro2::Span,
        scopes: &[ScopeFrame],
        endpoint: Option<&[super::model::Value]>,
        context: &RateLimitResolutionContext<'_>,
    ) -> syn::Result<NormalizedRateLimitRule> {
        match attachment {
            RateLimitAttachmentDecl::Inline(declaration) => self.resolve_inline_rate_limit(
                declaration,
                declaration_span,
                scopes,
                endpoint,
                context,
            ),
            RateLimitAttachmentDecl::Named(attachment) => {
                self.resolve_named_rate_limit(attachment, scopes, endpoint, context)
            }
        }
    }

    fn resolve_inline_rate_limit(
        &self,
        declaration: &RateLimitDecl,
        declaration_span: proc_macro2::Span,
        scopes: &[ScopeFrame],
        endpoint: Option<&[super::model::Value]>,
        context: &RateLimitResolutionContext<'_>,
    ) -> syn::Result<NormalizedRateLimitRule> {
        let _ = declaration.cost_span;
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
                    self.resolve_rate_limit_value(
                        &public_name,
                        &public_name,
                        namespace,
                        name,
                        name.span(),
                        scopes,
                        endpoint,
                        context,
                        false,
                    )
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
        let (id, explicit_id, id_span) = declaration.id.as_ref().map_or_else(
            || {
                (
                    format!("{}.rate_limit", context.id_path),
                    false,
                    declaration_span,
                )
            },
            |id| (id.value.value(), true, id.value.span()),
        );
        Ok(NormalizedRateLimitRule {
            id,
            explicit_id,
            id_span,
            origin: context.lexical_origin.to_owned(),
            dimensions,
            cost: declaration.cost,
            windows,
        })
    }

    fn resolve_named_rate_limit(
        &mut self,
        attachment: &RateLimitUseDecl,
        scopes: &[ScopeFrame],
        endpoint: Option<&[super::model::Value]>,
        context: &RateLimitResolutionContext<'_>,
    ) -> syn::Result<NormalizedRateLimitRule> {
        let attachment_name = canonical_ident(&attachment.name);
        let Some(definition_index) = self
            .rate_limit_definitions
            .iter()
            .position(|definition| definition.name == attachment_name)
        else {
            return Err(attachment
                .name
                .span()
                .error(format!("unknown rate-limit profile `{attachment_name}`")));
        };
        self.rate_limit_definitions[definition_index].uses += 1;
        let definition = self.rate_limit_definitions[definition_index].clone();

        for binding in &attachment.bindings {
            let binding_name = canonical_ident(&binding.name);
            if matches!(binding_name.as_str(), "host" | "endpoint") {
                return Err(binding.name.span().error(format!(
                    "built-in rate-limit dimension `{binding_name}` cannot be bound"
                )));
            }
            if !definition.dimensions.iter().any(|dimension| {
                matches!(
                    dimension,
                    ResolvedDefinitionDimension::Formal { public_name, .. }
                        if public_name == &binding_name
                )
            }) {
                return Err(binding.name.span().error(format!(
                    "unknown binding `{binding_name}` for rate-limit profile `{attachment_name}`"
                )));
            }
        }

        let dimensions = definition
            .dimensions
            .iter()
            .map(|dimension| match dimension {
                ResolvedDefinitionDimension::Host { span } => {
                    Ok(NormalizedRateLimitDimension::Host { span: *span })
                }
                ResolvedDefinitionDimension::Endpoint { span } => {
                    Ok(NormalizedRateLimitDimension::Endpoint { span: *span })
                }
                ResolvedDefinitionDimension::Formal { public_name, span } => {
                    let Some(binding) = attachment
                        .bindings
                        .iter()
                        .find(|binding| canonical_ident(&binding.name) == *public_name)
                    else {
                        return Err(attachment.name.span().error(format!(
                            "rate-limit profile `{attachment_name}` requires a binding for dimension `{public_name}`"
                        )));
                    };
                    let source_name = format!(
                        "{}.{}",
                        canonical_ident(&binding.value.namespace),
                        canonical_ident(&binding.value.name)
                    );
                    let dimension = self.resolve_rate_limit_value(
                        public_name,
                        &source_name,
                        &binding.value.namespace,
                        &binding.value.name,
                        binding.value.span,
                        scopes,
                        endpoint,
                        context,
                        true,
                    )?;
                    let _ = span;
                    Ok(dimension)
                }
            })
            .collect::<syn::Result<Vec<_>>>()?;

        let cost = attachment
            .cost
            .as_ref()
            .map_or(definition.cost, |cost| cost.value);
        if definition
            .windows
            .iter()
            .any(|window| cost > window.capacity)
        {
            let span = attachment
                .cost
                .as_ref()
                .map_or(attachment.name.span(), |cost| cost.key_span);
            return Err(span.error(format!(
                "cost override for rate-limit profile `{attachment_name}` must not exceed any window capacity"
            )));
        }
        Ok(NormalizedRateLimitRule {
            id: definition.id,
            explicit_id: definition.explicit_id,
            id_span: definition.id_span,
            origin: format!(
                "rate-limit profile `{attachment_name}` attached at {}",
                context.lexical_origin
            ),
            dimensions,
            cost,
            windows: definition.windows,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_rate_limit_value(
        &self,
        logical_name: &str,
        source_name: &str,
        namespace: &syn::Ident,
        name: &syn::Ident,
        span: proc_macro2::Span,
        scopes: &[ScopeFrame],
        endpoint: Option<&[super::model::Value]>,
        context: &RateLimitResolutionContext<'_>,
        named_binding: bool,
    ) -> syn::Result<NormalizedRateLimitDimension> {
        if logical_name.is_empty() || logical_name.len() > MAX_RATE_LIMIT_KEY_PART_NAME_BYTES {
            return Err(span.error(format!(
                "a rate-limit key-part name permits 1-{MAX_RATE_LIMIT_KEY_PART_NAME_BYTES} UTF-8 bytes"
            )));
        }
        match (canonical_ident(namespace).as_str(), context.layer) {
            ("scope", RateLimitDeclarationLayer::Client) => {
                let owner = if named_binding {
                    "attachment"
                } else {
                    "declaration"
                };
                return Err(name.span().error(format!(
                    "`{source_name}` is unavailable in a client rate-limit {owner}"
                )));
            }
            ("ep", RateLimitDeclarationLayer::Client | RateLimitDeclarationLayer::Scope) => {
                let owner = if named_binding {
                    "attachment"
                } else {
                    "declaration"
                };
                return Err(name.span().error(format!(
                    "`{source_name}` is unavailable in a {} rate-limit {owner}",
                    context.layer.name()
                )));
            }
            _ => {}
        }
        let value = self.resolve_value_reference(namespace, name, scopes, endpoint)?;
        match &value.shape {
            ValueShape::Option { .. } | ValueShape::OptionVec { .. } => {
                let message = if named_binding {
                    format!(
                        "rate-limit dimension `{logical_name}` bound to `{source_name}` must be a required or defaulted scalar; optional values are unsupported"
                    )
                } else {
                    format!(
                        "rate-limit dimension `{logical_name}` must be a required or defaulted scalar; optional values are unsupported"
                    )
                };
                return Err(name.span().error(message));
            }
            ValueShape::Vec { .. } => {
                let message = if named_binding {
                    format!(
                        "rate-limit dimension `{logical_name}` bound to `{source_name}` must be a scalar; vector values are unsupported"
                    )
                } else {
                    format!(
                        "rate-limit dimension `{logical_name}` must be a scalar; vector values are unsupported"
                    )
                };
                return Err(name.span().error(message));
            }
            ValueShape::Scalar => {}
        }
        Ok(NormalizedRateLimitDimension::Value {
            public_name: logical_name.to_owned(),
            reference: value.reference,
            span,
        })
    }

    pub(super) fn validate_rate_limit_id_compatibility(
        &mut self,
        rule: &NormalizedRateLimitRule,
    ) -> syn::Result<()> {
        let dimensions = rule
            .dimensions
            .iter()
            .map(|dimension| match dimension {
                NormalizedRateLimitDimension::Host { .. } => "host".to_owned(),
                NormalizedRateLimitDimension::Endpoint { .. } => "endpoint".to_owned(),
                NormalizedRateLimitDimension::Value { public_name, .. } => public_name.clone(),
            })
            .collect::<Vec<_>>();
        let windows = rule
            .windows
            .iter()
            .map(|window| (window.capacity, window.duration))
            .collect::<Vec<_>>();
        if let Some((first_dimensions, first_windows, first_span)) =
            self.rate_limit_ids.get(&rule.id)
        {
            if *first_dimensions != dimensions || *first_windows != windows {
                let mut error = rule.id_span.error(format!(
                    "rate-limit ID `{}` is reused with incompatible ordered dimensions or windows",
                    rule.id
                ));
                error.combine(first_span.error("the first declaration using this rule ID is here"));
                return Err(error);
            }
        } else {
            self.rate_limit_ids
                .insert(rule.id.clone(), (dimensions, windows, rule.id_span));
        }
        Ok(())
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
    fn definition_registry_alone_does_not_request_a_runtime_limiter() {
        let source = crate::syntax::parse(quote! {
            client Api {
                base "https://example.test";
                rate_limits {
                    rule unused_here { 1 / 1s; }
                }
            }
            GET read() -> Bytes;
        })
        .expect("definition-only syntax");

        assert!(!super::source_tree_declares_rate_limits(
            &source.client.layer,
            &source.children
        ));
    }

    #[test]
    fn reusable_rule_owns_stable_identity_dimensions_and_attachment_cost() {
        let api = test_support::resolve_tokens(quote! {
            client ReusableApi {
                base "https://example.test";
                vars { tenant: String; }
                rate_limit use service_reads;
                rate_limits {
                    rule account_reads {
                        id "service.account.read";
                        by [host, endpoint, account];
                        100 / 1m;
                    }
                    rule service_reads {
                        10_000 / 1m;
                    }
                }
            }
            scope accounts(account: String) {
                rate_limit use account_reads {
                    account = scope.account;
                }
                GET read() -> Bytes;
            }
            GET lookup(account: String) -> Bytes {
                rate_limit use account_reads {
                    cost 2;
                    account = ep.account;
                }
            }
        });
        let scoped = test_support::endpoint(&api.client.children, "read");
        let root = test_support::endpoint(&api.client.children, "lookup");

        assert_eq!(scoped.config.rate_limits.len(), 2);
        assert_eq!(root.config.rate_limits.len(), 2);
        let scoped_rule = &scoped.config.rate_limits[1];
        let root_rule = &root.config.rate_limits[1];
        assert_eq!(scoped_rule.id, "service.account.read");
        assert_eq!(root_rule.id, "service.account.read");
        assert!(scoped_rule.explicit_id);
        assert_eq!(scoped_rule.cost, 1);
        assert_eq!(root_rule.cost, 2);
        assert_eq!(
            scoped_rule.origin,
            "rate-limit profile `account_reads` attached at scope accounts"
        );
        assert_eq!(
            root_rule.origin,
            "rate-limit profile `account_reads` attached at endpoint lookup"
        );
        assert!(matches!(
            scoped_rule.dimensions[0],
            NormalizedRateLimitDimension::Host { .. }
        ));
        assert!(matches!(
            scoped_rule.dimensions[1],
            NormalizedRateLimitDimension::Endpoint { .. }
        ));
        assert!(matches!(
            &scoped_rule.dimensions[2],
            NormalizedRateLimitDimension::Value {
                public_name,
                reference: ValueReference::Scope {
                    scope_id: 0,
                    parameter: 0,
                },
                ..
            } if public_name == "account"
        ));
        assert!(matches!(
            &root_rule.dimensions[2],
            NormalizedRateLimitDimension::Value {
                public_name,
                reference: ValueReference::Endpoint(0),
                ..
            } if public_name == "account"
        ));
        assert_eq!(
            scoped.config.rate_limits[0].id,
            "reusable_api.rate_limit.service_reads"
        );
    }

    #[test]
    fn local_named_and_inline_attachments_preserve_source_order() {
        let api = test_support::resolve_tokens(quote! {
            client OrderedApi {
                base "https://example.test";
                rate_limits {
                    rule first { 10 / 1m; }
                    rule second { 20 / 1m; }
                }
            }
            GET read() -> Bytes {
                rate_limit use first;
                rate_limit { 5 / 1s; }
                rate_limit use second { cost 2; }
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
                "ordered_api.rate_limit.first",
                "ordered_api.read.rate_limit",
                "ordered_api.rate_limit.second",
            ]
        );
        assert_eq!(endpoint.config.rate_limits[2].cost, 2);
    }

    #[test]
    fn reusable_rule_registry_rejects_unknown_unused_and_invalid_bindings() {
        let unknown = resolve_error(quote! {
            client Api {
                base "https://example.test";
                rate_limit use missing;
            }
        });
        assert!(unknown.contains("unknown rate-limit profile `missing`"));

        let unused = resolve_error(quote! {
            client Api {
                base "https://example.test";
                rate_limits {
                    rule unused { 1 / 1s; }
                }
            }
            GET read() -> Bytes;
        });
        assert!(unused.contains("rate-limit profile `unused` is never attached"));

        let missing = resolve_error(quote! {
            client Api {
                base "https://example.test";
                rate_limits {
                    rule shared { by [account]; 1 / 1s; }
                }
                rate_limit use shared;
            }
        });
        assert!(missing.contains("requires a binding for dimension `account`"));

        let unknown_binding = resolve_error(quote! {
            client Api {
                base "https://example.test";
                vars { tenant: String; }
                rate_limits {
                    rule shared { by [account]; 1 / 1s; }
                }
                rate_limit use shared {
                    tenant = vars.tenant;
                }
            }
        });
        assert!(unknown_binding.contains("unknown binding `tenant`"));

        let builtin_binding = resolve_error(quote! {
            client Api {
                base "https://example.test";
                vars { tenant: String; }
                rate_limits {
                    rule shared { by [host, account]; 1 / 1s; }
                }
                rate_limit use shared {
                    host = vars.tenant;
                    account = vars.tenant;
                }
            }
        });
        assert!(builtin_binding.contains("built-in rate-limit dimension `host` cannot be bound"));
    }

    #[test]
    fn reusable_bindings_enforce_visibility_shape_pagination_and_cost() {
        let client_scope = resolve_error(quote! {
            client Api {
                base "https://example.test";
                rate_limits {
                    rule shared { by [account]; 1 / 1s; }
                }
                rate_limit use shared {
                    account = scope.account;
                }
            }
            scope accounts(account: String) { GET read() -> Bytes; }
        });
        assert!(client_scope.contains("`scope.account` is unavailable in a client"));

        let scope_endpoint = resolve_error(quote! {
            client Api {
                base "https://example.test";
                rate_limits {
                    rule shared { by [account]; 1 / 1s; }
                }
            }
            scope accounts() {
                rate_limit use shared {
                    account = ep.account;
                }
                GET read(account: String) -> Bytes;
            }
        });
        assert!(scope_endpoint.contains("`ep.account` is unavailable in a scope"));

        let client_endpoint = resolve_error(quote! {
            client Api {
                base "https://example.test";
                rate_limits {
                    rule shared { by [account]; 1 / 1s; }
                }
                rate_limit use shared {
                    account = ep.account;
                }
            }
            GET read(account: String) -> Bytes;
        });
        assert!(client_endpoint.contains("`ep.account` is unavailable in a client"));

        let ambiguous_scope = resolve_error(quote! {
            client Api {
                base "https://example.test";
                rate_limits {
                    rule shared { by [account]; 1 / 1s; }
                }
            }
            scope outer(account: String) {
                scope inner(account: String) {
                    rate_limit use shared {
                        account = scope.account;
                    }
                    GET read() -> Bytes;
                }
            }
        });
        assert!(ambiguous_scope.contains("ambiguous visible scope value `account`"));

        for (ty, expected) in [
            (quote!(Option<String>), "optional values are unsupported"),
            (quote!(Vec<String>), "vector values are unsupported"),
        ] {
            let error = resolve_error(quote! {
                client Api {
                    base "https://example.test";
                    vars { account: #ty; }
                    rate_limits {
                        rule shared { by [account]; 1 / 1s; }
                    }
                    rate_limit use shared {
                        account = vars.account;
                    }
                }
            });
            assert!(error.contains(expected), "unexpected error: {error}");
        }

        let pagination = resolve_error(quote! {
            client Api {
                base "https://example.test";
                rate_limits {
                    rule shared { by [account]; 1 / 1s; }
                }
            }
            GET list(account: String) -> crate::PageSpec {
                rate_limit use shared {
                    account = ep.account;
                }
                paginate crate::Cursor {
                    account: bind ep.account,
                }
            }
        });
        assert!(
            pagination.contains(
                "pagination-controlled value `account` cannot partition a rate-limit rule"
            )
        );

        let cost = resolve_error(quote! {
            client Api {
                base "https://example.test";
                rate_limits {
                    rule shared { 2 / 1s; }
                }
                rate_limit use shared { cost 3; }
            }
        });
        assert!(cost.contains("cost override"));
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
    fn explicit_ids_are_exact_and_stable_across_source_renames() {
        let original = test_support::resolve_tokens(quote! {
            client OriginalApi {
                base "https://example.test";
            }
            scope old_branch() {
                GET old_endpoint() -> Bytes {
                    rate_limit {
                        id "riot.application";
                        1 / 1s;
                    }
                }
            }
        });
        let renamed = test_support::resolve_tokens(quote! {
            client RenamedApi {
                base "https://example.test";
            }
            scope new_branch() {
                GET new_endpoint() -> Bytes {
                    rate_limit {
                        id "riot.application";
                        1 / 1s;
                    }
                }
            }
        });
        let original = test_support::endpoint(&original.client.children, "old_endpoint");
        let renamed = test_support::endpoint(&renamed.client.children, "new_endpoint");

        assert_eq!(original.config.rate_limits[0].id, "riot.application");
        assert_eq!(renamed.config.rate_limits[0].id, "riot.application");
        assert!(original.config.rate_limits[0].explicit_id);
        assert!(renamed.config.rate_limits[0].explicit_id);
    }

    #[test]
    fn disjoint_explicit_id_reuse_accepts_identical_schema_and_different_costs() {
        let api = test_support::resolve_tokens(quote! {
            client SharedRateApi {
                base "https://example.test";
            }
            scope first() {
                rate_limit {
                    id "riot.shared";
                    by [host, endpoint];
                    cost 1;
                    10 / 1s;
                    100 / 1m;
                }
                GET read() -> Bytes;
            }
            scope second() {
                rate_limit {
                    cost 2;
                    by [host, endpoint];
                    id "riot.shared";
                    10 / 1s;
                    100 / 1m;
                }
                GET write() -> Bytes;
            }
        });
        let first = test_support::endpoint(&api.client.children, "read");
        let second = test_support::endpoint(&api.client.children, "write");
        assert_eq!(first.config.rate_limits[0].id, "riot.shared");
        assert_eq!(second.config.rate_limits[0].id, "riot.shared");
        assert_eq!(first.config.rate_limits[0].cost, 1);
        assert_eq!(second.config.rate_limits[0].cost, 2);
    }

    #[test]
    fn explicit_id_reuse_rejects_different_dimension_names_or_order() {
        for second_dimensions in [quote!(endpoint, host), quote!(host, vars.tenant)] {
            let error = resolve_error(quote! {
                client SharedRateApi {
                    base "https://example.test";
                    vars { tenant: String; }
                }
                scope first() {
                    rate_limit {
                        id "riot.shared";
                        by [host, endpoint];
                        10 / 1s;
                    }
                    GET read() -> Bytes;
                }
                scope second() {
                    rate_limit {
                        id "riot.shared";
                        by [#second_dimensions];
                        10 / 1s;
                    }
                    GET write() -> Bytes;
                }
            });
            assert!(error.contains(
                "rate-limit ID `riot.shared` is reused with incompatible ordered dimensions or windows"
            ));
        }
    }

    #[test]
    fn explicit_id_reuse_rejects_different_windows_or_window_order() {
        for second_windows in [quote!(10 / 1s; 101 / 1m;), quote!(100 / 1m; 10 / 1s;)] {
            let error = resolve_error(quote! {
                client SharedRateApi {
                    base "https://example.test";
                }
                scope first() {
                    rate_limit {
                        id "riot.shared";
                        10 / 1s;
                        100 / 1m;
                    }
                    GET read() -> Bytes;
                }
                scope second() {
                    rate_limit {
                        id "riot.shared";
                        #second_windows
                    }
                    GET write() -> Bytes;
                }
            });
            assert!(error.contains(
                "rate-limit ID `riot.shared` is reused with incompatible ordered dimensions or windows"
            ));
        }
    }

    #[test]
    fn explicit_id_cannot_incompatibly_reuse_a_disjoint_derived_id() {
        let error = resolve_error(quote! {
            client SharedRateApi {
                base "https://example.test";
            }
            scope first() {
                rate_limit {
                    10 / 1s;
                }
                GET read() -> Bytes;
            }
            scope second() {
                rate_limit {
                    id "shared_rate_api.first.rate_limit";
                    20 / 1s;
                }
                GET write() -> Bytes;
            }
        });

        assert!(error.contains(
            "rate-limit ID `shared_rate_api.first.rate_limit` is reused with incompatible ordered dimensions or windows"
        ));
    }

    #[test]
    fn repeated_explicit_id_in_one_effective_plan_is_rejected_first() {
        let error = resolve_error(quote! {
            client SharedRateApi {
                base "https://example.test";
                rate_limit {
                    id "riot.shared";
                    10 / 1s;
                }
            }
            GET read() -> Bytes {
                rate_limit {
                    id "riot.shared";
                    cost 2;
                    10 / 1s;
                }
            }
        });

        assert!(error.contains("an effective rate-limit plan contains a duplicate rule ID"));
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
