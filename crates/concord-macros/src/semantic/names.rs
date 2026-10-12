//! Liveness (VALUE-06), operational identities (IDENTITY-01), value
//! controls (§4.3) and generated-name collision analysis (CALL-01).

use std::collections::HashMap;

use proc_macro2::Span;
use syn::Ident;

use super::surface::{prefixed_ident, suffixed_ident, to_snake_case};
use super::{
    BoundDim, Child, HeadOp, InitValue, LocalHost, LocalPath, OwnerKind, RateUse, Resolver,
    SegPart, Site, StatementKind, VRef, Value, ValueControls,
};
use crate::syntax::diagnostics::{Code, coded, is_valid_endpoint_id, related};

// ---- liveness ----

pub(crate) fn resolve_liveness(resolver: &mut Resolver) -> syn::Result<()> {
    let mut live: Vec<VRef> = Vec::new();
    let collect_program = |ops: &[HeadOp], live: &mut Vec<VRef>| {
        for (index, op) in ops.iter().enumerate() {
            let Some(emission) = &op.emission else {
                continue;
            };
            // An emission unconditionally suppressed by a later operation of
            // the same site is dead on every symbolic branch.
            let suppressed = ops[index + 1..].iter().any(|later| {
                later.family == op.family
                    && super::head::same_key(op.family, &later.key, &op.key)
                    && matches!(
                        later.kind,
                        Some(StatementKind::Remove | StatementKind::Replace)
                    )
            });
            if !suppressed {
                live.extend(emission.values());
            }
        }
    };
    let collect_path = |path: &Option<LocalPath>, live: &mut Vec<VRef>| {
        if let Some(LocalPath::Append(template) | LocalPath::Replace(template)) = path {
            for segment in &template.segments {
                for part in &segment.parts {
                    if let SegPart::Value { value, .. } = part {
                        live.push(*value);
                    }
                }
            }
        }
    };
    let collect_rates = |rates: &[RateUse], live: &mut Vec<VRef>| {
        for rate in rates {
            for dim in &rate.dims {
                if let BoundDim::Custom { value, .. } = dim {
                    live.push(*value);
                }
            }
        }
    };
    for owner in &resolver.model.owners {
        collect_program(&owner.program.ops, &mut live);
        collect_path(&owner.path, &mut live);
        if let Some(LocalHost::Prepend(labels) | LocalHost::Replace(labels)) = &owner.host {
            for label in labels {
                label.atom.values(&mut live);
            }
        }
        collect_rates(&owner.effective_rates, &mut live);
    }
    for endpoint in &resolver.model.endpoints {
        collect_program(&endpoint.program.ops, &mut live);
        collect_path(&endpoint.path, &mut live);
        collect_rates(&endpoint.effective_rates, &mut live);
        if let Some(pagination) = &endpoint.pagination {
            for (_, value) in &pagination.fields {
                if let InitValue::Value(value) = value {
                    live.push(*value);
                }
            }
        }
    }
    for key in live {
        match key.site {
            Site::Owner(owner) => resolver.model.owners[owner].values[key.index].live = true,
            Site::Endpoint(endpoint) => {
                resolver.model.endpoints[endpoint].values[key.index].live = true
            }
        }
    }
    for endpoint in &mut resolver.model.endpoints {
        if let Some(pagination) = &endpoint.pagination {
            for index in &pagination.cursor {
                endpoint.values[*index].live = true;
            }
        }
    }
    let check = |value: &Value, label: &str| -> syn::Result<()> {
        if value.live {
            return Ok(());
        }
        if value.is_required_input() {
            return Err(coded(
                value.name.span(),
                Code::Name,
                format!(
                    "required value `{}` of {label} is used nowhere; every input needs a reachable semantic use (a wire expression, path/DNS component, rate dimension or pagination mapping), so place it (for example `query {}: T`), reference it, or remove it",
                    value.logical, value.logical
                ),
            ));
        }
        if !value.is_deprecated() {
            return Err(coded(
                value.name.span(),
                Code::Name,
                format!(
                    "value `{}` of {label} has no reachable semantic use; use it, remove it, or mark it `#[deprecated]` as an explicit no-op migration bridge",
                    value.logical
                ),
            ));
        }
        Ok(())
    };
    for (index, owner) in resolver.model.owners.iter().enumerate() {
        let label = resolver.model.owner_label(index);
        for value in &owner.values {
            check(value, &label)?;
        }
    }
    for (index, endpoint) in resolver.model.endpoints.iter().enumerate() {
        let label = resolver.model.site_label(Site::Endpoint(index));
        for value in &endpoint.values {
            check(value, &label)?;
        }
    }
    Ok(())
}

