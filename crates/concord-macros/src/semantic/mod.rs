use std::collections::HashMap;

mod auth;
mod head_limits;
mod inheritance;
pub(crate) mod model;
mod pagination;
mod rate_limits;
pub(crate) mod surface;
mod symbols;

use proc_macro2::Span;
use syn::Ident;

use self::model::{
    Api, AttachmentFamily, AttachmentIr, AttachmentOperation, Child, Client, ContractId,
    Credential, DeclarationMetadata, Endpoint, NormalizedConfig, Repeatability, RequestBody,
    ResolvedAccept, ResolvedMethod, Scope, Value,
};
use self::surface::{
    canonical_ident, client_surface, endpoint_surface, finalize_endpoint_surfaces, scope_surface,
    to_snake_case, validate_surface_modules,
};
use crate::syntax::ast::{
    AcceptOperation, DeclarationAttributes, MethodKind as SourceMethod, RepeatabilityDecl,
    SourceApi, SourceChild, SourceEndpoint, SourceScope,
};
use auth::{
    convert_credentials, resolve_endpoint_credentials, validate_credential_cycles,
    validate_credential_names,
};
use inheritance::{ExpressionUse, local_attachment_ir};
use pagination::{pagination_bound_values, validate_pagination_binding_destinations};
use rate_limits::{
    RateLimitDefinition, RateLimitResolutionContext, resolve_rate_limit_definitions,
    source_tree_declares_rate_limits, validate_effective_rate_limit_pagination_dependencies,
    validate_rate_limit_definitions_used,
};
use symbols::{
    ScopeFrame, convert_values, validate_child_methods, validate_endpoint_setters,
    validate_setter_namespace,
};

type RateLimitWindowSchema = (u32, std::time::Duration);
type RateLimitSchema = (Vec<String>, Vec<RateLimitWindowSchema>, Span);

struct Resolver<'a> {
    base: &'a syn::LitStr,
    client_name: &'a Ident,
    client_values: &'a [Value],
    credentials: &'a [Credential],
    rate_limit_definitions: Vec<RateLimitDefinition>,
    rate_limit_ids: HashMap<String, RateLimitSchema>,
    next_scope_id: usize,
    next_endpoint_id: usize,
}

pub(crate) fn resolve(source: SourceApi) -> syn::Result<Api> {
    let SourceApi {
        client: source_client,
    } = source;
    let source_children = source_client.children.clone();
    let source_declares_limiter =
        source_tree_declares_rate_limits(&source_client.attachment_site, &source_children);
    let rate_limit_definitions = resolve_rate_limit_definitions(
        &source_client.name,
        source_client
            .rate_limits
            .map(|clause| clause.value)
            .unwrap_or_default(),
    )?;
    let values = convert_values(
        source_client
            .values
            .map(|clause| clause.value)
            .unwrap_or_default(),
    )?;
    validate_setter_namespace(&values, "client builder", true, true)?;
    let (secrets, mut credentials) =
        convert_credentials(source_client.credentials.map(|clause| clause.value))?;
    validate_credential_names(&values, &secrets, &credentials)?;

    let client_name = source_client.name;
    let base = source_client.base.value;
    let surface = client_surface(&client_name);
    let local_attachments = local_attachment_ir(&source_client.attachment_site, "client");
    let (config, children, rate_limit_profiles) = {
        let mut resolver = Resolver {
            base: &base,
            client_name: &client_name,
            client_values: &values,
            credentials: &credentials,
            rate_limit_definitions,
            rate_limit_ids: HashMap::new(),
            next_scope_id: 0,
            next_endpoint_id: 0,
        };
        let client_id = contract_path(&client_name, &[]);
        let config = resolver.normalize_attachment_site(
            &source_client.attachment_site,
            &NormalizedConfig::default(),
            &[],
            None,
            &RateLimitResolutionContext {
                id_path: &client_id,
                lexical_origin: "client",
            },
        )?;
        let children = resolver.resolve_children(source_children, &config, &[], &[])?;
        validate_rate_limit_definitions_used(&resolver.rate_limit_definitions)?;
        let rate_limit_profiles = resolver
            .rate_limit_definitions
            .iter()
            .map(RateLimitDefinition::profile)
            .collect();
        (config, children, rate_limit_profiles)
    };
    validate_contract_ids(&children)?;
    resolve_endpoint_credentials(&mut credentials, &children)?;
    let mut children = children;
    finalize_endpoint_surfaces(&mut children, &credentials)?;
    validate_surface_modules(&children, None, "generated client module")?;
    validate_credential_cycles(&credentials, &children)?;
    validate_child_methods(&children, &[], "client", true)?;

    let client = Client {
        attributes: declaration_metadata(source_client.attributes),
        name: client_name,
        surface,
        base,
        values,
        secrets,
        credentials,
        rate_limit_profiles,
        requires_limiter: source_declares_limiter,
        config,
        local_attachments,
        children,
    };

    Ok(Api { client })
}

