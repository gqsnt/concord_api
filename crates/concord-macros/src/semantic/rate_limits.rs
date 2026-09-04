use super::Resolver;
use super::SpanError;
use super::model::{
    NormalizedConfig, NormalizedRateLimitDimension, NormalizedRateLimitPolicy,
    NormalizedRateLimitWindow, RateLimitProfile, ResolvedExpr, ValuePresence, ValueReference,
};
use super::pagination::PaginationBoundValue;
use super::surface::canonical_ident;
use super::symbols::ScopeFrame;
use crate::syntax::ast::{
    AttachmentSite, RateLimitAttachmentDecl, RateLimitDecl, RateLimitDefinitionDecl,
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
    formals: Vec<String>,
    dimensions: Vec<ResolvedDefinitionDimension>,
    cost: u32,
    windows: Vec<NormalizedRateLimitWindow>,
    uses: usize,
}

#[derive(Clone)]
enum ResolvedDefinitionDimension {
    Origin {
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

impl ResolvedDefinitionDimension {
    fn public_name(&self) -> &str {
        match self {
            Self::Origin { .. } => "origin",
            Self::Endpoint { .. } => "endpoint",
            Self::Formal { public_name, .. } => public_name,
        }
    }
}

impl RateLimitDefinition {
    pub(super) fn profile(&self) -> RateLimitProfile {
        RateLimitProfile {
            name: self.name.clone(),
            id: self.id.clone(),
            explicit_id: self.explicit_id,
            formals: self.formals.clone(),
            dimensions: self
                .dimensions
                .iter()
                .map(|dimension| dimension.public_name().to_owned())
                .collect(),
            cost: self.cost,
            windows: self.windows.clone(),
        }
    }
}

pub(super) struct RateLimitResolutionContext<'a> {
    pub(super) lexical_origin: &'a str,
}

pub(super) fn source_tree_declares_rate_limits(
    attachment_site: &AttachmentSite,
    children: &[SourceChild],
) -> bool {
    let local = !attachment_site.rate_limits.is_empty();
    local
        || children.iter().any(|child| match child {
            SourceChild::Scope(scope) => {
                source_tree_declares_rate_limits(&scope.attachment_site, &scope.children)
            }
            SourceChild::Endpoint(endpoint) => !endpoint.attachment_site.rate_limits.is_empty(),
        })
}

pub(super) fn resolve_rate_limit_definitions(
    _client_name: &syn::Ident,
    definitions: Vec<RateLimitDefinitionDecl>,
) -> syn::Result<Vec<RateLimitDefinition>> {
    definitions
        .into_iter()
        .map(|definition| {
            let name = canonical_ident(&definition.name);
            let name_span = definition.name.span();
            let id = definition
                .id
                .as_ref()
                .expect("rate-policy parser requires a stable ID");
            let (id, explicit_id, id_span) = (id.value.value(), true, id.value.span());
            if id.len() > MAX_RATE_LIMIT_ID_BYTES {
                return Err(id_span.error(format!(
                    "a rate-limit policy ID permits at most {MAX_RATE_LIMIT_ID_BYTES} UTF-8 bytes"
                )));
            }
            let dimensions = definition
                .dimensions
                .iter()
                .map(|dimension| match dimension {
                    RateLimitDefinitionDimensionDecl::Origin { span } => {
                        ResolvedDefinitionDimension::Origin { span: *span }
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
                formals: definition.formals.iter().map(canonical_ident).collect(),
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
        _declaration_span: proc_macro2::Span,
        scopes: &[ScopeFrame],
        endpoint: Option<&[super::model::Value]>,
        context: &RateLimitResolutionContext<'_>,
    ) -> syn::Result<NormalizedRateLimitPolicy> {
        match attachment {
            RateLimitAttachmentDecl::Inline(declaration) => self.resolve_inline_rate_limit(
                declaration,
                _declaration_span,
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
        _declaration_span: proc_macro2::Span,
        scopes: &[ScopeFrame],
        endpoint: Option<&[super::model::Value]>,
        context: &RateLimitResolutionContext<'_>,
    ) -> syn::Result<NormalizedRateLimitPolicy> {
        let _ = declaration.cost_span;
        let dimensions = declaration
            .dimensions
            .iter()
            .map(|dimension| match dimension {
                RateLimitDimensionDecl::Origin { span } => {
                    Ok(NormalizedRateLimitDimension::Origin { span: *span })
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
        let id = declaration
            .id
            .as_ref()
            .expect("rate-policy parser requires a stable ID");
        let (id, explicit_id, id_span) = (id.value.value(), true, id.value.span());
        Ok(NormalizedRateLimitPolicy {
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
    ) -> syn::Result<NormalizedRateLimitPolicy> {
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
            if matches!(binding_name.as_str(), "origin" | "endpoint") {
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
                ResolvedDefinitionDimension::Origin { span } => {
                    Ok(NormalizedRateLimitDimension::Origin { span: *span })
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
        Ok(NormalizedRateLimitPolicy {
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
        let _ = context;
        let value = self.resolve_value_reference(namespace, name, scopes, endpoint)?;
        if matches!(value.presence, ValuePresence::Optional) {
            let message = if named_binding {
                format!(
                    "rate-limit dimension `{logical_name}` bound to `{source_name}` cannot use a Concord-optional slot"
                )
            } else {
                format!("rate-limit dimension `{logical_name}` cannot use a Concord-optional slot")
            };
            return Err(name.span().error(message));
        }
        Ok(NormalizedRateLimitDimension::Value {
            public_name: logical_name.to_owned(),
            reference: value.reference,
            span,
        })
    }

    pub(super) fn validate_rate_limit_id_compatibility(
        &mut self,
        rule: &NormalizedRateLimitPolicy,
    ) -> syn::Result<()> {
        let dimensions = rule
            .dimensions
            .iter()
            .map(|dimension| match dimension {
                NormalizedRateLimitDimension::Origin { .. } => "origin".to_owned(),
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
                error.combine(first_span.error("the first declaration via this policy ID is here"));
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
            .any(|dimension| matches!(dimension, NormalizedRateLimitDimension::Origin { .. }))
    });
    let host_dependencies = if uses_host {
        resolved_expression_dependencies(&config.host)
    } else {
        Vec::new()
    };

    for rule in &config.rate_limits {
        for dimension in &rule.dimensions {
            match dimension {
                NormalizedRateLimitDimension::Origin { span } => {
                    if let Some(pagination_value) = host_dependencies.iter().find_map(|reference| {
                        pagination_values
                            .iter()
                            .find(|value| value.reference == *reference)
                    }) {
                        return Err(span.error(format!(
                            "`origin` cannot partition this rate-limit policy because final origin shaping depends on pagination-controlled value `{}`",
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
                            "pagination-controlled value `{public_name}` cannot partition a rate-limit policy"
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
        ResolvedExpr::Concat(values) => {
            for value in values {
                collect_resolved_expression_dependencies(value, dependencies);
            }
        }
        ResolvedExpr::Adapted { value, .. } => {
            collect_resolved_expression_dependencies(value, dependencies);
        }
        ResolvedExpr::String(_)
        | ResolvedExpr::Character(_)
        | ResolvedExpr::Boolean(_)
        | ResolvedExpr::Integer { .. } => {}
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
                timeout 30s;
                base "https://example.test";
                rate_limits {
                    policy unused_here() { id "concord.rate_limits.1"; key [origin]; cost 1;  allow 1 per 1s; }
                }

            GET read() -> Bytes via Binary;
        }})
        .expect("definition-only syntax");

        assert!(!super::source_tree_declares_rate_limits(
            &source.client.attachment_site,
            &source.client.children
        ));
    }

    #[test]
    fn reusable_rule_owns_stable_identity_dimensions_and_attachment_cost() {
        let api = test_support::resolve_tokens(quote! {
            client ReusableApi {
                timeout 30s;
                base "https://example.test";
                values { tenant: String; }
                rate_limit service_reads();
                rate_limits {
                    policy account_reads(account) { cost 1;
                        id "service.account.read";
                        key [origin, endpoint, account];
                        allow 100 per 1m;
                    }
                    policy service_reads() { id "concord.rate_limits.3"; key [origin]; cost 1;
                        allow 10_000 per 1m;
                    }
                }

            scope accounts(account: String) {
                rate_limit account_reads(account = accounts.account);
                GET read() -> Bytes via Binary;
            }
            GET lookup(account: String) -> Bytes via Binary {
                rate_limit account_reads(account = endpoint.account) { cost 2; }
            }
        }});
        let scoped = test_support::endpoint(&api.client.children, "read");
        let root = test_support::endpoint(&api.client.children, "lookup");

        assert_eq!(api.client.rate_limit_profiles.len(), 2);
        let account_profile = &api.client.rate_limit_profiles[0];
        assert_eq!(account_profile.name, "account_reads");
        assert_eq!(account_profile.id, "service.account.read");
        assert_eq!(account_profile.formals, ["account"]);
        assert_eq!(
            account_profile.dimensions,
            ["origin", "endpoint", "account"]
        );
        assert_eq!(account_profile.cost, 1);
        assert_eq!(account_profile.windows.len(), 1);

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
            NormalizedRateLimitDimension::Origin { .. }
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
        assert_eq!(scoped.config.rate_limits[0].id, "concord.rate_limits.3");
    }

    #[test]
    fn local_named_and_inline_attachments_preserve_source_order() {
        let api = test_support::resolve_tokens(quote! {
            client OrderedApi {
                timeout 30s;
                base "https://example.test";
                rate_limits {
                    policy first() { id "concord.rate_limits.4"; key [origin]; cost 1;  allow 10 per 1m; }
                    policy second() { id "concord.rate_limits.5"; key [origin]; cost 1;  allow 20 per 1m; }
                }

            GET read() -> Bytes via Binary {
                rate_limit first();
                rate_limit { id "concord.rate_limits.6"; key [origin]; cost 1;  allow 5 per 1s; }
                rate_limit second() { cost 2; }
            }
        }});
        let endpoint = test_support::endpoint(&api.client.children, "read");
        assert_eq!(
            endpoint
                .config
                .rate_limits
                .iter()
                .map(|rule| rule.id.as_str())
                .collect::<Vec<_>>(),
            [
                "concord.rate_limits.4",
                "concord.rate_limits.6",
                "concord.rate_limits.5",
            ]
        );
        assert_eq!(endpoint.config.rate_limits[2].cost, 2);
    }

    #[test]
    fn reusable_rule_registry_rejects_unknown_unused_and_invalid_bindings() {
        let unknown = resolve_error(quote! {
            client Api {
                timeout 30s;
                base "https://example.test";
                rate_limit missing();
            }
        });
        assert!(unknown.contains("unknown rate-limit profile `missing`"));

        let unused = resolve_error(quote! {
            client Api {
                timeout 30s;
                base "https://example.test";
                rate_limits {
                    policy unused() { id "concord.rate_limits.7"; key [origin]; cost 1;  allow 1 per 1s; }
                }

            GET read() -> Bytes via Binary;
        }});
        assert!(unused.contains("rate-limit profile `unused` is never attached"));

        let missing = resolve_error(quote! {
            client Api {
                timeout 30s;
                base "https://example.test";
                rate_limits {
                    policy shared(account) { id "concord.rate_limits.8"; cost 1;  key [account]; allow 1 per 1s; }
                }
                rate_limit shared();
            }
        });
        assert!(missing.contains("requires a binding for dimension `account`"));

        let unknown_binding = resolve_error(quote! {
            client Api {
                timeout 30s;
                base "https://example.test";
                values { tenant: String; }
                rate_limits {
                    policy shared(account) { id "concord.rate_limits.9"; cost 1;  key [account]; allow 1 per 1s; }
                }
                rate_limit shared(tenant = client.tenant);
            }
        });
        assert!(unknown_binding.contains("unknown binding `tenant`"));

        let builtin_binding = resolve_error(quote! {
            client Api {
                timeout 30s;
                base "https://example.test";
                values { tenant: String; }
                rate_limits {
                    policy shared(account) { id "concord.rate_limits.10"; cost 1;  key [origin, account]; allow 1 per 1s; }
                }
                rate_limit shared(origin = client.tenant, account = client.tenant);
            }
        });
        assert!(builtin_binding.contains("built-in rate-limit dimension `origin` cannot be bound"));
    }

    #[test]
    fn reusable_bindings_enforce_visibility_shape_pagination_and_cost() {
        let client_scope = resolve_error(quote! {
            client Api {
                timeout 30s;
                base "https://example.test";
                rate_limits {
                    policy shared(account) { id "concord.rate_limits.11"; cost 1;  key [account]; allow 1 per 1s; }
                }
                rate_limit shared(account = accounts.account);

            scope accounts(account: String) { GET read() -> Bytes via Binary; }
        }});
        assert!(client_scope.contains("unknown value owner `accounts`"));

        let scope_endpoint = resolve_error(quote! {
            client Api {
                timeout 30s;
                base "https://example.test";
                rate_limits {
                    policy shared(account) { id "concord.rate_limits.12"; cost 1;  key [account]; allow 1 per 1s; }
                }

            scope accounts() {
                rate_limit shared(account = endpoint.account);
                GET read(account: String) -> Bytes via Binary;
            }
        }});
        assert!(
            scope_endpoint.contains("`endpoint.NAME` is available only while shaping an endpoint")
        );

        let client_endpoint = resolve_error(quote! {
            client Api {
                timeout 30s;
                base "https://example.test";
                rate_limits {
                    policy shared(account) { id "concord.rate_limits.13"; cost 1;  key [account]; allow 1 per 1s; }
                }
                rate_limit shared(account = endpoint.account);

            GET read(account: String) -> Bytes via Binary;
        }});
        assert!(
            client_endpoint.contains("`endpoint.NAME` is available only while shaping an endpoint")
        );

        let ambiguous_scope = resolve_error(quote! {
            client Api {
                timeout 30s;
                base "https://example.test";
                rate_limits {
                    policy shared(account) { id "concord.rate_limits.14"; cost 1;  key [account]; allow 1 per 1s; }
                }

            scope branch(account: String) {
                scope branch(account: String) {
                    rate_limit shared(account = branch.account);
                    GET read() -> Bytes via Binary;
                }
            }
        }});
        assert!(ambiguous_scope.contains("scope identifier `branch` duplicates an ancestor"));

        let optional = resolve_error(quote! {
            client Api {
                timeout 30s;
                base "https://example.test";
                values { account?: String; }
                rate_limits {
                    policy shared(account) { id "concord.rate_limits.15"; cost 1;  key [account]; allow 1 per 1s; }
                }
                rate_limit shared(account = client.account);
            }
        });
        assert!(optional.contains("cannot use a Concord-optional slot"));

        let pagination = resolve_error(quote! {
            client Api {
                timeout 30s;
                base "https://example.test";
                rate_limits {
                    policy shared(account) { id "concord.rate_limits.16"; cost 1;  key [account]; allow 1 per 1s; }
                }

            GET list(account: String) -> Vec<u32> via crate::PageSpec {
                rate_limit shared(account = endpoint.account);
                paginate via crate::Cursor {
                    account -> endpoint.account;
                }
            }
        }});
        assert!(pagination.contains(
            "pagination-controlled value `account` cannot partition a rate-limit policy"
        ));

        let cost = resolve_error(quote! {
            client Api {
                timeout 30s;
                base "https://example.test";
                rate_limits {
                    policy shared() { id "concord.rate_limits.17"; key [origin]; cost 1;  allow 2 per 1s; }
                }
                rate_limit shared() { cost 3; }
            }
        });
        assert!(cost.contains("cost override"));
    }

    #[test]
    fn composes_rate_rules_in_lexical_order_with_contract_identity() {
        let api = test_support::resolve_tokens(quote! {
            client RateApi {
                timeout 30s;
                base "https://example.test";
                rate_limit { id "concord.rate_limits.18"; key [origin]; cost 1;  allow 100 per 1m; }

            scope resources() {
                rate_limit { id "concord.rate_limits.19"; key [origin];  cost 2; allow 20 per 1s; }
                GET read() -> Bytes via Binary {
                    id "stable.read";
                    rate_limit { id "concord.rate_limits.20"; key [origin]; cost 1;  allow 5 per 1s; }
                }
            }
        }});
        let endpoint = test_support::endpoint(&api.client.children, "read");

        assert_eq!(
            endpoint
                .config
                .rate_limits
                .iter()
                .map(|rule| rule.id.as_str())
                .collect::<Vec<_>>(),
            [
                "concord.rate_limits.18",
                "concord.rate_limits.19",
                "concord.rate_limits.20",
            ]
        );
        assert_eq!(endpoint.config.rate_limits[1].cost, 2);
        assert!(endpoint.config.rate_limits.iter().all(|rule| matches!(
            rule.dimensions.as_slice(),
            [NormalizedRateLimitDimension::Origin { .. }]
        )));
    }

    #[test]
    fn explicit_ids_are_exact_and_stable_across_source_renames() {
        let original = test_support::resolve_tokens(quote! {
            client OriginalApi {
                timeout 30s;
                base "https://example.test";

            scope old_branch() {
                GET old_endpoint() -> Bytes via Binary {
                    rate_limit { key [origin]; cost 1;
                        id "riot.application";
                        allow 1 per 1s;
                    }
                }
            }
        }});
        let renamed = test_support::resolve_tokens(quote! {
            client RenamedApi {
                timeout 30s;
                base "https://example.test";

            scope new_branch() {
                GET new_endpoint() -> Bytes via Binary {
                    rate_limit { key [origin]; cost 1;
                        id "riot.application";
                        allow 1 per 1s;
                    }
                }
            }
        }});
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
                timeout 30s;
                base "https://example.test";

            scope first() {
                rate_limit {
                    id "riot.shared";
                    key [origin, endpoint];
                    cost 1;
                    allow 10 per 1s;
                    allow 100 per 1m;
                }
                GET read() -> Bytes via Binary;
            }
            scope second() {
                rate_limit {
                    cost 2;
                    key [origin, endpoint];
                    id "riot.shared";
                    allow 10 per 1s;
                    allow 100 per 1m;
                }
                GET write() -> Bytes via Binary;
            }
        }});
        let first = test_support::endpoint(&api.client.children, "read");
        let second = test_support::endpoint(&api.client.children, "write");
        assert_eq!(first.config.rate_limits[0].id, "riot.shared");
        assert_eq!(second.config.rate_limits[0].id, "riot.shared");
        assert_eq!(first.config.rate_limits[0].cost, 1);
        assert_eq!(second.config.rate_limits[0].cost, 2);
    }

    #[test]
    fn explicit_id_reuse_rejects_different_dimension_names_or_order() {
        for second_dimensions in [quote!(endpoint, origin), quote!(origin, client.tenant)] {
            let error = resolve_error(quote! {
                client SharedRateApi {
                    timeout 30s;
                    base "https://example.test";
                    values { tenant: String; }

                scope first() {
                    rate_limit { cost 1;
                        id "riot.shared";
                        key [origin, endpoint];
                        allow 10 per 1s;
                    }
                    GET read() -> Bytes via Binary;
                }
                scope second() {
                    rate_limit { cost 1;
                        id "riot.shared";
                        key [#second_dimensions];
                        allow 10 per 1s;
                    }
                    GET write() -> Bytes via Binary;
                }
            }});
            assert!(error.contains(
                "rate-limit ID `riot.shared` is reused with incompatible ordered dimensions or windows"
            ));
        }
    }

    #[test]
    fn explicit_id_reuse_rejects_different_windows_or_window_order() {
        for second_windows in [
            quote!(allow 10 per 1s; allow 101 per 1m;),
            quote!(allow 100 per 1m; allow 10 per 1s;),
        ] {
            let error = resolve_error(quote! {
                client SharedRateApi {
                    timeout 30s;
                    base "https://example.test";

                scope first() {
                    rate_limit { key [origin]; cost 1;
                        id "riot.shared";
                        allow 10 per 1s;
                        allow 100 per 1m;
                    }
                    GET read() -> Bytes via Binary;
                }
                scope second() {
                    rate_limit { key [origin]; cost 1;
                        id "riot.shared";
                        #second_windows
                    }
                    GET write() -> Bytes via Binary;
                }
            }});
            assert!(error.contains(
                "rate-limit ID `riot.shared` is reused with incompatible ordered dimensions or windows"
            ));
        }
    }

    #[test]
    fn explicit_id_cannot_incompatibly_reuse_a_disjoint_explicit_id() {
        let error = resolve_error(quote! {
            client SharedRateApi {
                timeout 30s;
                base "https://example.test";

            scope first() {
                rate_limit { id "shared_rate_api.first.rate_limit"; key [origin]; cost 1;
                    allow 10 per 1s;
                }
                GET read() -> Bytes via Binary;
            }
            scope second() {
                rate_limit { key [origin]; cost 1;
                    id "shared_rate_api.first.rate_limit";
                    allow 20 per 1s;
                }
                GET write() -> Bytes via Binary;
            }
        }});

        assert!(error.contains(
            "rate-limit ID `shared_rate_api.first.rate_limit` is reused with incompatible ordered dimensions or windows"
        ));
    }

    #[test]
    fn repeated_explicit_id_in_one_effective_plan_is_rejected_first() {
        let error = resolve_error(quote! {
            client SharedRateApi {
                timeout 30s;
                base "https://example.test";
                rate_limit { key [origin]; cost 1;
                    id "riot.shared";
                    allow 10 per 1s;
                }

            GET read() -> Bytes via Binary {
                rate_limit { key [origin];
                    id "riot.shared";
                    cost 2;
                    allow 10 per 1s;
                }
            }
        }});

        assert!(error.contains("an effective rate-limit plan contains a duplicate policy ID"));
    }

    #[test]
    fn client_rule_rejects_scope_reference() {
        let error = resolve_error(quote! {
            client RateApi {
                timeout 30s;
                base "https://example.test";
                rate_limit { id "concord.rate_limits.33"; cost 1;  key [scope.region]; allow 1 per 1s; }

            scope regional(region: String) {
                GET read() -> Bytes via Binary;
            }
        }});

        assert!(error.contains("v1 `scope.NAME` was removed"));
    }

    #[test]
    fn client_rule_rejects_endpoint_reference() {
        let error = resolve_error(quote! {
            client RateApi {
                timeout 30s;
                base "https://example.test";
                rate_limit { id "concord.rate_limits.34"; cost 1;  key [endpoint.account]; allow 1 per 1s; }

            GET read(account: String) -> Bytes via Binary;
        }});

        assert!(error.contains("`endpoint.NAME` is available only while shaping an endpoint"));
    }

    #[test]
    fn scope_rule_accepts_visible_scope_value() {
        let api = test_support::resolve_tokens(quote! {
            client RateApi {
                timeout 30s;
                base "https://example.test";
                values { tenant: String; }

            scope regional(region: String, shard: u32 = 1) {
                rate_limit { id "concord.rate_limits.35"; cost 1;
                    key [origin, endpoint, client.tenant, regional.region, regional.shard];
                    allow 1 per 1s;
                }
                GET read() -> Bytes via Binary;
            }
        }});
        let endpoint = test_support::endpoint(&api.client.children, "read");
        let dimensions = &endpoint.config.rate_limits[0].dimensions;

        assert!(matches!(
            dimensions[0],
            NormalizedRateLimitDimension::Origin { .. }
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
            } if public_name == "client.tenant"
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
            } if public_name == "regional.region"
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
            } if public_name == "regional.shard"
        ));
    }

    #[test]
    fn scope_rule_rejects_endpoint_reference() {
        let error = resolve_error(quote! {
            client RateApi {
                timeout 30s; base "https://example.test";
            scope regional(region: String) {
                rate_limit { id "concord.rate_limits.36"; cost 1;  key [endpoint.account]; allow 1 per 1s; }
                GET read(account: String) -> Bytes via Binary;
            }
        }});

        assert!(error.contains("`endpoint.NAME` is available only while shaping an endpoint"));
    }

    #[test]
    fn endpoint_rule_accepts_client_scope_and_endpoint_values() {
        let api = test_support::resolve_tokens(quote! {
            client RateApi {
                timeout 30s;
                base "https://example.test";
                values { tenant: String; }

            scope regional(region: String) {
                GET read(account: String, weight: u32 = 1) -> Bytes via Binary {
                    rate_limit { id "concord.rate_limits.37"; cost 1;
                        key [client.tenant, regional.region, endpoint.account, endpoint.weight];
                        allow 1 per 1s;
                    }
                }
            }
        }});
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
            client RateApi {
                timeout 30s; base "https://example.test";
            scope branch(account: String) {
                scope branch(account: String) {
                    rate_limit { id "concord.rate_limits.38"; cost 1;  key [branch.account]; allow 1 per 1s; }
                    GET read() -> Bytes via Binary;
                }
            }
        }});

        assert!(error.contains("scope identifier `branch` duplicates an ancestor"));
    }

    #[test]
    fn optional_dimension_is_rejected() {
        let error = resolve_error(quote! {
            client RateApi {
                timeout 30s;
                base "https://example.test";
                values { tenant?: String; }
                rate_limit { id "concord.rate_limits.39"; cost 1;  key [client.tenant]; allow 1 per 1s; }
            }
        });

        assert!(error.contains("cannot use a Concord-optional slot"));
    }

    #[test]
    fn vector_payload_is_not_classified_by_the_macro() {
        let api = test_support::resolve_tokens(quote! {
            client RateApi {
                timeout 30s;
                base "https://example.test";
                values { tenants: Vec<String>; }
                rate_limit { id "concord.rate_limits.40"; cost 1;  key [client.tenants]; allow 1 per 1s; }
            }
        });

        assert_eq!(api.client.config.rate_limits.len(), 1);
    }

    #[test]
    fn endpoint_rule_rejects_pagination_bound_endpoint_value() {
        let error = resolve_error(quote! {
            client RateApi {
                timeout 30s; base "https://example.test";
            GET list(cursor: String) -> Vec<u32> via crate::PageSpec {
                rate_limit { id "concord.rate_limits.41"; cost 1;  key [endpoint.cursor]; allow 1 per 1s; }
                paginate via crate::Cursor {
                    cursor -> endpoint.cursor;
                }
            }
        }});

        assert!(error.contains(
            "pagination-controlled value `endpoint.cursor` cannot partition a rate-limit policy"
        ));
    }

    #[test]
    fn endpoint_rule_rejects_pagination_bound_scope_value() {
        let error = resolve_error(quote! {
            client RateApi {
                timeout 30s; base "https://example.test";
            scope pages(cursor: String) {
                GET list() -> Vec<u32> via crate::PageSpec {
                    rate_limit { id "concord.rate_limits.42"; cost 1;  key [pages.cursor]; allow 1 per 1s; }
                    paginate via crate::Cursor {
                        cursor -> pages.cursor;
                    }
                }
            }
        }});

        assert!(error.contains("live pagination binding may update only an endpoint-owned slot"));
    }

    #[test]
    fn endpoint_rule_rejects_pagination_bound_client_value() {
        let error = resolve_error(quote! {
            client RateApi {
                timeout 30s;
                base "https://example.test";
                values { cursor: String; }

            GET list() -> Vec<u32> via crate::PageSpec {
                rate_limit { id "concord.rate_limits.43"; cost 1;  key [client.cursor]; allow 1 per 1s; }
                paginate via crate::Cursor {
                    cursor -> client.cursor;
                }
            }
        }});

        assert!(error.contains("live pagination binding may update only an endpoint-owned slot"));
    }

    #[test]
    fn inherited_scope_rule_rejects_pagination_bound_scope_value() {
        let error = resolve_error(quote! {
            client RateApi {
                timeout 30s; base "https://example.test";
            scope pages(cursor: String) {
                rate_limit { id "concord.rate_limits.44"; cost 1;  key [pages.cursor]; allow 1 per 1s; }
                GET list() -> Vec<u32> via crate::PageSpec {
                    paginate via crate::Cursor {
                        cursor -> pages.cursor;
                    }
                }
            }
        }});

        assert!(error.contains("live pagination binding may update only an endpoint-owned slot"));
    }

    #[test]
    fn inherited_client_rule_rejects_pagination_bound_client_value() {
        let error = resolve_error(quote! {
            client RateApi {
                timeout 30s;
                base "https://example.test";
                values { cursor: String; }
                rate_limit { id "concord.rate_limits.45"; cost 1;  key [client.cursor]; allow 1 per 1s; }

            GET list() -> Vec<u32> via crate::PageSpec {
                paginate via crate::Cursor {
                    cursor -> client.cursor;
                }
            }
        }});

        assert!(error.contains("live pagination binding may update only an endpoint-owned slot"));
    }

    #[test]
    fn unrelated_pagination_leaves_client_scope_and_endpoint_dimensions_valid() {
        let api = test_support::resolve_tokens(quote! {
            client RateApi {
                timeout 30s;
                base "https://example.test";
                values {
                    tenant: String;
                }

            scope pages(region: String) {
                GET list(account: String, cursor: String) -> Vec<u32> via crate::PageSpec {
                    rate_limit { id "concord.rate_limits.46"; cost 1;
                        key [client.tenant, pages.region, endpoint.account];
                        allow 1 per 1s;
                    }
                    paginate via crate::Cursor {
                        endpoint_cursor -> endpoint.cursor;
                    }
                }
            }
        }});
        let endpoint = test_support::endpoint(&api.client.children, "list");

        assert_eq!(endpoint.config.rate_limits[0].dimensions.len(), 3);
    }

    #[test]
    fn host_dimension_accepts_static_host_with_unrelated_pagination() {
        let api = test_support::resolve_tokens(quote! {
            client RateApi {
                timeout 30s;
                base "https://example.test";
                rate_limit { id "concord.rate_limits.47"; cost 1;  key [origin]; allow 1 per 1s; }

            GET list(cursor: String) -> Vec<u32> via crate::PageSpec {
                paginate via crate::Cursor {
                    cursor -> endpoint.cursor;
                }
            }
        }});

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
            client RateApi {
                timeout 30s; base "https://example.test";
            scope pages(region: String) {
                subdomain [pages.region, "api"];
                rate_limit { id "concord.rate_limits.48"; cost 1;  key [origin]; allow 1 per 1s; }
                GET list(cursor: String) -> Vec<u32> via crate::PageSpec {
                    paginate via crate::Cursor {
                        cursor -> endpoint.cursor;
                    }
                }
            }
        }});

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
                timeout 30s;
                base "https://example.test";
                values { region: String; }
                subdomain [client.region, "api"];
                rate_limit { id "concord.rate_limits.49"; cost 1;  key [origin]; allow 1 per 1s; }

            GET list() -> Vec<u32> via crate::PageSpec {
                paginate via crate::Cursor {
                    region -> client.region;
                }
            }
        }});

        assert!(error.contains("live pagination binding may update only an endpoint-owned slot"));
    }

    #[test]
    fn host_dimension_rejects_pagination_bound_scope_host_input() {
        let error = resolve_error(quote! {
            client RateApi {
                timeout 30s; base "https://example.test";
            scope pages(region: String) {
                subdomain [pages.region, "api"];
                rate_limit { id "concord.rate_limits.50"; cost 1;  key [origin]; allow 1 per 1s; }
                GET list() -> Vec<u32> via crate::PageSpec {
                    paginate via crate::Cursor {
                        region -> pages.region;
                    }
                }
            }
        }});

        assert!(error.contains("live pagination binding may update only an endpoint-owned slot"));
    }

    #[test]
    fn host_dimension_rejects_pagination_bound_endpoint_host_input() {
        let error = resolve_error(quote! {
            client RateApi {
                timeout 30s; base "https://example.test";
            GET list(region: String) -> Vec<u32> via crate::PageSpec {
                subdomain [endpoint.region, "api"];
                rate_limit { id "concord.rate_limits.51"; cost 1;  key [origin]; allow 1 per 1s; }
                paginate via crate::Cursor {
                    region -> endpoint.region;
                }
            }
        }});

        assert!(error.contains(
            "`origin` cannot partition this rate-limit policy because final origin shaping depends on pagination-controlled value `endpoint.region`"
        ));
    }

    #[test]
    fn formatted_host_dependency_is_detected_recursively() {
        let error = resolve_error(quote! {
            client RateApi {
                timeout 30s; base "https://example.test";
            scope pages() {
                rate_limit { id "concord.rate_limits.52"; cost 1;  key [origin]; allow 1 per 1s; }
                GET list(region: String) -> Vec<u32> via crate::PageSpec {
                    subdomain [concat["edge-", concat["regional-", endpoint.region]]];
                    paginate via crate::Cursor {
                        region -> endpoint.region;
                    }
                }
            }
        }});

        assert!(error.contains("pagination-controlled value `endpoint.region`"));
    }

    #[test]
    fn inherited_host_dimension_is_revalidated_at_endpoint() {
        let error = resolve_error(quote! {
            client RateApi {
                timeout 30s;
                base "https://example.test";
                rate_limit { id "concord.rate_limits.53"; cost 1;  key [origin]; allow 1 per 1s; }

            scope pages() {
                GET list(region: String) -> Vec<u32> via crate::PageSpec {
                    subdomain [endpoint.region, "api"];
                    paginate via crate::Cursor {
                        region -> endpoint.region;
                    }
                }
            }
        }});

        assert!(error.contains(
            "`origin` cannot partition this rate-limit policy because final origin shaping depends on pagination-controlled value `endpoint.region`"
        ));
    }

    #[test]
    fn secret_dimension_is_rejected() {
        let error = resolve_error(quote! {
            client RateApi {
                timeout 30s;
                base "https://example.test";
                credentials {
                    secret token;
                    credential bearer = bearer(secret.token);
                }
                rate_limit { id "concord.rate_limits.54"; cost 1;  key [secret.token]; allow 1 per 1s; }
            }
        });

        assert!(error.contains("secrets and credentials cannot be rate-limit dimensions"));
    }
}
