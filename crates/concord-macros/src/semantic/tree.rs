//! Owner/endpoint tree flattening and value registration.

use std::collections::HashMap;

use proc_macro2::Span;
use syn::Ident;

use super::surface::{canonical_ident, pascal_ident, private_prefixed_ident, snake_ident};
use super::{
    Body, Child, ClientInfo, DirectPlacement, Endpoint, EndpointSurface, FamilyMode, Initializer,
    LocalProgram, Method, Model, Owner, OwnerKind, OwnerSurface, Positional, RepresentationDefault,
    Resolver, Status, Value, ValueControls, meta_from,
};
use crate::syntax::ast::{
    AttachmentSite, Attrs, BodyDecl, MethodKind, SourceApi, SourceChild, SourceEndpoint, ValueDecl,
};
use crate::syntax::diagnostics::{Code, client_label, coded, endpoint_label, related, scope_label};

/// Attachment syntax of one owner, retained until resolution.
pub(crate) struct OwnerSource {
    pub(crate) site: AttachmentSite,
}

pub(crate) fn build(api: SourceApi) -> syn::Result<Resolver> {
    let client = api.client;
    let client_logical = canonical_ident(&client.name);
    let module = snake_ident(&client.name);
    if client_logical == canonical_ident(&module) {
        return Err(coded(
            client.name.span(),
            Code::Generated,
            format!(
                "client `{client_logical}` emits a client type `{client_logical}` and support module `{client_logical}` in the same Rust namespace; rename the client so its type and snake-case support module have distinct names"
            ),
        ));
    }
    let builder = Ident::new(&format!("{client_logical}Builder"), client.name.span());
    let mut state = TreeState {
        owners: Vec::new(),
        endpoints: Vec::new(),
        owner_sources: Vec::new(),
        endpoint_sources: Vec::new(),
    };
    let values = client.values.map(|clause| clause.value).unwrap_or_default();
    let root = Owner {
        kind: OwnerKind::Client,
        parent: None,
        depth: 0,
        logical: client_logical.clone(),
        display: client_logical.clone(),
        meta: meta_from(&client.attrs),
        values: register_values(&values, "client value", &client_label(&client_logical))?,
        positional: Vec::new(),
        host: None,
        path: None,
        program: empty_program(),
        auth: None,
        timeout: None,
        request_choice: None,
        response_choice: None,
        rates: Vec::new(),
        effective_timeout: None,
        effective_auth: Vec::new(),
        effective_rates: Vec::new(),
        request_default: RepresentationDefault::default(),
        response_default: RepresentationDefault::default(),
        children: Vec::new(),
        surface: OwnerSurface {
            handle: client.name.clone(),
            module_path: vec![module.clone()],
            method: None,
        },
        span: client.name.span(),
    };
    reject_client_attrs(&client.attrs, &client_label(&client_logical))?;
    state.owners.push(root);
    state.owner_sources.push(OwnerSource { site: client.site });
    let children = state.children(0, client.children)?;
    state.owners[0].children = children;
    let model = Model {
        client: ClientInfo {
            name: client.name.clone(),
            logical: client_logical,
            base: client.base.value,
            builder,
            module,
            secrets: Vec::new(),
            credentials: Vec::new(),
            window_sets: Vec::new(),
            policies: Vec::new(),
            requires_limiter: false,
        },
        owners: state.owners,
        endpoints: state.endpoints,
    };
    let owner_count = model.owners.len();
    let endpoint_count = model.endpoints.len();
    Ok(Resolver {
        model,
        owner_sources: state.owner_sources,
        endpoint_sources: state.endpoint_sources,
        credentials_source: client.credentials.map(|clause| clause.value),
        registry_source: client.rate_registry.map(|clause| clause.value),
        owner_potential: vec![super::head::Potential::default(); owner_count],
        endpoint_potential: vec![super::head::Potential::default(); endpoint_count],
    })
}

fn reject_client_attrs(attrs: &Attrs, owner: &str) -> syn::Result<()> {
    if let Some(span) = attrs.sensitive.or(attrs.default) {
        return Err(coded(
            span,
            Code::Owner,
            format!(
                "`#[sensitive]` and `#[default]` apply only to ordinary value declarations, not to {owner}; move the attribute onto a value"
            ),
        ));
    }
    Ok(())
}

pub(crate) fn empty_program() -> LocalProgram {
    LocalProgram {
        query_mode: FamilyMode::Inherit,
        header_mode: FamilyMode::Inherit,
        ops: Vec::new(),
    }
}

struct TreeState {
    owners: Vec<Owner>,
    endpoints: Vec<Endpoint>,
    owner_sources: Vec<OwnerSource>,
    endpoint_sources: Vec<SourceEndpoint>,
}

