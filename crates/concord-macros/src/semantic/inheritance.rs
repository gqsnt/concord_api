use super::auth::{find_credential, validate_auth_compatibility, validate_effective_auth};
use super::model::{
    CredentialSource, NormalizedAuth, NormalizedConfig, NormalizedHeader, NormalizedQuery,
    ResolvedAuthPlacement, ResolvedChallengePolicy, ResolvedExpr, Value, ValueShape,
};
use super::symbols::ScopeFrame;
use super::{Resolver, SpanError};
use crate::syntax::ast::{
    AuthChallenge as SourceChallenge, AuthOperation, AuthPlacementKind as SourceAuthPlacement,
    HeaderEntry, HeaderOperation, HostOperation, Layer, PathOperation, PublicExpr, QueryEntry,
    QueryOperation, TimeoutOperation,
};
use crate::{MAX_RATE_LIMIT_ID_BYTES, MAX_RATE_LIMIT_RULES_PER_PLAN};

#[derive(Clone, Copy)]
enum ExpressionUse {
    Single,
    Query,
    Header,
}

fn resolve_auth_placement(source: &SourceAuthPlacement) -> ResolvedAuthPlacement {
    match source {
        SourceAuthPlacement::Bearer => ResolvedAuthPlacement::Bearer,
        SourceAuthPlacement::Basic => ResolvedAuthPlacement::Basic,
        SourceAuthPlacement::Header(name) => ResolvedAuthPlacement::Header(name.clone()),
        SourceAuthPlacement::Query(name) => ResolvedAuthPlacement::Query(name.clone()),
    }
}

fn resolve_challenge(source: SourceChallenge) -> ResolvedChallengePolicy {
    match source {
        SourceChallenge::Unauthorized => ResolvedChallengePolicy::Unauthorized,
        SourceChallenge::UnauthorizedOrForbidden => {
            ResolvedChallengePolicy::UnauthorizedOrForbidden
        }
        SourceChallenge::Never => ResolvedChallengePolicy::Never,
    }
}

