use std::collections::HashMap;

use proc_macro2::Span;
use syn::spanned::Spanned;
use syn::{GenericArgument, Ident, PathArguments, Type, TypePath};

use crate::ast::{
    AuthChallenge, AuthOperation, AuthPlacementKind, CredentialSource as SourceCredentialSource,
    CredentialsBlock, HeaderEntry, HeaderOperation, HostOperation, Layer, PathOperation,
    PublicExpr, QueryEntry, QueryOperation, RateLimitDimension, RateLimitOperation,
    RateLimitTemplate, RateLimitUse, SourceApi, SourceChild, SourceEndpoint, SourceScope,
    TimeoutOperation, ValueDecl,
};
use crate::model::{
    Api, Child, Client, Credential, CredentialSource, Endpoint, NormalizedAuth, NormalizedConfig,
    NormalizedHeader, NormalizedQuery, NormalizedRateLimitRule, NormalizedRateLimitWindow,
    ResolvedExpr, ResolvedRateLimitDimension, ResolvedValue, Scope, Secret, Value, ValueReference,
    ValueShape,
};
use crate::names::{
    RESERVED_GENERATED_METHODS, canonical_ident, prelude_alias, public_ident, request_ext_name,
    snake_ident, to_pascal_case, to_snake_case,
};

#[derive(Clone)]
struct ScopeFrame {
    id: usize,
    values: Vec<Value>,
}

struct Resolver<'a> {
    client_name: &'a Ident,
    client_values: &'a [Value],
    credentials: &'a [Credential],
    rate_templates: &'a [RateLimitTemplate],
    next_scope_id: usize,
    next_endpoint_id: usize,
    prelude_aliases: &'a mut HashMap<String, Span>,
}

#[derive(Clone, Copy)]
enum ExpressionUse {
    Single,
    Query,
    Header,
}

pub(crate) fn resolve(source: SourceApi) -> syn::Result<Api> {
    validate_client_names(&source)?;
    let mut prelude_aliases = HashMap::new();
    let mut clients = Vec::new();

    for source_client in source.clients {
        let source_declares_limiter = source_client
            .rate_limits
            .as_ref()
            .is_some_and(|block| !block.templates.is_empty())
            || source_tree_declares_rate_limits(&source_client.layer, &source_client.children);
        let (rate_templates, rate_observer) = source_client.rate_limits.map_or_else(
            || (Vec::new(), None),
            |block| (block.templates, block.observer),
        );
        let values = convert_values(source_client.values)?;
        validate_setter_namespace(&values, "client builder", true, true)?;
        let (secrets, mut credentials) = convert_credentials(source_client.credentials)?;
        validate_credential_names(&values, &secrets, &credentials)?;

        let client_name = source_client.name;
        let client_canonical = canonical_ident(&client_name);
        let builder_name = Ident::new(&format!("{client_canonical}Builder"), client_name.span());
        let support_name = snake_ident(&client_name);
        let endpoint_capability_name =
            Ident::new(&format!("{client_canonical}Endpoint"), client_name.span());
        let values_type_name = Ident::new(
            &format!("__Concord{}Values", to_pascal_case(&client_canonical)),
            Span::mixed_site(),
        );

        let (config, children) = {
            let mut resolver = Resolver {
                client_name: &client_name,
                client_values: &values,
                credentials: &credentials,
                rate_templates: &rate_templates,
                next_scope_id: 0,
                next_endpoint_id: 0,
                prelude_aliases: &mut prelude_aliases,
            };
            let config = resolver.normalize_layer(
                &source_client.layer,
                &NormalizedConfig::default(),
                &[],
                None,
                &client_canonical,
            )?;
            let children =
                resolver.resolve_children(source_client.children, &config, &[], &[], "client")?;
            (config, children)
        };
        resolve_endpoint_credentials(&mut credentials, &children)?;
        validate_credential_generated_names(&credentials, &children, "client")?;
        validate_credential_cycles(&credentials, &children)?;
        validate_child_methods(&children, &[], "generated client", true)?;

        clients.push(Client {
            name: client_name,
            builder_name,
            support_name,
            endpoint_capability_name,
            values_type_name,
            base: source_client.base,
            values,
            secrets,
            credentials,
            retry: source_client.retry,
            rate_observer,
            requires_limiter: source_declares_limiter,
            config,
            children,
        });
    }

    Ok(Api { clients })
}