// ---- identities ----

pub(crate) fn resolve_identities(resolver: &mut Resolver) -> syn::Result<()> {
    let client = to_snake_case(&resolver.model.client.logical);
    let mut seen: HashMap<String, (usize, Span)> = HashMap::new();
    for index in 0..resolver.model.endpoints.len() {
        let endpoint = &resolver.model.endpoints[index];
        if !endpoint.id_explicit {
            let mut segments = vec![client.clone()];
            for owner in resolver.model.chain(endpoint.owner).into_iter().skip(1) {
                segments.push(resolver.model.owners[owner].logical.clone());
            }
            segments.push(endpoint.logical.clone());
            let derived = segments.join(".");
            if !is_valid_endpoint_id(&derived) {
                return Err(coded(
                    endpoint.span,
                    Code::Name,
                    format!(
                        "the navigation-derived endpoint ID `{derived}` of {} is not a valid operational ID (lowercase dot-separated segments of at most 63 bytes, at most 128 bytes); declare an explicit `id \"...\";`",
                        resolver.model.site_label(Site::Endpoint(index))
                    ),
                ));
            }
            resolver.model.endpoints[index].id = derived;
        }
        let endpoint = &resolver.model.endpoints[index];
        if let Some((_, first)) = seen.insert(endpoint.id.clone(), (index, endpoint.id_span)) {
            let error = coded(
                endpoint.id_span,
                Code::Name,
                format!(
                    "endpoint ID `{}` of {} is used by two endpoints of {}; declare a distinct `id \"...\";` on one of them",
                    endpoint.id,
                    resolver.model.site_label(Site::Endpoint(index)),
                    resolver.model.owner_label(0)
                ),
            );
            return Err(related(
                error,
                first,
                "the other endpoint with this ID is here",
            ));
        }
    }
    Ok(())
}

// ---- controls and collisions ----

fn controls_for(value: &Value, root: bool) -> ValueControls {
    let set = Some(prefixed_ident("set", &value.name));
    let replace = Some(prefixed_ident("replace", &value.name));
    let set_if_some = Some(suffixed_ident(
        &prefixed_ident("set", &value.name),
        "if_some",
    ));
    let omit = Some(prefixed_ident("omit", &value.name));
    let reset = Some(prefixed_ident("reset", &value.name));
    match (value.optional, value.is_initialized()) {
        (false, false) if root => ValueControls {
            set,
            set_if_some,
            ..Default::default()
        },
        (false, false) => ValueControls::default(),
        (false, true) => ValueControls {
            set,
            set_if_some,
            reset,
            ..Default::default()
        },
        (true, false) => ValueControls {
            set,
            replace,
            set_if_some,
            omit,
            reset: None,
        },
        (true, true) => ValueControls {
            set,
            replace,
            set_if_some,
            omit,
            reset,
        },
    }
}

struct Names<'a> {
    owner: &'a str,
    seen: HashMap<String, String>,
}

impl<'a> Names<'a> {
    fn new(owner: &'a str, reserved: &[&str]) -> Self {
        let mut seen = HashMap::new();
        for name in reserved {
            seen.insert(
                (*name).to_owned(),
                format!("the generated core method `{name}`"),
            );
        }
        Self { owner, seen }
    }

    fn insert(&mut self, name: &Ident, what: String) -> syn::Result<()> {
        let text = super::surface::canonical_ident(name);
        if let Some(first) = self.seen.get(&text) {
            return Err(coded(
                name.span(),
                Code::Generated,
                format!(
                    "generated name `{text}` for {what} on {} collides with {first}; rename one declaration (generated names are never suffixed or truncated)",
                    self.owner
                ),
            ));
        }
        self.seen.insert(text, what);
        Ok(())
    }
}