impl Resolver<'_> {
    pub(super) fn normalize_layer(
        &self,
        layer: &Layer,
        parent: &NormalizedConfig,
        scopes: &[ScopeFrame],
        endpoint: Option<&[Value]>,
        rate_id_path: &str,
        rate_lexical_origin: &str,
    ) -> syn::Result<NormalizedConfig> {
        let mut normalized = parent.clone();

        if let Some(operation) = &layer.host {
            let operation = &operation.value;
            normalized.host = match operation {
                HostOperation::Off => Vec::new(),
                HostOperation::Replace(values) => {
                    self.resolve_expressions(values, scopes, endpoint, ExpressionUse::Single)?
                }
            };
        }
        if let Some(operation) = &layer.path {
            let operation = &operation.value;
            match operation {
                PathOperation::Off => normalized.path.clear(),
                PathOperation::Replace(values) => {
                    normalized.path =
                        self.resolve_expressions(values, scopes, endpoint, ExpressionUse::Single)?;
                }
                PathOperation::Append(values) => {
                    normalized.path.extend(self.resolve_expressions(
                        values,
                        scopes,
                        endpoint,
                        ExpressionUse::Single,
                    )?);
                }
            }
        }
        if let Some(operation) = &layer.query {
            let operation = &operation.value;
            if matches!(operation, QueryOperation::Off | QueryOperation::Replace(_)) {
                normalized.query.clear();
            }
            if let QueryOperation::Modify(entries) | QueryOperation::Replace(entries) = operation {
                for entry in entries {
                    let key = match entry {
                        QueryEntry::Assign { key, .. } | QueryEntry::Remove(key) => key.value(),
                    };
                    normalized.query.retain(|entry| entry.key.value() != key);
                    if let QueryEntry::Assign { key, value } = entry {
                        normalized.query.push(NormalizedQuery {
                            key: key.clone(),
                            value: self.resolve_expr(
                                value,
                                scopes,
                                endpoint,
                                ExpressionUse::Query,
                            )?,
                        });
                    }
                }
            }
        }
        if let Some(operation) = &layer.headers {
            let operation = &operation.value;
            if matches!(
                operation,
                HeaderOperation::Off | HeaderOperation::Replace(_)
            ) {
                normalized.headers.clear();
            }
            if let HeaderOperation::Modify(entries) | HeaderOperation::Replace(entries) = operation
            {
                for entry in entries {
                    let name = match entry {
                        HeaderEntry::Assign { name, .. } | HeaderEntry::Remove(name) => {
                            name.value().to_ascii_lowercase()
                        }
                    };
                    normalized
                        .headers
                        .retain(|entry| entry.canonical_name != name);
                    if let HeaderEntry::Assign {
                        name: literal,
                        value,
                    } = entry
                    {
                        normalized.headers.push(NormalizedHeader {
                            name: literal.clone(),
                            canonical_name: name,
                            value: self.resolve_expr(
                                value,
                                scopes,
                                endpoint,
                                ExpressionUse::Header,
                            )?,
                        });
                    }
                }
            }
        }
        if let Some(operation) = &layer.auth {
            match operation {
                AuthOperation::Off { .. } => normalized.auth.clear(),
                AuthOperation::Modify {
                    replace,
                    placements,
                    ..
                } => {
                    if *replace {
                        normalized.auth.clear();
                    }
                    for placement in placements {
                        let credential = find_credential(self.credentials, &placement.credential)?;
                        let source = &self.credentials[credential].source;
                        validate_auth_compatibility(
                            &placement.kind,
                            source,
                            &placement.credential,
                        )?;
                        let challenge = match placement.challenge {
                            Some(challenge) => challenge,
                            None => match source {
                                CredentialSource::Custom { .. } => {
                                    return Err(placement.credential.span().error(
                                        "an auth placement using a custom provider requires an explicit challenge suffix",
                                    ));
                                }
                                _ => SourceChallenge::Never,
                            },
                        };
                        normalized.auth.push(NormalizedAuth {
                            kind: resolve_auth_placement(&placement.kind),
                            credential,
                            challenge: resolve_challenge(challenge),
                        });
                    }
                }
            }
        }
        validate_effective_auth(&normalized)?;
        if let Some(operation) = &layer.timeout {
            let operation = &operation.value;
            normalized.timeout = match operation {
                TimeoutOperation::Off => None,
                TimeoutOperation::Set(value) => {
                    let _ = value.span;
                    Some(value.value)
                }
            };
        }
        if let Some(declaration) = &layer.rate_limit {
            if normalized.rate_limits.len() >= MAX_RATE_LIMIT_RULES_PER_PLAN {
                return Err(declaration.key_span.error(format!(
                    "an effective rate-limit plan permits at most {MAX_RATE_LIMIT_RULES_PER_PLAN} rules"
                )));
            }
            let rule =
                self.resolve_rate_limit(&declaration.value, rate_id_path, rate_lexical_origin);
            if rule.id.len() > MAX_RATE_LIMIT_ID_BYTES {
                return Err(declaration.key_span.error(format!(
                    "a rate-limit rule ID permits at most {MAX_RATE_LIMIT_ID_BYTES} UTF-8 bytes"
                )));
            }
            if normalized
                .rate_limits
                .iter()
                .any(|existing| existing.id == rule.id)
            {
                return Err(declaration
                    .key_span
                    .error("an effective rate-limit plan contains a duplicate rule ID"));
            }
            normalized.rate_limits.push(rule);
        }
        Ok(normalized)
    }

    fn resolve_expressions(
        &self,
        values: &[PublicExpr],
        scopes: &[ScopeFrame],
        endpoint: Option<&[Value]>,
        usage: ExpressionUse,
    ) -> syn::Result<Vec<ResolvedExpr>> {
        values
            .iter()
            .map(|value| self.resolve_expr(value, scopes, endpoint, usage))
            .collect()
    }

    fn resolve_expr(
        &self,
        value: &PublicExpr,
        scopes: &[ScopeFrame],
        endpoint: Option<&[Value]>,
        usage: ExpressionUse,
    ) -> syn::Result<ResolvedExpr> {
        match value {
            PublicExpr::String(value) => Ok(ResolvedExpr::String(value.clone())),
            PublicExpr::Integer(value) => Ok(ResolvedExpr::Integer(value.clone())),
            PublicExpr::Format { span, values } => {
                let _ = span;
                Ok(ResolvedExpr::Format(self.resolve_expressions(
                    values,
                    scopes,
                    endpoint,
                    ExpressionUse::Single,
                )?))
            }
            PublicExpr::Reference { namespace, name } => {
                let reference = self.resolve_value_reference(namespace, name, scopes, endpoint)?;
                match usage {
                    ExpressionUse::Single if !matches!(&reference.shape, ValueShape::Scalar) => {
                        return Err(name.span().error(
                            "host, path, and formatted expressions require one scalar value",
                        ));
                    }
                    ExpressionUse::Header
                        if matches!(
                            &reference.shape,
                            ValueShape::Vec { .. } | ValueShape::OptionVec { .. }
                        ) =>
                    {
                        return Err(name
                            .span()
                            .error("public header declarations do not accept vector values"));
                    }
                    ExpressionUse::Single | ExpressionUse::Query | ExpressionUse::Header => {}
                }
                Ok(ResolvedExpr::Value(reference))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::super::test_support;
    use super::*;

    #[test]
    fn resolves_lexical_operations_into_one_normalized_public_head() {
        let api = test_support::resolve_tokens(quote! {
            client InheritanceApi {
                base "https://example.test";
                path ["v1"];
                query { "client" = "yes"; }
            }
            scope resources() {
                path ["resources"];
                query { "scope" = "yes"; }
                GET read() -> Bytes {
                    path ["read"];
                    query { remove "client"; "endpoint" = "yes"; }
                }
            }
        });
        let endpoint = test_support::endpoint(&api.client.children, "read");

        assert_eq!(
            route_shape_for_test(&endpoint.config.path),
            ["v1", "resources", "read"]
        );
        assert_eq!(
            endpoint
                .config
                .query
                .iter()
                .map(|entry| entry.key.value())
                .collect::<Vec<_>>(),
            ["scope", "endpoint"]
        );
    }

    fn route_shape_for_test(path: &[ResolvedExpr]) -> Vec<String> {
        path.iter()
            .map(|part| match part {
                ResolvedExpr::String(value) => value.value(),
                _ => panic!("static test route"),
            })
            .collect()
    }
}
