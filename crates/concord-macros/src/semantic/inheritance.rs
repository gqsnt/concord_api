use super::auth::{find_credential, validate_auth_compatibility, validate_effective_auth};
use super::model::{
    AttachmentFamily, AttachmentIr, AttachmentOperation, NormalizedAuth, NormalizedConfig,
    NormalizedHeader, NormalizedQuery, QueryCardinality, ResolvedAuthPlacement,
    ResolvedChallengePolicy, ResolvedExpr, Value, ValuePresence,
};
use super::symbols::ScopeFrame;
use super::{Resolver, SpanError};
use crate::semantic::rate_limits::RateLimitResolutionContext;
use crate::syntax::ast::{
    AttachmentSite, AuthChallenge as SourceChallenge, AuthOperation,
    AuthPlacementKind as SourceAuthPlacement, HeaderEntry, HeaderOperation, PathOperation,
    PublicExpr, QueryEmission, QueryEntry, QueryOperation, SubdomainOperation, TimeoutOperation,
};
use crate::{MAX_RATE_LIMIT_ID_BYTES, MAX_RATE_LIMIT_POLICIES_PER_PLAN};

#[derive(Clone, Copy)]
pub(super) enum ExpressionUse {
    Single,
    Query,
    Header,
}

pub(super) fn local_attachment_ir(site: &AttachmentSite, origin: &str) -> Vec<AttachmentIr> {
    let mut output = Vec::new();
    let mut push = |family, operation, target: Option<String>, item_count| {
        output.push(AttachmentIr {
            family,
            operation,
            origin: origin.to_owned(),
            target,
            item_count,
        });
    };

    if let Some(clause) = &site.subdomain {
        match &clause.value {
            SubdomainOperation::Prepend(values) => push(
                AttachmentFamily::Subdomain,
                AttachmentOperation::Prepend,
                None,
                values.len(),
            ),
            SubdomainOperation::Replace(values) => push(
                AttachmentFamily::Subdomain,
                AttachmentOperation::Replace,
                None,
                values.len(),
            ),
            SubdomainOperation::Off => push(
                AttachmentFamily::Subdomain,
                AttachmentOperation::Off,
                None,
                0,
            ),
        }
    }
    if let Some(clause) = &site.path {
        match &clause.value {
            PathOperation::Append(values) => push(
                AttachmentFamily::Path,
                AttachmentOperation::Append,
                None,
                values.len(),
            ),
            PathOperation::Replace(values) => push(
                AttachmentFamily::Path,
                AttachmentOperation::Replace,
                None,
                values.len(),
            ),
            PathOperation::Off => push(AttachmentFamily::Path, AttachmentOperation::Off, None, 0),
        }
    }
    if let Some(clause) = &site.query {
        let entries: &[QueryEntry] = match &clause.value {
            QueryOperation::Modify(entries) => entries,
            QueryOperation::Replace(entries) => {
                push(
                    AttachmentFamily::Query,
                    AttachmentOperation::Replace,
                    None,
                    0,
                );
                entries
            }
            QueryOperation::Off => {
                push(AttachmentFamily::Query, AttachmentOperation::Off, None, 0);
                &[]
            }
        };
        for entry in entries {
            match entry {
                QueryEntry::Assign { key, emission } => push(
                    AttachmentFamily::Query,
                    AttachmentOperation::Replace,
                    Some(key.value()),
                    usize::from(matches!(emission, QueryEmission::Each { .. })),
                ),
                QueryEntry::Append { key, emission } => push(
                    AttachmentFamily::Query,
                    AttachmentOperation::Append,
                    Some(key.value()),
                    usize::from(matches!(emission, QueryEmission::Each { .. })),
                ),
                QueryEntry::Remove(key) => push(
                    AttachmentFamily::Query,
                    AttachmentOperation::Remove,
                    Some(key.value()),
                    0,
                ),
            }
        }
    }
    if let Some(clause) = &site.headers {
        let entries: &[HeaderEntry] = match &clause.value {
            HeaderOperation::Modify(entries) => entries,
            HeaderOperation::Replace(entries) => {
                push(
                    AttachmentFamily::Headers,
                    AttachmentOperation::Replace,
                    None,
                    0,
                );
                entries
            }
            HeaderOperation::Off => {
                push(AttachmentFamily::Headers, AttachmentOperation::Off, None, 0);
                &[]
            }
        };
        for entry in entries {
            match entry {
                HeaderEntry::Assign { name, .. } => push(
                    AttachmentFamily::Headers,
                    AttachmentOperation::Replace,
                    Some(name.value()),
                    1,
                ),
                HeaderEntry::Remove(name) => push(
                    AttachmentFamily::Headers,
                    AttachmentOperation::Remove,
                    Some(name.value()),
                    0,
                ),
            }
        }
    }
    if let Some(auth) = &site.auth {
        match auth {
            AuthOperation::Off { .. } => push(
                AttachmentFamily::Authentication,
                AttachmentOperation::Off,
                None,
                0,
            ),
            AuthOperation::Modify {
                replace,
                placements,
                ..
            } => {
                if *replace {
                    push(
                        AttachmentFamily::Authentication,
                        AttachmentOperation::Replace,
                        None,
                        0,
                    );
                }
                for placement in placements {
                    let target = match &placement.kind {
                        SourceAuthPlacement::Bearer => "authorization:bearer".to_owned(),
                        SourceAuthPlacement::Basic => "authorization:basic".to_owned(),
                        SourceAuthPlacement::Authorization(scheme) => scheme.as_ref().map_or_else(
                            || "authorization:exact".to_owned(),
                            |value| format!("authorization:{}", value.value()),
                        ),
                        SourceAuthPlacement::Header(name) => format!("header:{}", name.value()),
                        SourceAuthPlacement::Query(name) => format!("query:{}", name.value()),
                    };
                    push(
                        AttachmentFamily::Authentication,
                        AttachmentOperation::Append,
                        Some(target),
                        1,
                    );
                }
            }
        }
    }
    if let Some(clause) = &site.timeout {
        push(
            AttachmentFamily::Timeout,
            match clause.value {
                TimeoutOperation::Set(_) => AttachmentOperation::Set,
                TimeoutOperation::Off => AttachmentOperation::Off,
            },
            None,
            1,
        );
    }
    for _ in &site.rate_limits {
        push(
            AttachmentFamily::RateLimit,
            AttachmentOperation::Append,
            None,
            1,
        );
    }
    output
}