const CLIENT_METHODS: &[&str] = &[
    "builder",
    "new",
    "unbound",
    "contract",
    "credentials",
    "request",
    "plan",
    "bind",
    "plan_runtime",
    "clone",
];
const SCOPE_METHODS: &[&str] = &["bind", "plan_runtime", "clone"];
const UNBOUND_METHODS: &[&str] = &["clone"];
const BUILDER_METHODS: &[&str] = &[
    "new",
    "build",
    "configure_http",
    "configure_runtime",
    "retry",
    "default",
];
const REQUEST_METHODS: &[&str] = &[
    "timeout",
    "reset_timeout",
    "cancel_with",
    "with_options",
    "control_summary",
    "description",
    "plan",
    "response",
    "accepted_bytes",
    "pages",
    "collect_prefix",
    "collect_all",
    "into_future",
    "clone",
];
const INTENT_METHODS: &[&str] = &["description", "clone"];
const MODULE_TYPES: &[&str] = &["Unbound", "__State", "__Local", "Credentials"];

pub(crate) fn resolve_surfaces(resolver: &mut Resolver) -> syn::Result<()> {
    // Value controls.
    for (index, owner) in resolver.model.owners.iter_mut().enumerate() {
        let root = index == 0;
        for value in &mut owner.values {
            value.controls = controls_for(value, root);
        }
    }
    for endpoint in &mut resolver.model.endpoints {
        for value in &mut endpoint.values {
            value.controls = controls_for(value, false);
        }
        if let Some(body) = &mut endpoint.body {
            if body.optional {
                body.controls = ValueControls {
                    set: Some(prefixed_ident("set", &body.name)),
                    replace: Some(prefixed_ident("replace", &body.name)),
                    set_if_some: Some(suffixed_ident(
                        &prefixed_ident("set", &body.name),
                        "if_some",
                    )),
                    omit: Some(prefixed_ident("omit", &body.name)),
                    reset: None,
                };
            }
        }
    }
    let model = &resolver.model;

    // Builder.
    let builder_owner = format!("builder `{}`", model.client.builder);
    let mut builder = Names::new(&builder_owner, BUILDER_METHODS);
    for value in &model.root().values {
        for name in value.controls.names() {
            builder.insert(name, format!("client value `{}`", value.logical))?;
        }
    }
    for secret in &model.client.secrets {
        builder.insert(&secret.name, format!("secret `{}`", secret.logical))?;
    }
    for credential in &model.client.credentials {
        if matches!(credential.source, super::CredentialSource::Custom { .. }) {
            builder.insert(
                &credential.name,
                format!("custom provider `{}`", credential.logical),
            )?;
        }
    }

    // Owner handles and unbound owners.
    for owner in &model.owners {
        let reserved = if owner.kind == OwnerKind::Client {
            CLIENT_METHODS
        } else {
            SCOPE_METHODS
        };
        let handle_owner = format!("handle of {}", owner.display);
        let mut handle = Names::new(&handle_owner, reserved);
        let unbound_owner = format!("unbound handle of {}", owner.display);
        let mut unbound = Names::new(&unbound_owner, UNBOUND_METHODS);
        if owner.kind == OwnerKind::Scope {
            for value in &owner.values {
                for name in value.controls.names() {
                    handle.insert(name, format!("scope value `{}`", value.logical))?;
                    unbound.insert(name, format!("scope value `{}`", value.logical))?;
                }
            }
        }
        let module_owner = format!("module of {}", owner.display);
        let mut types = Names::new(&module_owner, MODULE_TYPES);
        if owner.kind == OwnerKind::Client {
            types.insert(&model.client.builder, "the client builder".to_owned())?;
            // The client module also defines the sealed intent trait; an
            // endpoint `generated` would emit a colliding `GeneratedEndpoint`
            // type (CALL-01: reject before emission).
            types.seen.insert(
                "GeneratedEndpoint".to_owned(),
                "the generated intent trait".to_owned(),
            );
        }
        for child in &owner.children {
            match child {
                Child::Scope(scope) => {
                    let scope = &model.owners[*scope];
                    let method = scope.surface.method.as_ref().expect("scopes navigate");
                    handle.insert(method, format!("scope `{}`", scope.logical))?;
                    unbound.insert(method, format!("scope `{}`", scope.logical))?;
                    let module = scope.surface.module_path.last().expect("scope module");
                    types.insert(module, format!("module of scope `{}`", scope.logical))?;
                }
                Child::Endpoint(endpoint) => {
                    let endpoint = &model.endpoints[*endpoint];
                    handle.insert(
                        &endpoint.surface.method,
                        format!("endpoint `{}`", endpoint.logical),
                    )?;
                    unbound.insert(
                        &endpoint.surface.method,
                        format!("endpoint `{}`", endpoint.logical),
                    )?;
                    for (ty, what) in [
                        (&endpoint.surface.endpoint_type, "unbound intent type"),
                        (&endpoint.surface.request_type, "request type"),
                        (&endpoint.surface.spec_type, "hidden specification type"),
                        (&endpoint.surface.local_type, "hidden value storage type"),
                    ] {
                        types.insert(ty, format!("{what} of endpoint `{}`", endpoint.logical))?;
                    }
                }
            }
        }
    }

    // Requests and intents.
    for endpoint in &model.endpoints {
        let request_owner = format!("request `{}`", endpoint.surface.request_type);
        let mut request = Names::new(&request_owner, REQUEST_METHODS);
        let intent_owner = format!("intent `{}`", endpoint.surface.endpoint_type);
        let mut intent = Names::new(&intent_owner, INTENT_METHODS);
        for value in &endpoint.values {
            for name in value.controls.names() {
                request.insert(name, format!("endpoint value `{}`", value.logical))?;
                intent.insert(name, format!("endpoint value `{}`", value.logical))?;
            }
        }
        if let Some(body) = &endpoint.body {
            for name in body.controls.names() {
                request.insert(name, format!("request body `{}`", body.logical))?;
                intent.insert(name, format!("request body `{}`", body.logical))?;
            }
        }
        for credential in &endpoint.storage {
            let credential = &model.client.credentials[*credential];
            request.insert(
                &credential.store_terminal,
                format!("credential storage terminal for `{}`", credential.logical),
            )?;
        }
    }
    Ok(())
}

