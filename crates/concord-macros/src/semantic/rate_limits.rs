use super::Resolver;
use super::model::{NormalizedRateLimitRule, NormalizedRateLimitWindow};
use crate::syntax::ast::{Layer, RateLimitDecl, SourceChild};

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
    ) -> NormalizedRateLimitRule {
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
        NormalizedRateLimitRule {
            id: format!("{id_path}.rate_limit"),
            origin: origin.to_owned(),
            cost: declaration.cost,
            windows,
        }
    }
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::super::test_support;

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
    }
}