impl Resolver<'_> {
    fn resolve_children(
        &mut self,
        source: Vec<SourceChild>,
        parent: &NormalizedConfig,
        visible_scopes: &[ScopeFrame],
        scope_path: &[Ident],
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
        if let Some(existing) = visible_scopes
            .iter()
            .find(|scope| canonical_ident(&scope.name) == canonical_ident(&source.name))
        {
            let mut error = source.name.span().error(format!(
                "scope identifier `{}` duplicates an ancestor and would make owner references ambiguous",
                canonical_ident(&source.name)
            ));
            error.combine(
                existing
                    .name
                    .span()
                    .error("the ancestor scope is declared here"),
            );
            return Err(error);
        }
        validate_setter_namespace(&parameters, "scope facade", false, false)?;

        let mut frames = visible_scopes.to_vec();
        frames.push(ScopeFrame {
            id,
            name: source.name.clone(),
            values: parameters.clone(),
        });
        let mut path = scope_path.to_vec();
        path.push(source.name.clone());
        let id_path = contract_path(self.client_name, &path);
        let lexical_origin = format!(
            "scope {}",
            path.iter()
                .map(canonical_ident)
                .collect::<Vec<_>>()
                .join("::")
        );
        let local_attachments = local_attachment_ir(&source.attachment_site, &lexical_origin);
        let config = self.normalize_attachment_site(
            &source.attachment_site,
            parent,
            &frames,
            None,
            &RateLimitResolutionContext {
                id_path: &id_path,
                lexical_origin: &lexical_origin,
            },
        )?;
        let children = self.resolve_children(source.children, &config, &frames, &path)?;
        validate_child_methods(&children, &parameters, "scope facade", false)?;

        let surface = scope_surface(&source.name);
        Ok(Scope {
            id,
            attributes: declaration_metadata(source.attributes),
            name: source.name,
            surface,
            parameters,
            config,
            local_attachments,
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
        validate_endpoint_setters(&parameters)?;
        let pagination = self.resolve_pagination(
            source.pagination.map(|clause| clause.value),
            visible_scopes,
            &parameters,
        )?;
        let pagination_values = pagination_bound_values(
            pagination.as_ref(),
            self.client_values,
            visible_scopes,
            &parameters,
        );

        let dsl_path = scope_path
            .iter()
            .cloned()
            .chain(std::iter::once(source.name.clone()))
            .collect::<Vec<_>>();
        let contract_id = source.contract_id.map_or_else(
            || ContractId {
                value: contract_path(self.client_name, &dsl_path),
                explicit: false,
                span: source.name.span(),
            },
            |clause| ContractId {
                value: clause.value.value(),
                explicit: true,
                span: clause.value.span(),
            },
        );
        let lexical_origin = format!(
            "endpoint {}",
            dsl_path
                .iter()
                .map(canonical_ident)
                .collect::<Vec<_>>()
                .join("::")
        );
        let mut local_attachments = local_attachment_ir(&source.attachment_site, &lexical_origin);
        if let Some(accept) = &source.accept {
            local_attachments.push(AttachmentIr {
                family: AttachmentFamily::Accept,
                operation: match &accept.value {
                    AcceptOperation::Off => AttachmentOperation::Off,
                    AcceptOperation::Value(_) => AttachmentOperation::Set,
                },
                origin: lexical_origin.clone(),
                target: None,
                item_count: usize::from(matches!(&accept.value, AcceptOperation::Value(_))),
            });
        }
        let config = self.normalize_attachment_site(
            &source.attachment_site,
            parent,
            visible_scopes,
            Some(&parameters),
            &RateLimitResolutionContext {
                id_path: &contract_id.value,
                lexical_origin: &lexical_origin,
            },
        )?;
        validate_effective_rate_limit_pagination_dependencies(&config, &pagination_values)?;
        validate_pagination_binding_destinations(&pagination_values)?;
        head_limits::validate_static_head(self.base, &config, source.name.span())?;
        let body_position = source.body.as_ref().map(|body| body.position);
        let body = source.body.map(|body| RequestBody {
            name: body.name,
            input_type: body.input_type,
            spec_type: body.spec_type,
            docs: body.docs.into_iter().map(|doc| doc.value()).collect(),
        });
        let method = resolve_method(source.method);
        let output = source.output;
        let response_spec = source.response_spec;
        let repeatability_explicit = source.repeatability.is_some();
        let repeatability = source.repeatability.map_or_else(
            || method_default_repeatability(&method),
            |clause| match clause.value {
                RepeatabilityDecl::Idempotent => Repeatability::Idempotent,
                RepeatabilityDecl::Never => Repeatability::Never,
            },
        );
        let expose_accepted_bytes = source.expose_accepted_bytes.is_some();
        let accept = source
            .accept
            .map(|clause| match clause.value {
                AcceptOperation::Off => Ok(ResolvedAccept::Off),
                AcceptOperation::Value(value) => self
                    .resolve_expr(
                        &value,
                        visible_scopes,
                        Some(&parameters),
                        ExpressionUse::Single,
                    )
                    .map(ResolvedAccept::Value),
            })
            .transpose()?;
        let surface = endpoint_surface(
            self.client_name,
            scope_path,
            &source.name,
            &parameters,
            body_position,
            pagination.is_some(),
            expose_accepted_bytes,
        );
        Ok(Endpoint {
            id,
            attributes: declaration_metadata(source.attributes),
            method,
            name: source.name,
            surface,
            parameters,
            body,
            contract_id,
            config,
            local_attachments,
            output,
            response_spec,
            pagination,
            dsl_path,
            repeatability,
            repeatability_explicit,
            accept,
            expose_accepted_bytes,
        })
    }
}

fn resolve_method(source: SourceMethod) -> ResolvedMethod {
    match source {
        SourceMethod::Get => ResolvedMethod::Get,
        SourceMethod::Post => ResolvedMethod::Post,
        SourceMethod::Put => ResolvedMethod::Put,
        SourceMethod::Patch => ResolvedMethod::Patch,
        SourceMethod::Delete => ResolvedMethod::Delete,
        SourceMethod::Head => ResolvedMethod::Head,
        SourceMethod::Options => ResolvedMethod::Options,
        SourceMethod::Custom(value) => ResolvedMethod::Custom(value),
    }
}

fn method_default_repeatability(method: &ResolvedMethod) -> Repeatability {
    match method {
        ResolvedMethod::Get
        | ResolvedMethod::Put
        | ResolvedMethod::Delete
        | ResolvedMethod::Head
        | ResolvedMethod::Options => Repeatability::Idempotent,
        ResolvedMethod::Custom(method) if method.value() == "TRACE" => Repeatability::Idempotent,
        ResolvedMethod::Post | ResolvedMethod::Patch | ResolvedMethod::Custom(_) => {
            Repeatability::Never
        }
    }
}

fn declaration_metadata(source: DeclarationAttributes) -> DeclarationMetadata {
    DeclarationMetadata {
        docs: source.docs.into_iter().map(|doc| doc.value()).collect(),
        deprecated: source.deprecated.map(|note| note.value()),
    }
}

fn contract_path(client: &Ident, path: &[Ident]) -> String {
    std::iter::once(to_snake_case(&canonical_ident(client)))
        .chain(path.iter().map(canonical_ident))
        .collect::<Vec<_>>()
        .join(".")
}

fn validate_contract_ids(children: &[Child]) -> syn::Result<()> {
    fn visit(children: &[Child], ids: &mut HashMap<String, Span>) -> syn::Result<()> {
        for child in children {
            match child {
                Child::Scope(scope) => visit(&scope.children, ids)?,
                Child::Endpoint(endpoint) => {
                    if let Some(first) = ids.insert(
                        endpoint.contract_id.value.clone(),
                        endpoint.contract_id.span,
                    ) {
                        let mut error = endpoint.contract_id.span.error(format!(
                            "endpoint contract ID `{}` is not unique within this client",
                            endpoint.contract_id.value
                        ));
                        error.combine(
                            first.error("the first endpoint using this contract ID is here"),
                        );
                        return Err(error);
                    }
                }
            }
        }
        Ok(())
    }

    visit(children, &mut HashMap::new())
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
pub(crate) mod test_support {
    use super::*;

    pub(crate) fn resolve_tokens(tokens: proc_macro2::TokenStream) -> Api {
        try_resolve_tokens(tokens).expect("normalized semantic model")
    }

    pub(crate) fn try_resolve_tokens(tokens: proc_macro2::TokenStream) -> syn::Result<Api> {
        let source = crate::syntax::parse(tokens)?;
        resolve(source)
    }

    fn find_endpoint<'a>(children: &'a [Child], name: &str) -> Option<&'a Endpoint> {
        for child in children {
            match child {
                Child::Endpoint(endpoint) if endpoint.name == name => return Some(endpoint),
                Child::Scope(scope) => {
                    if let Some(endpoint) = find_endpoint(&scope.children, name) {
                        return Some(endpoint);
                    }
                }
                Child::Endpoint(_) => {}
            }
        }
        None
    }

    pub(crate) fn endpoint<'a>(children: &'a [Child], name: &str) -> &'a Endpoint {
        find_endpoint(children, name).unwrap_or_else(|| panic!("endpoint `{name}`"))
    }
}

#[cfg(test)]
mod tests;