// ---- user-type shadowing ----

/// Generated type names visible (by definition or through the generated
/// `use super::*` chain) inside the module of `owner`.
fn generated_types(model: &super::Model, owner: usize) -> Vec<(String, String)> {
    let mut names = Vec::new();
    for ancestor in model.chain(owner) {
        let entry = &model.owners[ancestor];
        names.push((
            "Unbound".to_owned(),
            format!("the unbound handle of {}", entry.display),
        ));
        if entry.kind == OwnerKind::Client {
            names.push((
                "GeneratedEndpoint".to_owned(),
                "the generated intent trait".to_owned(),
            ));
            names.push(("Credentials".to_owned(), "the credential facade".to_owned()));
            names.push((
                model.client.builder.to_string(),
                "the client builder".to_owned(),
            ));
        } else {
            names.push((
                entry.surface.handle.to_string(),
                format!("the handle of scope {}", entry.display),
            ));
        }
        for child in &entry.children {
            match child {
                Child::Endpoint(endpoint) => {
                    let endpoint = &model.endpoints[*endpoint];
                    for ty in [
                        &endpoint.surface.endpoint_type,
                        &endpoint.surface.request_type,
                    ] {
                        names.push((
                            ty.to_string(),
                            format!("a generated type of endpoint {}", endpoint.display),
                        ));
                    }
                }
                Child::Scope(scope) => {
                    let module = model.owners[*scope]
                        .surface
                        .module_path
                        .last()
                        .expect("module");
                    names.push((
                        module.to_string(),
                        format!(
                            "the support module of scope {}",
                            model.owners[*scope].display
                        ),
                    ));
                }
            }
        }
    }
    names
}

/// Rejects a relative path whose leading segment would resolve differently
/// inside the generated support modules: a generated name, or `self`/`super`
/// (which would name the generated module or its parent).
fn check_leading_segment(
    first: &Ident,
    rendered: &str,
    what_kind: &str,
    generated: &[(String, String)],
) -> syn::Result<()> {
    let name = super::surface::canonical_ident(first);
    if name == "self" || name == "super" {
        // The emitter rebases explicit lexical paths to the invocation's
        // module, including nested generic and qualified paths.
        return Ok(());
    }
    if let Some((_, what)) = generated.iter().find(|(generated, _)| *generated == name) {
        return Err(coded(
            first.span(),
            Code::Generated,
            format!(
                "{what_kind} `{rendered}` in this declaration would resolve through {what} inside the generated support modules; name it by an absolute path (for example `crate::models::{rendered}`) or rename it"
            ),
        ));
    }
    Ok(())
}