fn resolve_auth_placement(source: &SourceAuthPlacement) -> ResolvedAuthPlacement {
    match source {
        SourceAuthPlacement::Bearer => ResolvedAuthPlacement::Bearer,
        SourceAuthPlacement::Basic => ResolvedAuthPlacement::Basic,
        SourceAuthPlacement::Authorization(scheme) => {
            ResolvedAuthPlacement::Authorization(scheme.clone())
        }
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
    pub(super) fn normalize_attachment_site(
        &mut self,
        attachment_site: &AttachmentSite,
        parent: &NormalizedConfig,
        scopes: &[ScopeFrame],
        endpoint: Option<&[Value]>,
        rate_limit_context: &RateLimitResolutionContext<'_>,
    ) -> syn::Result<NormalizedConfig> {
        let mut normalized = parent.clone();
        let origin = rate_limit_context.lexical_origin;

        if let Some(operation) = &attachment_site.subdomain {
            normalized.host = match &operation.value {
                SubdomainOperation::Off => {
                    normalized.host_span = None;
                    normalized.host_origins.clear();
                    Vec::new()
                }
                SubdomainOperation::Replace(values) => {
                    normalized.host_span = Some(operation.key_span);
                    let resolved =
                        self.resolve_expressions(values, scopes, endpoint, ExpressionUse::Single)?;
                    normalized.host_origins = vec![origin.to_owned(); resolved.len()];
                    resolved
                }
                SubdomainOperation::Prepend(values) => {
                    normalized.host_span = Some(operation.key_span);
                    let mut local =
                        self.resolve_expressions(values, scopes, endpoint, ExpressionUse::Single)?;
                    let mut local_origins = vec![origin.to_owned(); local.len()];
                    local_origins.append(&mut normalized.host_origins);
                    normalized.host_origins = local_origins;
                    local.extend(::std::mem::take(&mut normalized.host));
                    local
                }
            };
        }
        if let Some(operation) = &attachment_site.path {
            let operation = &operation.value;
            match operation {
                PathOperation::Off => {
                    normalized.path.clear();
                    normalized.path_origins.clear();
                }
                PathOperation::Replace(values) => {
                    normalized.path =
                        self.resolve_expressions(values, scopes, endpoint, ExpressionUse::Single)?;
                    normalized.path_origins = vec![origin.to_owned(); normalized.path.len()];
                }
                PathOperation::Append(values) => {
                    let resolved =
                        self.resolve_expressions(values, scopes, endpoint, ExpressionUse::Single)?;
                    normalized
                        .path_origins
                        .extend(vec![origin.to_owned(); resolved.len()]);
                    normalized.path.extend(resolved);
                }
            }
        }
        if let Some(operation) = &attachment_site.query {
            let operation = &operation.value;
            if matches!(operation, QueryOperation::Off | QueryOperation::Replace(_)) {
                normalized.query.clear();
            }
            if let QueryOperation::Modify(entries) | QueryOperation::Replace(entries) = operation {
                for entry in entries {
                    let key = match entry {
                        QueryEntry::Assign { key, .. }
                        | QueryEntry::Append { key, .. }
                        | QueryEntry::Remove(key) => key.value(),
                    };
                    if matches!(entry, QueryEntry::Assign { .. } | QueryEntry::Remove(_)) {
                        normalized.query.retain(|entry| entry.key.value() != key);
                    }
                    if let QueryEntry::Assign { key, emission }
                    | QueryEntry::Append { key, emission } = entry
                    {
                        let (value, cardinality) =
                            self.resolve_query_emission(emission, scopes, endpoint)?;
                        normalized.query.push(NormalizedQuery {
                            key: key.clone(),
                            value,
                            cardinality,
                            operation: if matches!(entry, QueryEntry::Append { .. }) {
                                AttachmentOperation::Append
                            } else {
                                AttachmentOperation::Replace
                            },
                            origin: origin.to_owned(),
                        });
                    }
                }
            }
        }
        if let Some(operation) = &attachment_site.headers {
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
                            origin: origin.to_owned(),
                        });
                    }
                }
            }
        }
        if let Some(operation) = &attachment_site.auth {
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
                        let challenge = placement.challenge.unwrap_or(SourceChallenge::Never);
                        normalized.auth.push(NormalizedAuth {
                            kind: resolve_auth_placement(&placement.kind),
                            credential,
                            challenge: resolve_challenge(challenge),
                            span: placement.credential.span(),
                            origin: origin.to_owned(),
                        });
                    }
                }
            }
        }
        validate_effective_auth(&normalized)?;
        if let Some(operation) = &attachment_site.timeout {
            let operation = &operation.value;
            normalized.timeout_origin = Some(origin.to_owned());
            normalized.timeout = match operation {
                TimeoutOperation::Off => None,
                TimeoutOperation::Set(value) => {
                    let _ = value.span;
                    Some(value.value)
                }
            };
        }
        for declaration in &attachment_site.rate_limits {
            if normalized.rate_limits.len() >= MAX_RATE_LIMIT_POLICIES_PER_PLAN {
                return Err(declaration.key_span.error(format!(
                    "an effective rate-limit plan permits at most {MAX_RATE_LIMIT_POLICIES_PER_PLAN} policies"
                )));
            }
            let rule = self.resolve_rate_limit(
                &declaration.value,
                declaration.key_span,
                scopes,
                endpoint,
                rate_limit_context,
            )?;
            if rule.id.len() > MAX_RATE_LIMIT_ID_BYTES {
                return Err(declaration.key_span.error(format!(
                    "a rate-limit policy ID permits at most {MAX_RATE_LIMIT_ID_BYTES} UTF-8 bytes"
                )));
            }
            if let Some(existing) = normalized
                .rate_limits
                .iter()
                .find(|existing| existing.id == rule.id)
            {
                let mut error = rule
                    .id_span
                    .error("an effective rate-limit plan contains a duplicate policy ID");
                error.combine(
                    existing
                        .id_span
                        .error("the first effective policy using this ID is here"),
                );
                return Err(error);
            }
            self.validate_rate_limit_id_compatibility(&rule)?;
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

    pub(super) fn resolve_expr(
        &self,
        value: &PublicExpr,
        scopes: &[ScopeFrame],
        endpoint: Option<&[Value]>,
        usage: ExpressionUse,
    ) -> syn::Result<ResolvedExpr> {
        match value {
            PublicExpr::String(value) => Ok(ResolvedExpr::String(value.clone())),
            PublicExpr::Integer(value) => Ok(ResolvedExpr::Integer(value.clone())),
            PublicExpr::Concat { span, values } => {
                let _ = span;
                let mut flattened = Vec::new();
                for value in
                    self.resolve_expressions(values, scopes, endpoint, ExpressionUse::Single)?
                {
                    match value {
                        ResolvedExpr::Concat(values) => flattened.extend(values),
                        value => flattened.push(value),
                    }
                }
                Ok(ResolvedExpr::Concat(flattened))
            }
            PublicExpr::Reference { namespace, name } => {
                let reference = self.resolve_value_reference(namespace, name, scopes, endpoint)?;
                match usage {
                    ExpressionUse::Single
                        if matches!(reference.presence, ValuePresence::Optional) =>
                    {
                        return Err(name.span().error(
                            "Concord-optional values cannot appear in total scalar contexts such as subdomain, path, concat, accept, or rate keys",
                        ));
                    }
                    ExpressionUse::Single | ExpressionUse::Query | ExpressionUse::Header => {}
                }
                Ok(ResolvedExpr::Value(reference))
            }
        }
    }

    fn resolve_query_emission(
        &self,
        emission: &QueryEmission,
        scopes: &[ScopeFrame],
        endpoint: Option<&[Value]>,
    ) -> syn::Result<(ResolvedExpr, QueryCardinality)> {
        match emission {
            QueryEmission::One(value) => Ok((
                self.resolve_expr(value, scopes, endpoint, ExpressionUse::Query)?,
                QueryCardinality::One,
            )),
            QueryEmission::Each {
                namespace,
                name,
                span,
            } => {
                let _ = span;
                Ok((
                    ResolvedExpr::Value(
                        self.resolve_value_reference(namespace, name, scopes, endpoint)?,
                    ),
                    QueryCardinality::Each,
                ))
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
                timeout off;
                base "https://example.test";
                path ["v1"];
                query { "client" = "yes"; }

            scope resources() {
                path ["resources"];
                query { "scope" = "yes"; }
                GET read() -> Bytes using Bytes {
                    path ["read"];
                    query { remove "client"; "endpoint" = "yes"; }
                }
            }
        }});
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