impl Resolver<'_> {
    fn resolve_children(
        &mut self,
        source: Vec<SourceChild>,
        parent: &NormalizedConfig,
        visible_scopes: &[ScopeFrame],
        scope_path: &[Ident],
        owner: &str,
    ) -> syn::Result<Vec<Child>> {
        let mut children = Vec::new();
        for child in source {
            children.push(match child {
                SourceChild::Scope(scope) => Child::Scope(Box::new(self.resolve_scope(
                    *scope,
                    parent,
                    visible_scopes,
                    scope_path,
                )?)),
                SourceChild::Endpoint(endpoint) => Child::Endpoint(self.resolve_endpoint(
                    endpoint,
                    parent,
                    visible_scopes,
                    scope_path,
                )?),
            });
        }
        validate_module_items(&children, owner)?;
        Ok(children)
    }

    fn resolve_scope(
        &mut self,
        source: SourceScope,
        parent: &NormalizedConfig,
        visible_scopes: &[ScopeFrame],
        scope_path: &[Ident],
    ) -> syn::Result<Scope> {
        let id = self.next_scope_id;
        self.next_scope_id += 1;
        let parameters = convert_values(source.parameters)?;
        validate_setter_namespace(&parameters, "scope facade", false, false)?;

        let mut frames = visible_scopes.to_vec();
        frames.push(ScopeFrame {
            id,
            values: parameters.clone(),
        });
        let mut path = scope_path.to_vec();
        path.push(source.name.clone());
        let origin = rate_limit_origin(self.client_name, &path);
        let config = self.normalize_layer(&source.layer, parent, &frames, None, &origin)?;
        let children = self.resolve_children(source.children, &config, &frames, &path, "scope")?;
        validate_child_methods(&children, &parameters, "scope facade", false)?;

        let method_name = snake_ident(&source.name);
        let module_name = method_name.clone();
        let facade_name = Ident::new(
            &format!("{}Facade", to_pascal_case(&canonical_ident(&source.name))),
            source.name.span(),
        );
        Ok(Scope {
            id,
            name: source.name,
            method_name,
            module_name,
            facade_name,
            parameters,
            config,
            children,
        })
    }

    fn resolve_endpoint(
        &mut self,
        source: SourceEndpoint,
        parent: &NormalizedConfig,
        visible_scopes: &[ScopeFrame],
        scope_path: &[Ident],
    ) -> syn::Result<Endpoint> {
        let id = self.next_endpoint_id;
        self.next_endpoint_id += 1;
        let parameters = convert_values(source.parameters)?;
        validate_endpoint_setters(&parameters, visible_scopes)?;

        let public_method_name = source.alias.as_ref().map_or_else(
            || snake_ident(&source.name),
            |alias| public_ident(&canonical_ident(alias), alias.span()),
        );
        let request_ext_name = request_ext_name(&source.name);
        let alias = prelude_alias(
            self.client_name,
            scope_path,
            source.alias.as_ref().unwrap_or(&public_method_name),
        );
        let alias_name = canonical_ident(&alias);
        if let Some(previous) = self
            .prelude_aliases
            .insert(alias_name.clone(), alias.span())
        {
            let mut error = alias.span().error(format!(
                "generated request-extension prelude alias `{alias_name}` collides deterministically"
            ));
            error.combine(previous.error("the first generated alias is here"));
            return Err(error);
        }

        let dsl_path = scope_path
            .iter()
            .cloned()
            .chain(std::iter::once(source.name.clone()))
            .collect::<Vec<_>>();
        let origin = rate_limit_origin(self.client_name, &dsl_path);
        let config = self.normalize_layer(
            &source.layer,
            parent,
            visible_scopes,
            Some(&parameters),
            &origin,
        )?;
        Ok(Endpoint {
            id,
            method: source.method,
            name: source.name,
            alias: source.alias,
            public_method_name,
            request_ext_name,
            prelude_alias: alias,
            parameters,
            body: source.body,
            config,
            response: source.response,
            dsl_path,
        })
    }

    fn normalize_layer(
        &self,
        layer: &Layer,
        parent: &NormalizedConfig,
        scopes: &[ScopeFrame],
        endpoint: Option<&[Value]>,
        rate_origin: &str,
    ) -> syn::Result<NormalizedConfig> {
        let mut normalized = parent.clone();

        if let Some(operation) = &layer.host {
            normalized.host = match operation {
                HostOperation::Off => Vec::new(),
                HostOperation::Replace(values) => {
                    self.resolve_expressions(values, scopes, endpoint, ExpressionUse::Single)?
                }
            };
        }
        if let Some(operation) = &layer.path {
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
                AuthOperation::Off => normalized.auth.clear(),
                AuthOperation::Modify {
                    replace,
                    placements,
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
                                CredentialSource::OAuth2Client { .. } => {
                                    AuthChallenge::Unauthorized
                                }
                                CredentialSource::Custom { .. } => {
                                    return Err(placement.credential.span().error(
                                        "an auth placement using a custom provider requires an explicit challenge suffix",
                                    ));
                                }
                                _ => AuthChallenge::Never,
                            },
                        };
                        normalized.auth.push(NormalizedAuth {
                            kind: placement.kind.clone(),
                            credential,
                            challenge,
                        });
                    }
                }
            }
        }
        validate_effective_auth(&normalized)?;
        if let Some(operation) = &layer.timeout {
            normalized.timeout = match operation {
                TimeoutOperation::Off => None,
                TimeoutOperation::Set(value) => {
                    let _ = value.span;
                    Some(value.value)
                }
            };
        }
        if let Some(operation) = &layer.rate_limit {
            match operation {
                RateLimitOperation::Off => normalized.rate_limits.clear(),
                RateLimitOperation::Modify {
                    replacement,
                    additions,
                } => {
                    if let Some(name) = replacement {
                        normalized.rate_limits.clear();
                        normalized
                            .rate_limits
                            .push(self.resolve_named_rate_limit(name, scopes, endpoint)?);
                    }
                    for addition in additions {
                        normalized.rate_limits.push(match addition {
                            RateLimitUse::Named(name) => {
                                self.resolve_named_rate_limit(name, scopes, endpoint)?
                            }
                            RateLimitUse::Direct(template) => {
                                let id = format!(
                                    "{rate_origin}::rate_limit::{}",
                                    canonical_ident(&template.name)
                                );
                                self.resolve_rate_limit(template, id, scopes, endpoint)?
                            }
                        });
                    }
                }
            }
        }
        Ok(normalized)
    }

    fn resolve_named_rate_limit(
        &self,
        name: &Ident,
        scopes: &[ScopeFrame],
        endpoint: Option<&[Value]>,
    ) -> syn::Result<NormalizedRateLimitRule> {
        let template = self
            .rate_templates
            .iter()
            .find(|template| canonical_ident(&template.name) == canonical_ident(name))
            .ok_or_else(|| {
                name.span().error(format!(
                    "unknown client rate-limit template `{}`",
                    canonical_ident(name)
                ))
            })?;
        let id = format!(
            "{}::rate_limit::{}",
            canonical_ident(self.client_name),
            canonical_ident(&template.name)
        );
        self.resolve_rate_limit(template, id, scopes, endpoint)
    }

    fn resolve_rate_limit(
        &self,
        template: &RateLimitTemplate,
        id: String,
        scopes: &[ScopeFrame],
        endpoint: Option<&[Value]>,
    ) -> syn::Result<NormalizedRateLimitRule> {
        let dimensions = template
            .dimensions
            .iter()
            .map(|dimension| match dimension {
                RateLimitDimension::Host(_) => Ok(ResolvedRateLimitDimension::Host),
                RateLimitDimension::Endpoint(_) => Ok(ResolvedRateLimitDimension::Endpoint),
                RateLimitDimension::Method(_) => Ok(ResolvedRateLimitDimension::Method),
                RateLimitDimension::Value { namespace, name } => {
                    let value = self.resolve_value_reference(namespace, name, scopes, endpoint)?;
                    if !matches!(value.shape, ValueShape::Scalar) {
                        return Err(name.span().error(
                            "a dynamic rate-limit dimension must resolve to a required non-optional scalar value",
                        ));
                    }
                    Ok(ResolvedRateLimitDimension::Value {
                        name: format!("{}.{}", namespace, canonical_ident(name)),
                        value,
                    })
                }
            })
            .collect::<syn::Result<Vec<_>>>()?;
        let windows = template
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
            id,
            declared_name: template.name.clone(),
            dimensions,
            cost: template.cost,
            windows,
        })
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

    fn resolve_value_reference(
        &self,
        namespace: &Ident,
        name: &Ident,
        scopes: &[ScopeFrame],
        endpoint: Option<&[Value]>,
    ) -> syn::Result<ResolvedValue> {
        match namespace.to_string().as_str() {
            "vars" => {
                let index = find_unique_value(self.client_values, name, "client value")?;
                Ok(ResolvedValue {
                    reference: ValueReference::Client(index),
                    shape: self.client_values[index].shape.clone(),
                })
            }
            "scope" => {
                let matches = scopes
                    .iter()
                    .filter_map(|scope| {
                        find_value(&scope.values, name).map(|parameter| (scope, parameter))
                    })
                    .collect::<Vec<_>>();
                if matches.is_empty() {
                    return Err(name.span().error(format!(
                        "unknown visible scope value `{}`",
                        canonical_ident(name)
                    )));
                }
                if matches.len() != 1 {
                    return Err(name.span().error(format!(
                                "ambiguous visible scope value `{}`; qualify the DSL model so the name is unique",
                                canonical_ident(name)
                            )));
                }
                let (scope, parameter) = matches[0];
                Ok(ResolvedValue {
                    reference: ValueReference::Scope {
                        scope_id: scope.id,
                        parameter,
                    },
                    shape: scope.values[parameter].shape.clone(),
                })
            }
            "ep" => {
                let Some(endpoint) = endpoint else {
                    return Err(name
                        .span()
                        .error("`ep.NAME` is available only while shaping an endpoint"));
                };
                let index = find_unique_value(endpoint, name, "endpoint value")?;
                Ok(ResolvedValue {
                    reference: ValueReference::Endpoint(index),
                    shape: endpoint[index].shape.clone(),
                })
            }
            _ => Err(namespace
                .span()
                .error("value references use only `vars.NAME`, `scope.NAME`, or `ep.NAME`")),
        }
    }
}

