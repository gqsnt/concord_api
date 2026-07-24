use std::collections::HashMap;

mod auth;
mod inheritance;
pub(crate) mod model;
mod pagination;
mod rate_limits;
pub(crate) mod surface;
mod symbols;

use proc_macro2::Span;
use syn::Ident;

use self::model::{
    Api, Child, Client, ContractId, Credential, Endpoint, NormalizedConfig, RequestBody,
    ResolvedMethod, ResolvedRetry, Scope, Value,
};
use self::surface::{
    canonical_ident, client_surface, endpoint_surface, finalize_endpoint_surfaces, scope_surface,
    to_snake_case, validate_surface_modules,
};
use crate::syntax::ast::{
    MethodKind as SourceMethod, RetrySelection as SourceRetry, SourceApi, SourceChild,
    SourceEndpoint, SourceScope,
};
use auth::{
    convert_credentials, resolve_endpoint_credentials, validate_credential_cycles,
    validate_credential_names,
};
use rate_limits::source_tree_declares_rate_limits;
use symbols::{
    ScopeFrame, convert_values, validate_child_methods, validate_endpoint_setters,
    validate_setter_namespace,
};

struct Resolver<'a> {
    client_name: &'a Ident,
    client_values: &'a [Value],
    credentials: &'a [Credential],
    next_scope_id: usize,
    next_endpoint_id: usize,
}

pub(crate) fn resolve(source: SourceApi) -> syn::Result<Api> {
    let SourceApi {
        client: source_client,
        children: source_children,
    } = source;
    let source_declares_limiter =
        source_tree_declares_rate_limits(&source_client.layer, &source_children);
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
    let surface = client_surface(&client_name);

    let (config, children) = {
        let mut resolver = Resolver {
            client_name: &client_name,
            client_values: &values,
            credentials: &credentials,
            next_scope_id: 0,
            next_endpoint_id: 0,
        };
        let client_id = contract_path(&client_name, &[]);
        let config = resolver.normalize_layer(
            &source_client.layer,
            &NormalizedConfig::default(),
            &[],
            None,
            &client_id,
            "client",
        )?;
        let children = resolver.resolve_children(source_children, &config, &[], &[])?;
        (config, children)
    };
    validate_contract_ids(&children)?;
    resolve_endpoint_credentials(&mut credentials, &children)?;
    let mut children = children;
    finalize_endpoint_surfaces(&mut children, &credentials)?;
    validate_surface_modules(&children, None, "generated client module")?;
    validate_credential_cycles(&credentials, &children)?;
    validate_child_methods(&children, &[], "generated client", true)?;

    let client = Client {
        name: client_name,
        surface,
        base: source_client.base.value,
        values,
        secrets,
        credentials,
        retry: resolve_retry(source_client.retry.map(|clause| clause.value)),
        requires_limiter: source_declares_limiter,
        config,
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
        validate_setter_namespace(&parameters, "scope facade", false, false)?;

        let mut frames = visible_scopes.to_vec();
        frames.push(ScopeFrame {
            id,
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
        let config = self.normalize_layer(
            &source.layer,
            parent,
            &frames,
            None,
            &id_path,
            &lexical_origin,
        )?;
        let children = self.resolve_children(source.children, &config, &frames, &path)?;
        validate_child_methods(&children, &parameters, "scope facade", false)?;

        let surface = scope_surface(&source.name);
        Ok(Scope {
            id,
            name: source.name,
            surface,
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
        let config = self.normalize_layer(
            &source.layer,
            parent,
            visible_scopes,
            Some(&parameters),
            &contract_id.value,
            &lexical_origin,
        )?;
        let pagination = self.resolve_pagination(
            source.pagination.map(|clause| clause.value),
            visible_scopes,
            &parameters,
        )?;
        let body_position = source.body.as_ref().map(|body| body.position);
        let body = source.body.map(|body| RequestBody {
            name: body.name,
            ty: body.ty,
        });
        let surface = endpoint_surface(
            self.client_name,
            scope_path,
            &source.name,
            &parameters,
            body_position,
            pagination.is_some(),
        );
        Ok(Endpoint {
            id,
            method: resolve_method(source.method),
            name: source.name,
            surface,
            parameters,
            body,
            contract_id,
            config,
            response: source.response,
            pagination,
            dsl_path,
        })
    }
}

fn resolve_retry(source: Option<SourceRetry>) -> ResolvedRetry {
    match source.unwrap_or(SourceRetry::Disabled) {
        SourceRetry::Disabled => ResolvedRetry::Disabled,
        SourceRetry::ReqwestDefault => ResolvedRetry::ReqwestDefault,
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
        SourceMethod::Trace => ResolvedMethod::Trace,
        SourceMethod::Connect => ResolvedMethod::Connect,
        SourceMethod::Custom(value) => ResolvedMethod::Custom(value),
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
        let source = crate::syntax::parse(tokens).expect("semantic test syntax");
        resolve(source).expect("normalized semantic model")
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

    pub(super) fn endpoint<'a>(children: &'a [Child], name: &str) -> &'a Endpoint {
        find_endpoint(children, name).unwrap_or_else(|| panic!("endpoint `{name}`"))
    }
}

#[cfg(test)]
mod tests;