fn render_path(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|segment| super::surface::canonical_ident(&segment.ident))
        .collect::<Vec<_>>()
        .join("::")
}

/// A stable constant path is evaluated inside the generated support modules.
/// Its value-namespace last segment cannot collide with generated types, but a
/// leading module segment is resolved in the type namespace.
fn check_constant(path: &syn::Path, generated: &[(String, String)]) -> syn::Result<()> {
    if path.leading_colon.is_some() || path.segments.len() < 2 {
        return Ok(());
    }
    check_leading_segment(
        &path.segments[0].ident,
        &render_path(path),
        "constant path",
        generated,
    )
}

fn check_stable(value: &super::StableValue, generated: &[(String, String)]) -> syn::Result<()> {
    match value {
        super::StableValue::Path(path) => check_constant(path, generated),
        _ => Ok(()),
    }
}

fn check_atom(atom: &super::RAtom, generated: &[(String, String)]) -> syn::Result<()> {
    match atom {
        super::RAtom::Literal { .. } | super::RAtom::Value { via: None, .. } => Ok(()),
        super::RAtom::Value { via: Some(via), .. } => check_type(via, generated),
        super::RAtom::Concat(parts) => parts
            .iter()
            .try_for_each(|part| check_atom(part, generated)),
    }
}

fn check_program(ops: &[HeadOp], generated: &[(String, String)]) -> syn::Result<()> {
    for op in ops {
        match &op.emission {
            Some(super::Emission::Atom(atom)) => check_atom(atom, generated)?,
            Some(super::Emission::Each { via: Some(via), .. }) => check_type(via, generated)?,
            Some(super::Emission::Each { via: None, .. }) | None => {}
        }
    }
    Ok(())
}

fn check_route(path: Option<&LocalPath>, generated: &[(String, String)]) -> syn::Result<()> {
    if let Some(LocalPath::Append(template) | LocalPath::Replace(template)) = path {
        for segment in &template.segments {
            for part in &segment.parts {
                if let SegPart::Value { via: Some(via), .. } = part {
                    check_type(via, generated)?;
                }
            }
        }
    }
    Ok(())
}

fn check_rates(rates: &[RateUse], generated: &[(String, String)]) -> syn::Result<()> {
    for rate in rates {
        for dim in &rate.dims {
            if let BoundDim::Custom { via: Some(via), .. } = dim {
                check_type(via, generated)?;
            }
        }
    }
    Ok(())
}

/// Checks a type path (not behind a qualified self) and its generic arguments.
fn check_type_path(path: &syn::Path, generated: &[(String, String)]) -> syn::Result<()> {
    if path.leading_colon.is_none() {
        if let Some(first) = path.segments.first() {
            let name = super::surface::canonical_ident(&first.ident);
            if let Some((_, what)) = generated.iter().find(|(generated, _)| *generated == name) {
                return Err(coded(
                    first.ident.span(),
                    Code::Generated,
                    format!(
                        "type `{name}` in this declaration would resolve to {what} inside the generated support modules; name the type by an absolute path (for example `crate::models::{name}`) or rename it"
                    ),
                ));
            }
            if path.segments.len() > 1 {
                check_leading_segment(&first.ident, &render_path(path), "type path", generated)?;
            }
        }
    }
    check_generic_arguments(path, generated)
}

fn check_generic_arguments(path: &syn::Path, generated: &[(String, String)]) -> syn::Result<()> {
    for segment in &path.segments {
        if let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments {
            for argument in &arguments.args {
                if let syn::GenericArgument::Type(inner) = argument {
                    check_type(inner, generated)?;
                }
            }
        }
    }
    Ok(())
}