fn source_tree_declares_rate_limits(layer: &Layer, children: &[SourceChild]) -> bool {
    let local = matches!(
        &layer.rate_limit,
        Some(RateLimitOperation::Modify {
            replacement: Some(_),
            ..
        })
    ) || matches!(
        &layer.rate_limit,
        Some(RateLimitOperation::Modify { additions, .. }) if !additions.is_empty()
    );
    local
        || children.iter().any(|child| match child {
            SourceChild::Scope(scope) => {
                source_tree_declares_rate_limits(&scope.layer, &scope.children)
            }
            SourceChild::Endpoint(endpoint) => {
                endpoint.layer.rate_limit.as_ref().is_some_and(|operation| {
                    matches!(
                        operation,
                        RateLimitOperation::Modify {
                            replacement: Some(_),
                            ..
                        }
                    ) || matches!(
                        operation,
                        RateLimitOperation::Modify { additions, .. } if !additions.is_empty()
                    )
                })
            }
        })
}

fn rate_limit_origin(client: &Ident, path: &[Ident]) -> String {
    std::iter::once(canonical_ident(client))
        .chain(path.iter().map(canonical_ident))
        .collect::<Vec<_>>()
        .join("::")
}

fn convert_credentials(
    block: Option<CredentialsBlock>,
) -> syn::Result<(Vec<Secret>, Vec<Credential>)> {
    let Some(block) = block else {
        return Ok((Vec::new(), Vec::new()));
    };
    let secrets = block
        .secrets
        .into_iter()
        .map(|secret| Secret { name: secret.name })
        .collect::<Vec<_>>();
    let secret_index = secrets
        .iter()
        .enumerate()
        .map(|(index, secret)| (canonical_ident(&secret.name), index))
        .collect::<HashMap<_, _>>();
    let mut credentials = Vec::new();
    for credential in block.credentials {
        let source = match credential.source {
            SourceCredentialSource::ApiKey { secret } => CredentialSource::ApiKey {
                secret: secret_index[&canonical_ident(&secret)],
            },
            SourceCredentialSource::Bearer { secret } => CredentialSource::Bearer {
                secret: secret_index[&canonical_ident(&secret)],
            },
            SourceCredentialSource::Basic { username, password } => CredentialSource::Basic {
                username: secret_index[&canonical_ident(&username)],
                password: secret_index[&canonical_ident(&password)],
            },
            SourceCredentialSource::OAuth2Client {
                token_url,
                client_id,
                client_secret,
                scope,
            } => CredentialSource::OAuth2Client {
                token_url,
                client_id: secret_index[&canonical_ident(&client_id)],
                client_secret: secret_index[&canonical_ident(&client_secret)],
                scope,
            },
            SourceCredentialSource::Endpoint { path } => CredentialSource::Endpoint {
                path,
                endpoint_id: None,
            },
            SourceCredentialSource::Custom { material } => CredentialSource::Custom { material },
        };
        credentials.push(Credential {
            name: credential.name,
            source,
        });
    }
    Ok((secrets, credentials))
}

fn find_credential(credentials: &[Credential], name: &Ident) -> syn::Result<usize> {
    credentials
        .iter()
        .position(|credential| canonical_ident(&credential.name) == canonical_ident(name))
        .ok_or_else(|| {
            name.span()
                .error(format!("unknown credential `{}`", canonical_ident(name)))
        })
}

fn validate_auth_compatibility(
    placement: &AuthPlacementKind,
    source: &CredentialSource,
    span: &Ident,
) -> syn::Result<()> {
    let compatible = match source {
        CredentialSource::ApiKey { .. } => {
            matches!(
                placement,
                AuthPlacementKind::Header(_) | AuthPlacementKind::Query(_)
            )
        }
        CredentialSource::Bearer { .. } | CredentialSource::OAuth2Client { .. } => {
            matches!(placement, AuthPlacementKind::Bearer)
        }
        CredentialSource::Basic { .. } => matches!(placement, AuthPlacementKind::Basic),
        CredentialSource::Endpoint { .. } | CredentialSource::Custom { .. } => true,
    };
    if compatible {
        Ok(())
    } else {
        Err(span
            .span()
            .error("credential source is incompatible with this authentication placement"))
    }
}