impl TreeState {
    fn children(&mut self, parent: usize, children: Vec<SourceChild>) -> syn::Result<Vec<Child>> {
        let mut out = Vec::with_capacity(children.len());
        for child in children {
            match child {
                SourceChild::Scope(scope) => {
                    let scope = *scope;
                    let scope_path = format!(
                        "{}::{}",
                        self.owners[parent].display,
                        canonical_ident(&scope.name)
                    );
                    reject_client_attrs(&scope.attrs, &scope_label(&scope_path))?;
                    let logical = canonical_ident(&scope.name);
                    if matches!(logical.as_str(), "client" | "endpoint")
                        || logical.starts_with("__concord")
                    {
                        return Err(coded(
                            scope.name.span(),
                            Code::Name,
                            format!(
                                "`{logical}` is a reserved owner name; rename {}",
                                scope_label(&scope_path)
                            ),
                        ));
                    }
                    // A visible scope identifier is unique along one ancestor chain.
                    let mut ancestor = Some(parent);
                    while let Some(current) = ancestor {
                        if current != 0 && self.owners[current].logical == logical {
                            let error = coded(
                                scope.name.span(),
                                Code::Name,
                                format!(
                                    "{} repeats the name of its enclosing {}; qualified references would be ambiguous, so rename one of the scopes",
                                    scope_label(&scope_path),
                                    scope_label(&self.owners[current].display)
                                ),
                            );
                            return Err(related(
                                error,
                                self.owners[current].span,
                                "the enclosing scope is declared here",
                            ));
                        }
                        ancestor = self.owners[current].parent;
                    }
                    let parent_owner = &self.owners[parent];
                    let mut module_path = parent_owner.surface.module_path.clone();
                    module_path.push(snake_ident(&scope.name));
                    let depth = parent_owner.depth + 1;
                    let display = format!("{}::{logical}", parent_owner.display);
                    let (values, body) = (scope.parameters, None::<BodyDecl>);
                    debug_assert!(body.is_none());
                    let registered =
                        register_values(&values, "scope parameter", &scope_label(&display))?;
                    let positional = registered
                        .iter()
                        .enumerate()
                        .filter(|(_, value)| value.is_required_input())
                        .map(|(index, _)| index)
                        .collect();
                    let index = self.owners.len();
                    self.owners.push(Owner {
                        kind: OwnerKind::Scope,
                        parent: Some(parent),
                        depth,
                        logical,
                        display,
                        meta: meta_from(&scope.attrs),
                        values: registered,
                        positional,
                        host: None,
                        path: None,
                        program: empty_program(),
                        auth: None,
                        timeout: None,
                        request_choice: None,
                        response_choice: None,
                        rates: Vec::new(),
                        effective_timeout: None,
                        effective_auth: Vec::new(),
                        effective_rates: Vec::new(),
                        request_default: RepresentationDefault::default(),
                        response_default: RepresentationDefault::default(),
                        children: Vec::new(),
                        surface: OwnerSurface {
                            handle: pascal_ident(&scope.name),
                            module_path,
                            method: Some(scope.name.clone()),
                        },
                        span: scope.name.span(),
                    });
                    self.owner_sources.push(OwnerSource { site: scope.site });
                    let grandchildren = self.children(index, scope.children)?;
                    self.owners[index].children = grandchildren;
                    out.push(Child::Scope(index));
                }
                SourceChild::Endpoint(endpoint) => {
                    let endpoint = *endpoint;
                    let index = self.endpoints.len();
                    let resolved = self.endpoint(parent, &endpoint)?;
                    self.endpoints.push(resolved);
                    self.endpoint_sources.push(endpoint);
                    out.push(Child::Endpoint(index));
                }
            }
        }
        Ok(out)
    }

