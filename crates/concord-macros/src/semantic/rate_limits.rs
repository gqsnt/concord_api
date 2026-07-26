use super::Resolver;
use super::SpanError;
use super::model::{
    NormalizedRateLimitDimension, NormalizedRateLimitRule, NormalizedRateLimitWindow,
    ValueReference, ValueShape,
};
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
        id_path: &str,
        origin: &str,
        scopes: &[ScopeFrame],
        endpoint: Option<&[super::model::Value]>,
        pagination_values: &[ValueReference],
        layer: RateLimitDeclarationLayer,
    ) -> syn::Result<NormalizedRateLimitRule> {
        let dimensions = declaration
            .dimensions
            .iter()
            .map(|dimension| match dimension {
                RateLimitDimensionDecl::Host { .. } => Ok(NormalizedRateLimitDimension::Host),
                RateLimitDimensionDecl::Endpoint { .. } => {
                    Ok(NormalizedRateLimitDimension::Endpoint)
                }
                RateLimitDimensionDecl::Value {
                    namespace, name, ..
                } => {
                    let public_name = dimension.public_name();
                    match (namespace.to_string().as_str(), layer) {
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
                                layer.name()
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
                    if matches!(value.reference, ValueReference::Endpoint(_))
                        && pagination_values.contains(&value.reference)
                    {
                        return Err(name.span().error(format!(
                            "pagination-controlled endpoint value `{public_name}` cannot be a rate-limit dimension"
                        )));
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
            id: format!("{id_path}.rate_limit"),
            origin: origin.to_owned(),
            dimensions,
            cost: declaration.cost,
            windows,
        })
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

        assert!(matches!(dimensions[0], NormalizedRateLimitDimension::Host));
        assert!(matches!(
            dimensions[1],
            NormalizedRateLimitDimension::Endpoint
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
    fn pagination_bound_dimension_is_rejected() {
        let error = resolve_error(quote! {
            client RateApi { base "https://example.test"; }
            GET list(cursor: String) -> crate::PageSpec {
                rate_limit { by [ep.cursor]; 1 / 1s; }
                paginate crate::Cursor {
                    cursor: bind ep.cursor,
                }
            }
        });

        assert!(error.contains("pagination-controlled endpoint value `ep.cursor`"));
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