fn validate_effective_auth(config: &NormalizedConfig) -> syn::Result<()> {
    let challenge_count = config
        .auth
        .iter()
        .filter(|placement| placement.challenge != AuthChallenge::Never)
        .count();
    if challenge_count > 1 {
        let span = config
            .auth
            .iter()
            .find(|placement| placement.challenge != AuthChallenge::Never)
            .map(|placement| match &placement.kind {
                AuthPlacementKind::Header(name) | AuthPlacementKind::Query(name) => name.span(),
                AuthPlacementKind::Bearer | AuthPlacementKind::Basic => Span::call_site(),
            })
            .unwrap_or_else(Span::call_site);
        return Err(span.error(
            "one effective authentication plan may contain at most one challenge-enabled placement",
        ));
    }
    for (index, placement) in config.auth.iter().enumerate() {
        match &placement.kind {
            AuthPlacementKind::Bearer | AuthPlacementKind::Basic => {
                if config
                    .auth
                    .iter()
                    .enumerate()
                    .any(|(other, candidate)| {
                        other != index
                            && (matches!(
                                &candidate.kind,
                                AuthPlacementKind::Bearer | AuthPlacementKind::Basic
                            ) || matches!(
                                &candidate.kind,
                                AuthPlacementKind::Header(name)
                                    if name.value().eq_ignore_ascii_case("authorization")
                            ))
                    })
                {
                    return Err(Span::call_site().error(
                        "effective authentication placements collide on `Authorization`",
                    ));
                }
            }
            AuthPlacementKind::Header(name) => {
                let canonical = name.value().to_ascii_lowercase();
                if canonical == "authorization"
                    || config
                        .headers
                        .iter()
                        .any(|header| {
                            header.canonical_name == canonical
                                && resolved_expr_is_guaranteed_present(&header.value)
                        })
                    || config.auth[..index].iter().any(|candidate| {
                        matches!(&candidate.kind, AuthPlacementKind::Header(other) if other.value().eq_ignore_ascii_case(&name.value()))
                    })
                {
                    return Err(name.span().error(
                        "authentication header collides with an effective public or authentication header",
                    ));
                }
            }
            AuthPlacementKind::Query(name) => {
                if config
                    .query
                    .iter()
                    .any(|query| {
                        query.key.value() == name.value()
                            && resolved_expr_is_guaranteed_present(&query.value)
                    })
                    || config.auth[..index].iter().any(|candidate| {
                        matches!(&candidate.kind, AuthPlacementKind::Query(other) if other.value() == name.value())
                    })
                {
                    return Err(name.span().error(
                        "authentication query key collides with an effective public or authentication query key",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn resolved_expr_is_guaranteed_present(value: &ResolvedExpr) -> bool {
    match value {
        ResolvedExpr::Value(value) => matches!(value.shape, ValueShape::Scalar),
        ResolvedExpr::String(_) | ResolvedExpr::Integer(_) | ResolvedExpr::Format(_) => true,
    }
}

fn validate_credential_names(
    values: &[Value],
    secrets: &[Secret],
    credentials: &[Credential],
) -> syn::Result<()> {
    let mut builder = RESERVED_GENERATED_METHODS
        .iter()
        .map(|name| ((*name).to_owned(), Span::call_site()))
        .collect::<HashMap<_, _>>();
    for value in values {
        insert_name(
            &mut builder,
            canonical_ident(&value.name),
            value.name.span(),
            "generated client builder method",
        )?;
        if value.is_optional() {
            insert_name(
                &mut builder,
                format!("clear_{}", canonical_ident(&value.name)),
                value.name.span(),
                "generated client builder method",
            )?;
        } else if value.default().is_some() {
            insert_name(
                &mut builder,
                format!("reset_{}", canonical_ident(&value.name)),
                value.name.span(),
                "generated client builder method",
            )?;
        }
    }
    for secret in secrets {
        insert_name(
            &mut builder,
            canonical_ident(&secret.name),
            secret.name.span(),
            "generated client builder method",
        )?;
    }
    for credential in credentials {
        if matches!(credential.source, CredentialSource::Custom { .. }) {
            insert_name(
                &mut builder,
                format!(
                    "{}_provider",
                    to_snake_case(&canonical_ident(&credential.name))
                ),
                credential.name.span(),
                "generated client builder method",
            )?;
        }
    }
    let mut accessors = HashMap::from([("__concord_new".to_owned(), Span::call_site())]);
    for credential in credentials {
        insert_name(
            &mut accessors,
            to_snake_case(&canonical_ident(&credential.name)),
            credential.name.span(),
            "generated credential accessor",
        )?;
    }
    Ok(())
}

fn validate_credential_generated_names(
    credentials: &[Credential],
    children: &[Child],
    owner: &str,
) -> syn::Result<()> {
    let mut items = HashMap::new();
    for child in children {
        match child {
            Child::Scope(scope) => {
                insert_name(
                    &mut items,
                    canonical_ident(&scope.module_name),
                    scope.name.span(),
                    &format!("{owner} generated module"),
                )?;
                validate_credential_generated_names(credentials, &scope.children, "scope")?;
            }
            Child::Endpoint(endpoint) => {
                insert_name(
                    &mut items,
                    canonical_ident(&endpoint.name),
                    endpoint.name.span(),
                    &format!("{owner} explicit endpoint module"),
                )?;
                let acquisitions = endpoint_acquisitions(credentials, endpoint.id);
                if endpoint.parameters.iter().any(|value| !value.is_required())
                    || !acquisitions.is_empty()
                {
                    insert_name(
                        &mut items,
                        canonical_ident(&endpoint.request_ext_name),
                        endpoint.name.span(),
                        &format!("{owner} endpoint request extension"),
                    )?;
                }
                if !acquisitions.is_empty() {
                    validate_acquisition_methods(endpoint, &acquisitions)?;
                }
            }
        }
    }
    Ok(())
}

fn endpoint_acquisitions(credentials: &[Credential], endpoint_id: usize) -> Vec<&Credential> {
    credentials
        .iter()
        .filter(|credential| {
            matches!(
                credential.source,
                CredentialSource::Endpoint {
                    endpoint_id: Some(id),
                    ..
                } if id == endpoint_id
            )
        })
        .collect()
}

fn validate_acquisition_methods(
    endpoint: &Endpoint,
    acquisitions: &[&Credential],
) -> syn::Result<()> {
    let mut names = RESERVED_GENERATED_METHODS
        .iter()
        .map(|name| ((*name).to_owned(), Span::call_site()))
        .collect::<HashMap<_, _>>();
    for value in endpoint
        .parameters
        .iter()
        .filter(|value| !value.is_required())
    {
        insert_name(
            &mut names,
            canonical_ident(&value.name),
            value.name.span(),
            "endpoint request controls and credential acquisition terminals",
        )?;
        let helper = if value.is_optional() {
            format!("clear_{}", canonical_ident(&value.name))
        } else {
            format!("reset_{}", canonical_ident(&value.name))
        };
        insert_name(
            &mut names,
            helper,
            value.name.span(),
            "endpoint request controls and credential acquisition terminals",
        )?;
    }
    for credential in acquisitions {
        insert_name(
            &mut names,
            format!(
                "acquire_as_{}",
                to_snake_case(&canonical_ident(&credential.name))
            ),
            credential.name.span(),
            "endpoint request controls and credential acquisition terminals",
        )?;
    }
    Ok(())
}

fn resolve_endpoint_credentials(
    credentials: &mut [Credential],
    children: &[Child],
) -> syn::Result<()> {
    let mut endpoints = Vec::new();
    collect_endpoints(children, &mut endpoints);
    let paths = endpoints
        .iter()
        .map(|endpoint| {
            (
                endpoint
                    .dsl_path
                    .iter()
                    .map(canonical_ident)
                    .collect::<Vec<_>>()
                    .join("::"),
                endpoint.id,
            )
        })
        .collect::<HashMap<_, _>>();
    for credential in credentials {
        if let CredentialSource::Endpoint { path, endpoint_id } = &mut credential.source {
            let canonical = path
                .segments
                .iter()
                .map(|segment| canonical_ident(&segment.ident))
                .collect::<Vec<_>>()
                .join("::");
            *endpoint_id = Some(*paths.get(&canonical).ok_or_else(|| {
                path.span().error(format!(
                    "unknown endpoint-backed credential target `{canonical}`"
                ))
            })?);
        }
    }
    Ok(())
}

fn collect_endpoints<'a>(children: &'a [Child], output: &mut Vec<&'a Endpoint>) {
    for child in children {
        match child {
            Child::Scope(scope) => collect_endpoints(&scope.children, output),
            Child::Endpoint(endpoint) => output.push(endpoint),
        }
    }
}

fn validate_credential_cycles(credentials: &[Credential], children: &[Child]) -> syn::Result<()> {
    let mut endpoints = Vec::new();
    collect_endpoints(children, &mut endpoints);
    let endpoint_count = endpoints
        .iter()
        .map(|endpoint| endpoint.id)
        .max()
        .map_or(0, |maximum| maximum + 1);
    let credential_count = credentials.len();
    let mut edges = vec![Vec::new(); credential_count + endpoint_count];
    for (index, credential) in credentials.iter().enumerate() {
        if let CredentialSource::Endpoint {
            endpoint_id: Some(endpoint),
            ..
        } = credential.source
        {
            edges[index].push(credential_count + endpoint);
        }
    }
    for endpoint in endpoints {
        for auth in &endpoint.config.auth {
            edges[credential_count + endpoint.id].push(auth.credential);
        }
    }
    let mut state = vec![0_u8; edges.len()];
    fn visit(node: usize, edges: &[Vec<usize>], state: &mut [u8]) -> bool {
        if state[node] == 1 {
            return true;
        }
        if state[node] == 2 {
            return false;
        }
        state[node] = 1;
        if edges[node]
            .iter()
            .copied()
            .any(|next| visit(next, edges, state))
        {
            return true;
        }
        state[node] = 2;
        false
    }
    for node in 0..edges.len() {
        if visit(node, &edges, &mut state) {
            return Err(Span::call_site()
                .error("endpoint-backed credential dependency or authentication cycle detected"));
        }
    }
    Ok(())
}

fn convert_values(values: Vec<ValueDecl>) -> syn::Result<Vec<Value>> {
    values
        .into_iter()
        .map(|value| {
            let shape = value_shape(&value.ty)?;
            Ok(Value {
                name: value.name,
                ty: value.ty,
                kind: value.kind,
                shape,
            })
        })
        .collect()
}

fn value_shape(ty: &Type) -> syn::Result<ValueShape> {
    if let Some(inner) = single_outer_argument(ty, "Option")? {
        if let Some(element) = single_outer_argument(&inner, "Vec")? {
            return Ok(ValueShape::OptionVec { inner, element });
        }
        return Ok(ValueShape::Option { inner });
    }
    if let Some(element) = single_outer_argument(ty, "Vec")? {
        return Ok(ValueShape::Vec { element });
    }
    Ok(ValueShape::Scalar)
}

fn single_outer_argument(ty: &Type, expected: &str) -> syn::Result<Option<Type>> {
    let Type::Path(TypePath { qself: None, path }) = ty else {
        return Ok(None);
    };
    if path.leading_colon.is_some()
        || path.segments.len() != 1
        || path.segments[0].ident != expected
    {
        return Ok(None);
    }
    let PathArguments::AngleBracketed(arguments) = &path.segments[0].arguments else {
        return Err(ty
            .span()
            .error(format!("`{expected}` requires exactly one type argument")));
    };
    let mut arguments = arguments.args.iter();
    let Some(GenericArgument::Type(inner)) = arguments.next() else {
        return Err(ty
            .span()
            .error(format!("`{expected}` requires exactly one type argument")));
    };
    if arguments.next().is_some() {
        return Err(ty
            .span()
            .error(format!("`{expected}` requires exactly one type argument")));
    }
    Ok(Some(inner.clone()))
}

fn find_value(values: &[Value], name: &Ident) -> Option<usize> {
    let name = canonical_ident(name);
    values
        .iter()
        .position(|value| canonical_ident(&value.name) == name)
}

fn find_unique_value(values: &[Value], name: &Ident, owner: &str) -> syn::Result<usize> {
    find_value(values, name).ok_or_else(|| {
        name.span()
            .error(format!("unknown {owner} `{}`", canonical_ident(name)))
    })
}

fn validate_client_names(source: &SourceApi) -> syn::Result<()> {
    let mut invocation_items = HashMap::new();
    for client in &source.clients {
        let client_name = canonical_ident(&client.name);
        insert_name(
            &mut invocation_items,
            client_name.clone(),
            client.name.span(),
            "invocation-scope client, builder, or support-module",
        )?;
        insert_name(
            &mut invocation_items,
            format!("{client_name}Builder"),
            client.name.span(),
            "invocation-scope client, builder, or support-module",
        )?;
        insert_name(
            &mut invocation_items,
            to_snake_case(&client_name),
            client.name.span(),
            "invocation-scope client, builder, or support-module",
        )?;
    }
    Ok(())
}

fn validate_setter_namespace(
    values: &[Value],
    owner: &str,
    include_required: bool,
    reserve_builder_methods: bool,
) -> syn::Result<()> {
    let mut names = HashMap::new();
    if reserve_builder_methods {
        for name in RESERVED_GENERATED_METHODS {
            names.insert((*name).to_owned(), Span::call_site());
        }
    }
    for value in values {
        if include_required || !value.is_required() {
            insert_name(
                &mut names,
                canonical_ident(&value.name),
                value.name.span(),
                owner,
            )?;
        }
        if value.is_optional() {
            insert_name(
                &mut names,
                format!("clear_{}", canonical_ident(&value.name)),
                value.name.span(),
                owner,
            )?;
        } else if value.default().is_some() {
            insert_name(
                &mut names,
                format!("reset_{}", canonical_ident(&value.name)),
                value.name.span(),
                owner,
            )?;
        }
    }
    Ok(())
}

fn validate_endpoint_setters(parameters: &[Value], scopes: &[ScopeFrame]) -> syn::Result<()> {
    let mut values = scopes
        .iter()
        .flat_map(|scope| scope.values.iter())
        .filter(|value| !value.is_required())
        .cloned()
        .collect::<Vec<_>>();
    values.extend(
        parameters
            .iter()
            .filter(|value| !value.is_required())
            .cloned(),
    );
    validate_setter_namespace(&values, "explicit endpoint controls", false, true)?;
    validate_setter_namespace(parameters, "endpoint request controls", false, true)
}

fn validate_child_methods(
    children: &[Child],
    local_values: &[Value],
    owner: &str,
    reserve_all: bool,
) -> syn::Result<()> {
    let mut names = HashMap::new();
    if reserve_all {
        for reserved in RESERVED_GENERATED_METHODS {
            names.insert((*reserved).to_owned(), Span::call_site());
        }
    }
    for value in local_values.iter().filter(|value| !value.is_required()) {
        insert_name(
            &mut names,
            canonical_ident(&value.name),
            value.name.span(),
            owner,
        )?;
        let helper = if value.is_optional() {
            format!("clear_{}", canonical_ident(&value.name))
        } else {
            format!("reset_{}", canonical_ident(&value.name))
        };
        insert_name(&mut names, helper, value.name.span(), owner)?;
    }
    for child in children {
        let (name, span) = match child {
            Child::Scope(scope) => (canonical_ident(&scope.method_name), scope.name.span()),
            Child::Endpoint(endpoint) => (
                canonical_ident(&endpoint.public_method_name),
                endpoint.name.span(),
            ),
        };
        if RESERVED_GENERATED_METHODS.contains(&name.as_str()) {
            return Err(span.error(format!(
                "generated {owner} method `{name}` collides with a reserved Concord method"
            )));
        }
        insert_name(&mut names, name, span, owner)?;
    }
    Ok(())
}

fn validate_module_items(children: &[Child], owner: &str) -> syn::Result<()> {
    let mut items = HashMap::new();
    for child in children {
        match child {
            Child::Scope(scope) => {
                insert_name(
                    &mut items,
                    canonical_ident(&scope.module_name),
                    scope.name.span(),
                    &format!("{owner} generated module"),
                )?;
            }
            Child::Endpoint(endpoint) => {
                insert_name(
                    &mut items,
                    canonical_ident(&endpoint.name),
                    endpoint.name.span(),
                    &format!("{owner} explicit endpoint module"),
                )?;
                if endpoint.parameters.iter().any(|value| !value.is_required()) {
                    insert_name(
                        &mut items,
                        canonical_ident(&endpoint.request_ext_name),
                        endpoint.name.span(),
                        &format!("{owner} endpoint request extension"),
                    )?;
                }
            }
        }
    }
    Ok(())
}

fn insert_name(
    names: &mut HashMap<String, Span>,
    name: String,
    span: Span,
    owner: &str,
) -> syn::Result<()> {
    if let Some(previous) = names.insert(name.clone(), span) {
        let mut error = span.error(format!(
            "generated {owner} name `{name}` collides after deterministic normalization"
        ));
        error.combine(previous.error("the first generated name is here"));
        return Err(error);
    }
    Ok(())
}

trait SpanError {
    fn error(self, message: impl std::fmt::Display) -> syn::Error;
}

impl SpanError for Span {
    fn error(self, message: impl std::fmt::Display) -> syn::Error {
        syn::Error::new(self, message)
    }
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::*;
    use crate::parse;

    fn resolve_ok(tokens: proc_macro2::TokenStream) -> Api {
        resolve(parse::parse(tokens).expect("AST")).expect("normalized IR")
    }

    fn resolve_error(tokens: proc_macro2::TokenStream) -> syn::Error {
        resolve(parse::parse(tokens).expect("source AST"))
            .err()
            .expect("semantic failure")
    }

    #[test]
    fn phase7_rate_normalization_resolves_append_replace_off_direct_order_and_ids() {
        let api = resolve_ok(quote! {
            client RateApi {
                base "https://example.test";
                var tenant: String;
                rate_limits {
                    application {
                        by [host];
                        500 / 10s;
                        30000 / 10m;
                    }
                    tenant {
                        by [host, endpoint, method, vars.tenant];
                        cost 2;
                        100 / 1m;
                    }
                    observe crate::Observer;
                }
                rate_limit application;
                scope regions(region: String) {
                    rate_limit replace tenant;
                    rate_limit regional {
                        by [scope.region];
                        20 / 1s;
                    }
                    GET Read(id: String) -> Bytes {
                        rate_limit application;
                        rate_limit endpoint_quota {
                            by [ep.id];
                            5 / 1s;
                        }
                    }
                    GET Public() -> Bytes { rate_limit off; }
                }
            }
        });
        let client = &api.clients[0];
        assert!(client.requires_limiter);
        assert!(client.rate_observer.is_some());
        assert_eq!(client.config.rate_limits.len(), 1);
        assert_eq!(
            client.config.rate_limits[0].id,
            "RateApi::rate_limit::application"
        );
        let Child::Scope(scope) = &client.children[0] else {
            panic!("scope")
        };
        assert_eq!(scope.config.rate_limits.len(), 2);
        assert_eq!(
            scope
                .config
                .rate_limits
                .iter()
                .map(|rule| canonical_ident(&rule.declared_name))
                .collect::<Vec<_>>(),
            ["tenant", "regional"]
        );
        assert_eq!(
            scope.config.rate_limits[1].id,
            "RateApi::regions::rate_limit::regional"
        );
        let Child::Endpoint(read) = &scope.children[0] else {
            panic!("read")
        };
        assert_eq!(
            read.config
                .rate_limits
                .iter()
                .map(|rule| canonical_ident(&rule.declared_name))
                .collect::<Vec<_>>(),
            ["tenant", "regional", "application", "endpoint_quota"]
        );
        assert_eq!(
            read.config.rate_limits[3].id,
            "RateApi::regions::Read::rate_limit::endpoint_quota"
        );
        assert_eq!(read.config.rate_limits[0].cost, 2);
        assert_eq!(read.config.rate_limits[0].windows.len(), 1);
        assert!(matches!(
            read.config.rate_limits[0].dimensions[3],
            ResolvedRateLimitDimension::Value {
                value: ResolvedValue {
                    reference: ValueReference::Client(0),
                    ..
                },
                ..
            }
        ));
        assert!(matches!(
            read.config.rate_limits[1].dimensions[0],
            ResolvedRateLimitDimension::Value {
                value: ResolvedValue {
                    reference: ValueReference::Scope { .. },
                    ..
                },
                ..
            }
        ));
        assert!(matches!(
            read.config.rate_limits[3].dimensions[0],
            ResolvedRateLimitDimension::Value {
                value: ResolvedValue {
                    reference: ValueReference::Endpoint(0),
                    ..
                },
                ..
            }
        ));
        let Child::Endpoint(public) = &scope.children[1] else {
            panic!("public")
        };
        assert!(public.config.rate_limits.is_empty());
    }

    #[test]
    fn phase7_rate_semantics_reject_undefined_out_of_scope_and_optional_dimensions() {
        let undefined = resolve_error(quote! {
            client Api {
                base "https://example.test";
                rate_limit missing;
            }
        });
        assert!(
            undefined
                .to_string()
                .contains("unknown client rate-limit template")
        );

        let undefined_value = resolve_error(quote! {
            client Api {
                base "https://example.test";
                rate_limits { quota { by [vars.missing]; 1 / 1s; } }
                rate_limit quota;
            }
        });
        assert!(undefined_value.to_string().contains("unknown client value"));

        let optional = resolve_error(quote! {
            client Api {
                base "https://example.test";
                var tenant: Option<String>;
                rate_limits { quota { by [vars.tenant]; 1 / 1s; } }
                rate_limit quota;
            }
        });
        assert!(
            optional
                .to_string()
                .contains("required non-optional scalar")
        );

        let missing_scope = resolve_error(quote! {
            client Api {
                base "https://example.test";
                rate_limits { quota { by [scope.tenant]; 1 / 1s; } }
                rate_limit quota;
            }
        });
        assert!(
            missing_scope
                .to_string()
                .contains("unknown visible scope value")
        );

        let endpoint_at_client = resolve_error(quote! {
            client Api {
                base "https://example.test";
                rate_limits { quota { by [ep.id]; 1 / 1s; } }
                rate_limit quota;
            }
        });
        assert!(
            endpoint_at_client
                .to_string()
                .contains("available only while shaping an endpoint")
        );
    }

    #[test]
    fn normalization_removes_layer_operations_and_preserves_order() {
        let api = resolve_ok(quote! {
            client ShapeApi {
                base "https://example.test/root";
                var tenant: String;
                path ["v1"];
                query { "first" = "one"; "replace" = "old"; }
                headers { "X-First" = "one"; }
                scope users(region: String) {
                    path replace ["users", scope.region];
                    query { remove "replace"; "second" = vars.tenant; }
                    headers replace { "X-Second" = scope.region; }
                    GET ById(id: String) -> Bytes { path [ep.id]; timeout 2s; }
                }
            }
        });
        let Child::Scope(scope) = &api.clients[0].children[0] else {
            panic!("scope")
        };
        let Child::Endpoint(endpoint) = &scope.children[0] else {
            panic!("endpoint")
        };
        assert_eq!(endpoint.config.path.len(), 3);
        assert_eq!(
            endpoint
                .config
                .query
                .iter()
                .map(|entry| entry.key.value())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        assert_eq!(endpoint.config.headers.len(), 1);
        assert_eq!(
            endpoint.config.timeout,
            Some(std::time::Duration::from_secs(2))
        );
    }

    #[test]
    fn lexical_resolution_rejects_ambiguous_visible_scope_names() {
        let error = resolve(
            parse::parse(quote! {
                client Api {
                    base "https://example.test";
                    scope outer(id: String) {
                        scope inner(id: String) {
                            GET Read() -> Bytes { path [scope.id]; }
                        }
                    }
                }
            })
            .expect("AST"),
        )
        .err()
        .expect("ambiguous scope name");
        assert!(error.to_string().contains("ambiguous visible scope value"));
    }

    #[test]
    fn prelude_aliases_include_client_scope_and_endpoint_identity() {
        let api = resolve_ok(quote! {
            client ExampleApi {
                base "https://example.test";
                scope regions(region: String) {
                    scope users(tenant: String) {
                        GET ById(verbose: Option<bool>) as fetch -> Bytes;
                    }
                }
            }
        });
        let Child::Scope(regions) = &api.clients[0].children[0] else {
            panic!()
        };
        let Child::Scope(users) = &regions.children[0] else {
            panic!()
        };
        let Child::Endpoint(endpoint) = &users.children[0] else {
            panic!()
        };
        assert_eq!(
            endpoint.prelude_alias,
            "ExampleApiRegionsUsersFetchRequestExt"
        );
    }

    #[test]
    fn normalization_resolves_replace_remove_off_and_type_shapes() {
        let api = resolve_ok(quote! {
            client LayerApi {
                base "https://example.test/fixed";
                host ["client"];
                path ["client"];
                query { "keep" = "client"; "change" = "client"; }
                headers { "X-Keep" = "client"; "X-Change" = "client"; }
                timeout 5s;
                scope shaped() {
                    host off;
                    path replace ["scope"];
                    query {
                        "change" = "scope";
                        "local" = "scope";
                    }
                    headers {
                        "x-change" = "scope";
                        "X-Local" = "scope";
                    }
                    timeout off;
                    GET Modified(
                        scalar: String,
                        optional: Option<String>,
                        vector: Vec<String>,
                        optional_vector: Option<Vec<String>>,
                    ) -> Bytes;
                    GET Cleared() -> Bytes {
                        host ["endpoint"];
                        path off;
                        query off;
                        headers off;
                        timeout 1ms;
                    }
                    GET Replaced() -> Bytes {
                        query replace { "only" = "endpoint"; }
                        headers replace { "X-Only" = "endpoint"; }
                    }
                }
            }
        });
        let Child::Scope(scope) = &api.clients[0].children[0] else {
            panic!("scope")
        };
        let Child::Endpoint(modified) = &scope.children[0] else {
            panic!("modified endpoint")
        };
        assert!(modified.config.host.is_empty());
        assert_eq!(modified.config.path.len(), 1);
        assert_eq!(
            modified
                .config
                .query
                .iter()
                .map(|entry| entry.key.value())
                .collect::<Vec<_>>(),
            ["keep", "change", "local"]
        );
        assert_eq!(
            modified
                .config
                .headers
                .iter()
                .map(|entry| entry.canonical_name.as_str())
                .collect::<Vec<_>>(),
            ["x-keep", "x-change", "x-local"]
        );
        assert_eq!(modified.config.timeout, None);
        assert!(matches!(modified.parameters[0].shape, ValueShape::Scalar));
        assert!(matches!(
            modified.parameters[1].shape,
            ValueShape::Option { .. }
        ));
        assert!(matches!(
            modified.parameters[2].shape,
            ValueShape::Vec { .. }
        ));
        assert!(matches!(
            modified.parameters[3].shape,
            ValueShape::OptionVec { .. }
        ));

        let Child::Endpoint(cleared) = &scope.children[1] else {
            panic!("cleared endpoint")
        };
        assert_eq!(cleared.config.host.len(), 1);
        assert!(cleared.config.path.is_empty());
        assert!(cleared.config.query.is_empty());
        assert!(cleared.config.headers.is_empty());
        assert_eq!(
            cleared.config.timeout,
            Some(std::time::Duration::from_millis(1))
        );

        let Child::Endpoint(replaced) = &scope.children[2] else {
            panic!("replaced endpoint")
        };
        assert_eq!(replaced.config.query[0].key.value(), "only");
        assert_eq!(replaced.config.query.len(), 1);
        assert_eq!(replaced.config.headers[0].canonical_name, "x-only");
        assert_eq!(replaced.config.headers.len(), 1);
    }

    #[test]
    fn semantic_references_and_header_vector_use_fail_with_focused_diagnostics() {
        let unknown = resolve(
            parse::parse(quote! {
                client Api {
                    base "https://example.test";
                    GET Read() -> Bytes { path [ep.missing]; }
                }
            })
            .expect("AST"),
        )
        .err()
        .expect("unknown endpoint value");
        assert!(
            unknown
                .to_string()
                .contains("unknown endpoint value `missing`")
        );

        let vector_header = resolve(
            parse::parse(quote! {
                client Api {
                    base "https://example.test";
                    GET Read(values: Vec<String>) -> Bytes {
                        headers { "X-Values" = ep.values; }
                    }
                }
            })
            .expect("AST"),
        )
        .err()
        .expect("vector header");
        assert!(
            vector_header
                .to_string()
                .contains("do not accept vector values")
        );

        let secret_namespace = parse::parse(quote! {
            client Api {
                base "https://example.test";
                GET Read() -> Bytes { query { "token" = credentials.token; } }
            }
        })
        .err()
        .expect("credential expression namespace");
        assert!(
            secret_namespace
                .to_string()
                .contains("secrets and credentials are not expressible")
        );
    }

    #[test]
    fn phase6_auth_normalization_appends_replaces_clears_and_resolves_defaults() {
        let api = resolve_ok(quote! {
            client AuthApi {
                base "https://example.test";
                credentials {
                    secret key_value: String;
                    secret bearer_value: String;
                    secret client_id: String;
                    secret client_secret: String;
                    credential key = api_key(secret.key_value);
                    credential bearer = bearer(secret.bearer_value);
                    credential oauth = oauth2_client {
                        token_url: "https://auth.example.test/token",
                        client_id: secret.client_id,
                        client_secret: secret.client_secret,
                    };
                    credential acquired = endpoint Login;
                    credential custom = custom crate::SessionToken;
                }
                auth header "X-Api-Key" = key;
                GET Login() -> Bytes;
                scope Secured() {
                    auth bearer oauth;
                    GET Inherited() -> Bytes;
                    GET Replaced() -> Bytes {
                        auth replace bearer bearer;
                    }
                    GET Custom() -> Bytes {
                        auth replace header "X-Session" = custom challenge never;
                    }
                    GET Public() -> Bytes { auth off; }
                }
            }
        });
        let client = &api.clients[0];
        assert_eq!(client.config.auth.len(), 1);
        assert_eq!(client.config.auth[0].challenge, AuthChallenge::Never);
        let Child::Scope(scope) = &client.children[1] else {
            panic!("secured scope")
        };
        assert_eq!(scope.config.auth.len(), 2);
        assert_eq!(scope.config.auth[0].challenge, AuthChallenge::Never);
        assert_eq!(scope.config.auth[1].challenge, AuthChallenge::Unauthorized);
        let Child::Endpoint(inherited) = &scope.children[0] else {
            panic!("inherited endpoint")
        };
        assert_eq!(inherited.config.auth.len(), 2);
        let Child::Endpoint(replaced) = &scope.children[1] else {
            panic!("replaced endpoint")
        };
        assert_eq!(replaced.config.auth.len(), 1);
        assert_eq!(replaced.config.auth[0].challenge, AuthChallenge::Never);
        let Child::Endpoint(custom) = &scope.children[2] else {
            panic!("custom endpoint")
        };
        assert_eq!(custom.config.auth.len(), 1);
        assert_eq!(custom.config.auth[0].challenge, AuthChallenge::Never);
        let Child::Endpoint(public) = &scope.children[3] else {
            panic!("public endpoint")
        };
        assert!(public.config.auth.is_empty());
    }

    #[test]
    fn phase6_auth_semantics_reject_unknown_incompatible_ambiguous_and_colliding_plans() {
        fn error(tokens: proc_macro2::TokenStream) -> String {
            match parse::parse(tokens) {
                Ok(source) => resolve(source).err().expect("semantic error").to_string(),
                Err(error) => error.to_string(),
            }
        }

        let unknown = error(quote! {
            client Api {
                base "https://example.test";
                GET Read() -> Bytes { auth bearer missing; }
            }
        });
        assert!(unknown.contains("unknown credential `missing`"));

        let incompatible = error(quote! {
            client Api {
                base "https://example.test";
                credentials {
                    secret key_value: String;
                    credential key = api_key(secret.key_value);
                }
                GET Read() -> Bytes { auth bearer key; }
            }
        });
        assert!(incompatible.contains("incompatible"));

        let custom = error(quote! {
            client Api {
                base "https://example.test";
                credentials { credential session = custom crate::SessionToken; }
                GET Read() -> Bytes { auth bearer session; }
            }
        });
        assert!(custom.contains("requires an explicit challenge suffix"));

        let challengers = error(quote! {
            client Api {
                base "https://example.test";
                credentials {
                    secret first_value: String;
                    secret second_value: String;
                    credential first = bearer(secret.first_value);
                    credential second = api_key(secret.second_value);
                }
                GET Read() -> Bytes {
                    auth bearer first challenge unauthorized;
                    auth query "key" = second challenge unauthorized_or_forbidden;
                }
            }
        });
        assert!(challengers.contains("at most one challenge-enabled"));

        let authorization = error(quote! {
            client Api {
                base "https://example.test";
                credentials {
                    secret token: String;
                    credential bearer = bearer(secret.token);
                }
                headers { "Authorization" = "public"; }
                GET Read() -> Bytes { auth bearer bearer; }
            }
        });
        assert!(authorization.contains("reserved by transport, authentication"));

        let header = error(quote! {
            client Api {
                base "https://example.test";
                credentials {
                    secret key_value: String;
                    credential key = api_key(secret.key_value);
                }
                headers { "X-Key" = "public"; }
                GET Read() -> Bytes { auth header "x-key" = key; }
            }
        });
        assert!(header.contains("collides"));

        let query = error(quote! {
            client Api {
                base "https://example.test";
                credentials {
                    secret key_value: String;
                    credential key = api_key(secret.key_value);
                }
                query { "key" = "public"; }
                GET Read() -> Bytes { auth query "key" = key; }
            }
        });
        assert!(query.contains("collides"));

        let duplicate_auth = error(quote! {
            client Api {
                base "https://example.test";
                credentials {
                    secret first_value: String;
                    secret second_value: String;
                    credential first = api_key(secret.first_value);
                    credential second = api_key(secret.second_value);
                }
                GET Read() -> Bytes {
                    auth header "X-Key" = first;
                    auth header "x-key" = second;
                }
            }
        });
        assert!(duplicate_auth.contains("collides"));
    }

    #[test]
    fn phase6_optional_public_values_defer_material_collision_to_request_preparation() {
        resolve_ok(quote! {
            client Api {
                base "https://example.test";
                credentials {
                    secret key_value: String;
                    credential key = api_key(secret.key_value);
                }
                GET Optional(
                    query_value: Option<String>,
                    header_value: Option<String>,
                ) -> Bytes {
                    query { "key" = ep.query_value; }
                    headers { "X-Key" = ep.header_value; }
                    auth query "key" = key;
                    auth header "X-Key" = key;
                }
            }
        });
    }

    #[test]
    fn phase6_endpoint_backed_dependency_and_authentication_cycles_are_rejected() {
        fn error(tokens: proc_macro2::TokenStream) -> String {
            resolve(parse::parse(tokens).expect("source AST"))
                .err()
                .expect("cycle")
                .to_string()
        }

        let direct = error(quote! {
            client Api {
                base "https://example.test";
                credentials { credential session = endpoint Login; }
                GET Login() -> Bytes {
                    auth bearer session challenge never;
                }
            }
        });
        assert!(direct.contains("dependency or authentication cycle"));

        let mutual = error(quote! {
            client Api {
                base "https://example.test";
                credentials {
                    credential first = endpoint First;
                    credential second = endpoint Second;
                }
                GET First() -> Bytes {
                    auth bearer second challenge never;
                }
                GET Second() -> Bytes {
                    auth bearer first challenge never;
                }
            }
        });
        assert!(mutual.contains("dependency or authentication cycle"));
    }

    #[test]
    fn generated_name_collisions_cover_aliases_traits_and_generic_request_methods() {
        let support = resolve(
            parse::parse(quote! {
                client lower_api { base "https://example.test"; }
            })
            .expect("AST"),
        )
        .err()
        .expect("client/support module collision");
        assert!(support.to_string().contains("lower_api"));
        assert!(support.to_string().contains("collides"));

        let alias = resolve(
            parse::parse(quote! {
                client Api {
                    base "https://example.test";
                    GET First() as same -> Bytes;
                    GET Second() as same -> Bytes;
                }
            })
            .expect("AST"),
        )
        .err()
        .expect("alias collision");
        let alias = alias.to_string().to_ascii_lowercase();
        assert!(alias.contains("same"));
        assert!(alias.contains("collides"));

        let trait_item = resolve(
            parse::parse(quote! {
                client Api {
                    base "https://example.test";
                    GET Read(value: Option<String>) -> Bytes;
                    GET ReadRequestExt() -> Bytes;
                }
            })
            .expect("AST"),
        )
        .err()
        .expect("request extension trait collision");
        assert!(trait_item.to_string().contains("collides"));

        let request_method = resolve(
            parse::parse(quote! {
                client Api {
                    base "https://example.test";
                    GET Read(timeout: Option<u64>) -> Bytes;
                }
            })
            .expect("AST"),
        )
        .err()
        .expect("generic request method collision");
        assert!(request_method.to_string().contains("timeout"));
    }

    #[test]
    fn phase6_generated_names_cover_builder_accessor_and_acquisition_namespaces() {
        fn error(tokens: proc_macro2::TokenStream) -> String {
            resolve(parse::parse(tokens).expect("source AST"))
                .err()
                .expect("generated name collision")
                .to_string()
        }

        let reserved_secret = error(quote! {
            client Api {
                base "https://example.test";
                credentials {
                    secret build: String;
                    credential key = api_key(secret.build);
                }
            }
        });
        assert!(reserved_secret.contains("build"));
        assert!(reserved_secret.contains("collides"));

        let helper_secret = error(quote! {
            client Api {
                base "https://example.test";
                var token: Option<String>;
                credentials {
                    secret clear_token: String;
                    credential key = api_key(secret.clear_token);
                }
            }
        });
        assert!(helper_secret.contains("clear_token"));
        assert!(helper_secret.contains("collides"));

        let internal_accessor = error(quote! {
            client Api {
                base "https://example.test";
                credentials {
                    secret value: String;
                    credential __concord_new = api_key(secret.value);
                }
            }
        });
        assert!(internal_accessor.contains("__concord_new"));
        assert!(internal_accessor.contains("collides"));

        let acquisition_method = error(quote! {
            client Api {
                base "https://example.test";
                credentials { credential session = endpoint Login; }
                GET Login(acquire_as_session: Option<String>) -> Bytes;
            }
        });
        assert!(acquisition_method.contains("acquire_as_session"));
        assert!(acquisition_method.contains("collides"));

        let acquisition_trait = error(quote! {
            client Api {
                base "https://example.test";
                credentials { credential session = endpoint Read; }
                GET Read() -> Bytes;
                GET ReadRequestExt() -> Bytes;
            }
        });
        assert!(acquisition_trait.contains("collides"));
    }
}