fn check_type(ty: &syn::Type, generated: &[(String, String)]) -> syn::Result<()> {
    match ty {
        syn::Type::Path(path) => match &path.qself {
            Some(qself) => {
                check_type(&qself.ty, generated)?;
                check_generic_arguments(&path.path, generated)
            }
            None => check_type_path(&path.path, generated),
        },
        syn::Type::Reference(reference) => check_type(&reference.elem, generated),
        syn::Type::Slice(slice) => check_type(&slice.elem, generated),
        syn::Type::Array(array) => check_type(&array.elem, generated),
        syn::Type::Paren(paren) => check_type(&paren.elem, generated),
        syn::Type::Group(group) => check_type(&group.elem, generated),
        syn::Type::Tuple(tuple) => tuple
            .elems
            .iter()
            .try_for_each(|elem| check_type(elem, generated)),
        _ => Ok(()),
    }
}

/// Names the owning declaration in a shadowing diagnostic (DIAG-01): the
/// shared checks describe the site as "this declaration".
fn in_owner(error: syn::Error, label: &str) -> syn::Error {
    let mut combined: Option<syn::Error> = None;
    for part in error {
        let message = part
            .to_string()
            .replacen("in this declaration", &format!("in {label}"), 1);
        let rebuilt = syn::Error::new(part.span(), message);
        match &mut combined {
            Some(existing) => existing.combine(rebuilt),
            None => combined = Some(rebuilt),
        }
    }
    combined.expect("a syn::Error carries at least one message")
}

/// Rejects bare names that generated support items could shadow. Explicit
/// lexical paths are preserved by the emitter rather than rejected here.
pub(crate) fn check_shadowing(resolver: &Resolver) -> syn::Result<()> {
    let model = &resolver.model;
    for index in 0..model.owners.len() {
        check_owner_shadowing(model, index)
            .map_err(|error| in_owner(error, &model.owner_label(index)))?;
    }
    for index in 0..model.endpoints.len() {
        check_endpoint_shadowing(model, index)
            .map_err(|error| in_owner(error, &model.site_label(Site::Endpoint(index))))?;
    }
    Ok(())
}

fn check_owner_shadowing(model: &super::Model, index: usize) -> syn::Result<()> {
    {
        let owner = &model.owners[index];
        let generated = generated_types(model, index);
        for value in &owner.values {
            check_type(&value.ty, &generated)?;
            if let Some(via) = &value.via {
                check_type(via, &generated)?;
            }
            if let super::Initializer::Stable(stable) = &value.init {
                check_stable(stable, &generated)?;
            }
        }
        check_program(&owner.program.ops, &generated)?;
        check_route(owner.path.as_ref(), &generated)?;
        if let Some(LocalHost::Prepend(labels) | LocalHost::Replace(labels)) = &owner.host {
            for label in labels {
                check_atom(&label.atom, &generated)?;
            }
        }
        check_rates(&owner.effective_rates, &generated)?;
        for choice in [&owner.request_choice, &owner.response_choice]
            .into_iter()
            .flatten()
        {
            if let super::RepresentationChoice::Select(ty) = choice {
                check_type(ty, &generated)?;
            }
        }
        if index == 0 {
            for credential in &model.client.credentials {
                if let super::CredentialSource::Custom { provider } = &credential.source {
                    check_type(provider, &generated)?;
                }
            }
        }
    }
    Ok(())
}

fn check_endpoint_shadowing(model: &super::Model, index: usize) -> syn::Result<()> {
    {
        let endpoint = &model.endpoints[index];
        let generated = generated_types(model, endpoint.owner);
        for value in &endpoint.values {
            check_type(&value.ty, &generated)?;
            if let Some(via) = &value.via {
                check_type(via, &generated)?;
            }
            if let super::Initializer::Stable(stable) = &value.init {
                check_stable(stable, &generated)?;
            }
        }
        if let Some(body) = &endpoint.body {
            check_type(&body.ty, &generated)?;
            check_type(&body.representation, &generated)?;
        }
        check_type(&endpoint.output, &generated)?;
        check_type(&endpoint.response, &generated)?;
        check_program(&endpoint.program.ops, &generated)?;
        check_route(endpoint.path.as_ref(), &generated)?;
        check_rates(&endpoint.effective_rates, &generated)?;
        if let Some(pagination) = &endpoint.pagination {
            check_type(&pagination.controller, &generated)?;
            check_type_path(&pagination.init_type, &generated)?;
            for (_, value) in &pagination.fields {
                if let InitValue::Stable(stable) = value {
                    check_stable(stable, &generated)?;
                }
            }
        }
    }
    Ok(())
}