    fn endpoint(&self, owner: usize, source: &SourceEndpoint) -> syn::Result<Endpoint> {
        reject_client_attrs(
            &source.attrs,
            &endpoint_label(&format!(
                "{}::{}",
                self.owners[owner].display,
                canonical_ident(&source.name)
            )),
        )?;
        let logical = canonical_ident(&source.name);
        let display = format!("{}::{logical}", self.owners[owner].display);
        let values = register_values(
            &source.parameters,
            "endpoint parameter",
            &endpoint_label(&display),
        )?;
        let body = source.body.as_ref().map(register_body);
        let mut positional: Vec<(usize, Positional)> = values
            .iter()
            .enumerate()
            .filter(|(_, value)| value.is_required_input())
            .map(|(index, value)| (value.position, Positional::Value(index)))
            .collect();
        if let Some(body) = &body {
            if !body.optional {
                positional.push((body.position, Positional::Body));
            }
        }
        positional.sort_by_key(|(position, _)| *position);
        let method = match &source.method {
            MethodKind::Get => Method::Get,
            MethodKind::Post => Method::Post,
            MethodKind::Put => Method::Put,
            MethodKind::Patch => Method::Patch,
            MethodKind::Delete => Method::Delete,
            MethodKind::Head => Method::Head,
            MethodKind::Options => Method::Options,
            MethodKind::Custom(literal) => Method::Custom(literal.value()),
        };
        let pascal = pascal_ident(&source.name);
        let pascal_text = pascal.to_string();
        let span = source.name.span();
        Ok(Endpoint {
            owner,
            logical,
            display,
            meta: meta_from(&source.attrs),
            effect: method.default_effect(),
            method,
            values,
            body,
            positional: positional
                .into_iter()
                .map(|(_, positional)| positional)
                .collect(),
            output: source.output.clone(),
            output_via: source.output_via.clone(),
            response: source.output.clone(),
            response_origin: String::new(),
            id: String::new(),
            id_explicit: false,
            id_span: span,
            status: Status::Success,
            status_explicit: false,
            effect_explicit: false,
            absence: None,
            absence_span: span,
            pagination: None,
            expose_accepted_bytes: source.expose_accepted_bytes.is_some(),
            path: None,
            program: empty_program(),
            auth: None,
            timeout: None,
            rates: Vec::new(),
            effective_timeout: None,
            effective_auth: Vec::new(),
            effective_rates: Vec::new(),
            storage: Vec::new(),
            surface: EndpointSurface {
                method: source.name.clone(),
                endpoint_type: Ident::new(&format!("{pascal_text}Endpoint"), span),
                request_type: Ident::new(&format!("{pascal_text}Request"), span),
                spec_type: Ident::new(&format!("__{pascal_text}Spec"), span),
                local_type: Ident::new(&format!("__{pascal_text}Local"), span),
            },
            span,
        })
    }
}

fn register_values(values: &[ValueDecl], what: &str, owner: &str) -> syn::Result<Vec<Value>> {
    let mut seen: HashMap<String, Span> = HashMap::new();
    values
        .iter()
        .map(|declaration| {
            let logical = canonical_ident(&declaration.name);
            if let Some(first) = seen.insert(logical.clone(), declaration.name.span()) {
                let error = coded(
                    declaration.name.span(),
                    Code::Name,
                    format!("duplicate {what} `{logical}` in {owner}; rename one of them"),
                );
                return Err(related(error, first, "the first declaration is here"));
            }
            let init = if let Some(span) = declaration.attrs.default {
                Initializer::Default(span)
            } else if let Some(value) = &declaration.initializer {
                Initializer::Stable(value.clone())
            } else {
                Initializer::None
            };
            let placement = declaration
                .placement
                .as_ref()
                .map(|placement| DirectPlacement {
                    family: placement.family,
                    key: placement
                        .key
                        .as_ref()
                        .map_or_else(|| logical.clone(), syn::LitStr::value),
                    each: placement.each,
                    span: placement.span,
                });
            Ok(Value {
                name: declaration.name.clone(),
                logical: logical.clone(),
                meta: meta_from(&declaration.attrs),
                sensitive: declaration.attrs.sensitive.is_some(),
                optional: declaration.optional,
                ty: declaration.ty.clone(),
                via: declaration.via.clone(),
                init,
                placement,
                position: declaration.position,
                field: private_prefixed_ident("v", &declaration.name),
                initial_field: private_prefixed_ident("i", &declaration.name),
                controls: ValueControls::default(),
                live: false,
                page_live: false,
            })
        })
        .collect()
}

fn register_body(body: &BodyDecl) -> Body {
    Body {
        name: body.name.clone(),
        logical: canonical_ident(&body.name),
        meta: meta_from(&body.attrs),
        optional: body.optional,
        ty: body.ty.clone(),
        via: body.via.clone(),
        representation: body.ty.clone(),
        representation_origin: String::new(),
        position: body.position,
        controls: ValueControls::default(),
    }
}

#[cfg(test)]
mod s4_client_module_collision_regressions {
    #[test]
    fn client_and_support_module_names_must_not_collide() {
        for name in ["r#type", "catalog", "already_snake"] {
            let source = format!(
                "client {name} {{ base \"https://fixture.test/\"; GET read() -> () via Empty; }}"
            );
            let parsed = crate::syntax::parse(source.parse().unwrap()).unwrap();
            let error = crate::semantic::resolve(parsed)
                .err()
                .expect("ambiguous generated symbols");
            assert!(error.to_string().contains("C3-GENERATED"));
            assert!(error.to_string().contains("client type"));
            assert!(error.to_string().contains("support module"));
        }
        let parsed = crate::syntax::parse(
            "client r#HTTPStatus { base \"https://fixture.test/\"; GET read() -> () via Empty; }"
                .parse()
                .unwrap(),
        )
        .unwrap();
        let model = crate::semantic::resolve(parsed).unwrap();
        assert_eq!(model.endpoints[0].id, "http_status.read");
    }
}
