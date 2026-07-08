use crate::emit_helpers;
use crate::sema::*;
use proc_macro2::Span;
use std::collections::{BTreeMap, BTreeSet};
use syn::{Ident, Type};

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct FacadeIr {
    pub client_name: Ident,
    pub client_setters: Vec<FacadeSetter>,
    pub auth_setters: Vec<FacadeSetter>,
    pub credential_methods: Vec<FacadeCredentialMethods>,
    pub scopes: Vec<FacadeScope>,
    pub endpoints: Vec<FacadeEndpoint>,
    pub docs: Vec<FacadeDoc>,
    scope_index: BTreeMap<Vec<String>, usize>,
    endpoint_index: BTreeMap<EndpointTargetKey, usize>,
    credential_methods_index: BTreeMap<String, usize>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct FacadeScope {
    pub path: Vec<Ident>,
    pub public_name: Ident,
    pub public_method: Ident,
    pub rust_type_name: Ident,
    pub parent_path: Vec<Ident>,
    pub decls: Vec<VarInfo>,
    pub setters: Vec<FacadeSetter>,
    pub methods: Vec<FacadeMethod>,
    pub constructor_docs: Vec<FacadeDoc>,
    pub docs: Vec<FacadeDoc>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct FacadeEndpoint {
    pub target: FacadeEndpointTarget,
    pub public_method: Ident,
    pub scope_path: Vec<Ident>,
    pub required_args: Vec<FacadeArg>,
    pub constructor: FacadeEndpointConstructorPlan,
    pub captured_setters: Vec<FacadeCapturedSetter>,
    pub setters: Vec<FacadeSetter>,
    pub docs: Vec<FacadeDoc>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct FacadeMethod {
    pub public_name: Ident,
    pub target_scope_path: Vec<Ident>,
    pub target_scope_type_name: Ident,
    pub docs: Vec<FacadeDoc>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct FacadeCredentialMethods {
    pub credential: Ident,
    pub acquire_name: Ident,
    pub set_name: Ident,
    pub clear_name: Ident,
    pub has_name: Ident,
    pub pending_method: Ident,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct FacadeArg {
    pub name: Ident,
    pub ty: Type,
    pub kind: FacadeArgKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum FacadeArgKind {
    Value,
    Body,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct FacadeEndpointTarget {
    pub scope_path: Vec<Ident>,
    pub endpoint: Ident,
}

impl FacadeEndpointTarget {
    pub(crate) fn key(&self) -> EndpointTargetKey {
        EndpointTargetKey {
            scope_modules: self.scope_path.iter().map(ToString::to_string).collect(),
            endpoint: self.endpoint.to_string(),
        }
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct FacadeEndpointConstructorPlan {
    pub args: Vec<FacadeConstructorArg>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum FacadeConstructorArg {
    PublicArg { name: Ident },
    CapturedScopeField { name: Ident },
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct FacadeCapturedSetter {
    pub field: Ident,
    pub optional: bool,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct FacadeSetter {
    pub field: Ident,
    pub ty: Type,
    pub set_name: Ident,
    pub set_optional_name: Ident,
    pub clear_name: Ident,
    pub forms: Vec<SetterForm>,
    pub set_doc: String,
    pub set_optional_doc: String,
    pub clear_doc: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum SetterForm {
    Set,
    SetOptional,
    Clear,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct FacadeDoc {
    pub summary: String,
    pub details: Vec<String>,
}

impl FacadeIr {
    pub(crate) fn scope_for_path(&self, path: &[Ident]) -> Option<&FacadeScope> {
        self.scope_index
            .get(&scope_path_key(path))
            .map(|index| &self.scopes[*index])
    }

    pub(crate) fn endpoint_for_resolved(&self, ep: &ResolvedEndpoint) -> Option<&FacadeEndpoint> {
        self.endpoint_index
            .get(&resolved_endpoint_key(ep))
            .map(|index| &self.endpoints[*index])
    }

    pub(crate) fn credential_methods_for(&self, name: &Ident) -> Option<&FacadeCredentialMethods> {
        self.credential_methods_index
            .get(&name.to_string())
            .map(|index| &self.credential_methods[*index])
    }
}

pub(crate) fn build_facade_ir(resolved_api: &ResolvedApi) -> FacadeIr {
    let scope_infos = collect_facade_scopes(resolved_api);
    let scope_children = index_facade_scope_children(&scope_infos);
    let scopes = scope_infos
        .iter()
        .enumerate()
        .map(|(index, scope)| {
            facade_scope_from_info(resolved_api, &scope_infos, &scope_children, index, scope)
        })
        .collect();
    let endpoints = resolved_api
        .endpoints
        .iter()
        .map(facade_endpoint_from_resolved)
        .collect();
    FacadeIr {
        client_name: resolved_api.client_name.clone(),
        client_setters: facade_client_setters(&resolved_api.client_vars),
        auth_setters: facade_client_setters(&resolved_api.client_auth_vars),
        credential_methods: facade_credential_methods(&resolved_api.client_auth_credentials),
        scopes,
        endpoints,
        docs: Vec::new(),
        scope_index: index_facade_scopes(&scope_infos),
        endpoint_index: index_facade_endpoints(resolved_api),
        credential_methods_index: index_facade_credential_methods(resolved_api),
    }
}

fn facade_credential_methods(credentials: &[AuthCredentialIr]) -> Vec<FacadeCredentialMethods> {
    credentials
        .iter()
        .filter_map(|credential| {
            if !matches!(credential.kind, AuthCredentialKindIr::Endpoint { .. }) {
                return None;
            }
            let name = credential.name.clone();
            Some(FacadeCredentialMethods {
                credential: name.clone(),
                acquire_name: emit_helpers::ident(&format!("acquire_auth_{name}"), name.span()),
                set_name: emit_helpers::ident(&format!("set_auth_{name}_value"), name.span()),
                clear_name: emit_helpers::ident(&format!("clear_auth_{name}"), name.span()),
                has_name: emit_helpers::ident(&format!("has_auth_{name}"), name.span()),
                pending_method: emit_helpers::ident(&format!("acquire_as_{name}"), name.span()),
            })
        })
        .collect()
}

fn facade_client_setters(vars: &[VarInfo]) -> Vec<FacadeSetter> {
    vars.iter()
        .map(|var| {
            let ty = &var.ty;
            let field = var.rust.clone();
            let mut forms = vec![SetterForm::Set];
            if var.optional {
                forms.push(SetterForm::Clear);
            }
            FacadeSetter {
                field: field.clone(),
                ty: ty.clone(),
                set_name: emit_helpers::ident(&format!("set_{field}"), field.span()),
                set_optional_name: emit_helpers::ident(&format!("set_{field}_opt"), field.span()),
                clear_name: emit_helpers::ident(&format!("clear_{field}"), field.span()),
                forms,
                set_doc: format!("Set client parameter `{field}`."),
                set_optional_doc: format!(
                    "Set or clear client parameter `{field}` from an Option; None clears it."
                ),
                clear_doc: format!("Clear client parameter `{field}`."),
            }
        })
        .collect()
}

#[derive(Clone)]
struct FacadeScopeInfo {
    path: Vec<Ident>,
    decls: Vec<VarInfo>,
}

fn collect_facade_scopes(resolved_api: &ResolvedApi) -> Vec<FacadeScopeInfo> {
    let mut scopes: Vec<FacadeScopeInfo> = Vec::new();
    let mut seen = BTreeSet::new();
    for ep in &resolved_api.endpoints {
        for idx in 0..ep.scope_modules.len() {
            let path = ep.scope_modules[..=idx].to_vec();
            if !seen.insert(scope_path_key(&path)) {
                continue;
            }
            let decls = ep
                .facade_param_groups
                .iter()
                .take(idx + 1)
                .flat_map(|group| group.iter().cloned())
                .collect();
            scopes.push(FacadeScopeInfo { path, decls });
        }
    }
    scopes
}

fn index_facade_scopes(scope_infos: &[FacadeScopeInfo]) -> BTreeMap<Vec<String>, usize> {
    scope_infos
        .iter()
        .enumerate()
        .map(|(idx, scope)| (scope_path_key(&scope.path), idx))
        .collect()
}

fn index_facade_scope_children(
    scope_infos: &[FacadeScopeInfo],
) -> BTreeMap<Vec<String>, Vec<usize>> {
    let mut children: BTreeMap<Vec<String>, Vec<usize>> = BTreeMap::new();
    for (idx, scope) in scope_infos.iter().enumerate() {
        if scope.path.len() <= 1 {
            continue;
        }
        let parent = scope.path[..scope.path.len() - 1].to_vec();
        children
            .entry(scope_path_key(&parent))
            .or_default()
            .push(idx);
    }
    children
}

fn index_facade_endpoints(resolved_api: &ResolvedApi) -> BTreeMap<EndpointTargetKey, usize> {
    resolved_api
        .endpoints
        .iter()
        .enumerate()
        .map(|(idx, ep)| (resolved_endpoint_key(ep), idx))
        .collect()
}

fn index_facade_credential_methods(resolved_api: &ResolvedApi) -> BTreeMap<String, usize> {
    resolved_api
        .client_auth_credentials
        .iter()
        .filter(|credential| matches!(credential.kind, AuthCredentialKindIr::Endpoint { .. }))
        .enumerate()
        .map(|(idx, credential)| (credential.name.to_string(), idx))
        .collect()
}

fn facade_scope_from_info(
    resolved_api: &ResolvedApi,
    scope_infos: &[FacadeScopeInfo],
    scope_children: &BTreeMap<Vec<String>, Vec<usize>>,
    _scope_index: usize,
    scope: &FacadeScopeInfo,
) -> FacadeScope {
    let path = scope.path.clone();
    let public_name = path
        .last()
        .cloned()
        .expect("facade scope path must be non-empty");
    let methods = scope_children
        .get(&scope_path_key(&scope.path))
        .into_iter()
        .flat_map(|indices| indices.iter().copied())
        .map(|child_index| {
            let child = &scope_infos[child_index];
            let target_scope_path = child.path.clone();
            let public_name = target_scope_path
                .last()
                .cloned()
                .expect("facade child scope path must be non-empty");
            FacadeMethod {
                public_name,
                target_scope_path: target_scope_path.clone(),
                target_scope_type_name: emit_helpers::ident(
                    &generated_scope_type_name(&resolved_api.client_name, &child.path),
                    resolved_api.client_name.span(),
                ),
                docs: vec![FacadeDoc {
                    summary: format!(
                        "Enter the `{}` facade scope.",
                        ident_path_strings(&target_scope_path).join("::")
                    ),
                    details: Vec::new(),
                }],
            }
        })
        .collect();
    let setters = scope
        .decls
        .iter()
        .filter(|var| var.optional || var.default.is_some())
        .map(|var| {
            let ty = &var.ty;
            let docs = facade_scope_setter_docs(var);
            let mut forms = vec![SetterForm::Set];
            if var.optional {
                forms.push(SetterForm::Clear);
            }
            FacadeSetter {
                field: var.rust.clone(),
                ty: ty.clone(),
                set_name: var.rust.clone(),
                set_optional_name: emit_helpers::ident(
                    &format!("{}_opt", var.rust),
                    var.rust.span(),
                ),
                clear_name: emit_helpers::ident(&format!("clear_{}", var.rust), var.rust.span()),
                forms,
                set_doc: docs.0,
                set_optional_doc: docs.1,
                clear_doc: docs.2,
            }
        })
        .collect();
    FacadeScope {
        path: path.clone(),
        public_name,
        public_method: path
            .last()
            .cloned()
            .expect("facade scope path must be non-empty"),
        rust_type_name: emit_helpers::ident(
            &generated_scope_type_name(&resolved_api.client_name, &scope.path),
            resolved_api.client_name.span(),
        ),
        parent_path: scope
            .path
            .iter()
            .take(scope.path.len().saturating_sub(1))
            .cloned()
            .collect(),
        decls: scope.decls.clone(),
        setters,
        methods,
        constructor_docs: vec![FacadeDoc {
            summary: format!(
                "Enter the `{}` facade scope.",
                ident_path_strings(&path).join("::")
            ),
            details: Vec::new(),
        }],
        docs: vec![FacadeDoc {
            summary: format!(
                "Facade handle for the `{}` scope.",
                ident_path_strings(&path).join("::")
            ),
            details: Vec::new(),
        }],
    }
}

fn scope_path_key(path: &[Ident]) -> Vec<String> {
    path.iter().map(ToString::to_string).collect()
}

fn resolved_endpoint_key(ep: &ResolvedEndpoint) -> EndpointTargetKey {
    EndpointTargetKey {
        scope_modules: ep.scope_modules.iter().map(ToString::to_string).collect(),
        endpoint: ep.name.to_string(),
    }
}

fn facade_scope_setter_docs(var: &VarInfo) -> (String, String, String) {
    let field = var.rust.to_string();
    if var.optional {
        (
            format!("Set optional scope parameter `{field}`."),
            format!(
                "Set or clear optional scope parameter `{field}` from an Option; None clears it."
            ),
            format!("Clear optional scope parameter `{field}`."),
        )
    } else {
        (
            format!("Set defaulted scope parameter `{field}`."),
            format!(
                "Set defaulted scope parameter `{field}` from an Option; None resets to the declared default."
            ),
            format!("Reset defaulted scope parameter `{field}` to its declared default."),
        )
    }
}

fn facade_endpoint_from_resolved(ep: &ResolvedEndpoint) -> FacadeEndpoint {
    let captured_names = ep
        .facade_param_groups
        .iter()
        .flatten()
        .map(|var| var.rust.to_string())
        .collect::<std::collections::BTreeSet<_>>();
    let mut required_args: Vec<FacadeArg> = Vec::new();
    let mut constructor_args: Vec<FacadeConstructorArg> = Vec::new();
    for var in &ep.vars {
        let is_captured = captured_names.contains(&var.rust.to_string());
        if (var.optional || var.default.is_some()) && is_captured {
            continue;
        }
        if !is_captured && !var.optional && var.default.is_none() {
            required_args.push(FacadeArg {
                name: var.rust.clone(),
                ty: var.ty.clone(),
                kind: FacadeArgKind::Value,
            });
        }
        if !var.optional && var.default.is_none() {
            constructor_args.push(if is_captured {
                FacadeConstructorArg::CapturedScopeField {
                    name: var.rust.clone(),
                }
            } else {
                FacadeConstructorArg::PublicArg {
                    name: var.rust.clone(),
                }
            });
        }
    }
    if let Some(body_ty) = facade_request_body_ty(ep) {
        required_args.push(FacadeArg {
            name: emit_helpers::ident("body", Span::call_site()),
            ty: body_ty.clone(),
            kind: FacadeArgKind::Body,
        });
        constructor_args.push(FacadeConstructorArg::PublicArg {
            name: emit_helpers::ident("body", Span::call_site()),
        });
    }
    let captured_setters = ep
        .facade_param_groups
        .iter()
        .flatten()
        .filter(|var| var.optional || var.default.is_some())
        .map(|var| FacadeCapturedSetter {
            field: var.rust.clone(),
            optional: var.optional,
        })
        .collect();
    let setters = ep
        .vars
        .iter()
        .filter(|var| !captured_names.contains(&var.rust.to_string()))
        .filter(|var| var.optional || var.default.is_some())
        .map(|var| {
            let ty = &var.ty;
            let mut forms = vec![SetterForm::Set];
            let docs = facade_setter_docs(ep, var);
            if var.optional {
                forms.push(SetterForm::SetOptional);
                forms.push(SetterForm::Clear);
            }
            FacadeSetter {
                field: var.rust.clone(),
                ty: ty.clone(),
                set_name: var.rust.clone(),
                set_optional_name: emit_helpers::ident(
                    &format!("{}_opt", var.rust),
                    var.rust.span(),
                ),
                clear_name: emit_helpers::ident(&format!("clear_{}", var.rust), var.rust.span()),
                forms,
                set_doc: docs.0,
                set_optional_doc: docs.1,
                clear_doc: docs.2,
            }
        })
        .collect();
    let public_method = ep.alias.as_ref().unwrap_or(&ep.name).clone();
    FacadeEndpoint {
        target: FacadeEndpointTarget {
            scope_path: ep.scope_modules.clone(),
            endpoint: ep.name.clone(),
        },
        public_method: emit_helpers::ident(
            &generated_endpoint_method_name(&public_method.to_string()),
            public_method.span(),
        ),
        scope_path: ep.scope_modules.clone(),
        required_args,
        constructor: FacadeEndpointConstructorPlan {
            args: constructor_args,
        },
        captured_setters,
        setters,
        docs: facade_endpoint_doc_texts(ep),
    }
}

fn facade_endpoint_doc_texts(ep: &ResolvedEndpoint) -> Vec<FacadeDoc> {
    let mut docs = vec![FacadeDoc {
        summary: format!("{} {}", ep.method, doc_path(ep)),
        details: Vec::new(),
    }];
    let required = ep
        .vars
        .iter()
        .filter(|var| !var.optional && var.default.is_none())
        .map(|var| format!("`{}`", var.rust))
        .collect::<Vec<_>>();
    if !required.is_empty() {
        docs.push(FacadeDoc {
            summary: format!("Required params: {}", required.join(", ")),
            details: Vec::new(),
        });
    }
    if let Some(pagination) = &ep.paginate {
        let controller_ty = &pagination.controller_ty;
        docs.push(FacadeDoc {
            summary: format!("Pagination: {}", quote::quote!(#controller_ty)),
            details: Vec::new(),
        });
    }
    if let Some(body_summary) = &ep.io.request_entity.doc.facade_summary {
        docs.push(FacadeDoc {
            summary: body_summary.clone(),
            details: Vec::new(),
        });
    }
    if let Some(response_summary) = &ep.io.response_entity.doc.facade_summary {
        docs.push(FacadeDoc {
            summary: response_summary.clone(),
            details: Vec::new(),
        });
    }
    docs
}

fn facade_setter_docs(ep: &ResolvedEndpoint, var: &VarInfo) -> (String, String, String) {
    let field = var.rust.to_string();
    let role = endpoint_var_role(ep, &var.rust);
    if var.optional {
        (
            format!("Set optional {role} parameter `{field}`."),
            format!(
                "Set or clear optional {role} parameter `{field}` from an Option; None clears it."
            ),
            format!("Clear optional {role} parameter `{field}`."),
        )
    } else {
        (
            format!("Set defaulted {role} parameter `{field}`."),
            format!(
                "Set defaulted {role} parameter `{field}` from an Option; None resets to the declared default."
            ),
            format!("Reset defaulted {role} parameter `{field}` to its declared default."),
        )
    }
}

fn facade_request_body_ty(ep: &ResolvedEndpoint) -> Option<Type> {
    ep.io.request_entity.public_input_ty.clone()
}

fn endpoint_var_role(ep: &ResolvedEndpoint, field: &Ident) -> &'static str {
    if route_pieces_use_ep_field(&ep.scope_path_pieces, field)
        || route_pieces_use_ep_field(&ep.route_pieces, field)
    {
        return "path";
    }
    if policy_ops_use_ep_field(&ep.policy.endpoint.query, field)
        || ep
            .policy
            .scopes
            .iter()
            .any(|policy| policy_ops_use_ep_field(&policy.query, field))
    {
        return "query";
    }
    if policy_ops_use_ep_field(&ep.policy.endpoint.headers, field)
        || ep
            .policy
            .scopes
            .iter()
            .any(|policy| policy_ops_use_ep_field(&policy.headers, field))
    {
        return "header";
    }
    "request"
}

fn route_pieces_use_ep_field(pieces: &[PathPiece], field: &Ident) -> bool {
    pieces.iter().any(|piece| match piece {
        PathPiece::EpVar { field: candidate } => candidate == field,
        PathPiece::Fmt(fmt) => fmt.pieces.iter().any(|piece| {
            matches!(
                piece,
                FmtResolvedPiece::Var {
                    source: FmtVarSource::Ep,
                    field: candidate,
                    ..
                } if candidate == field
            )
        }),
        _ => false,
    })
}

fn policy_ops_use_ep_field(ops: &[PolicyOp], field: &Ident) -> bool {
    ops.iter().any(|op| match op {
        PolicyOp::Set { value, .. } => policy_set_value_uses_ep_field(value, field),
        PolicyOp::Remove { .. } => false,
    })
}

fn value_kind_uses_ep_field(value: &PublicValueKind, field: &Ident) -> bool {
    match value {
        PublicValueKind::EpField(candidate) => candidate == field,
        PublicValueKind::Fmt(fmt) => fmt.pieces.iter().any(|piece| {
            matches!(
                piece,
                FmtResolvedPiece::Var {
                    source: FmtVarSource::Ep,
                    field: candidate,
                    ..
                } if candidate == field
            )
        }),
        _ => false,
    }
}

fn policy_set_value_uses_ep_field(value: &PolicySetValue, field: &Ident) -> bool {
    match value {
        PolicySetValue::OptionalEpField(candidate) => candidate == field,
        PolicySetValue::Value(value) => value_kind_uses_ep_field(value, field),
        PolicySetValue::OptionalCxField(_) => false,
    }
}

pub(crate) fn ident_path_strings(path: &[Ident]) -> Vec<String> {
    path.iter().map(ToString::to_string).collect()
}

pub(crate) fn generated_scope_type_name(client_name: &Ident, path: &[Ident]) -> String {
    format!("{}{}Scope", client_name, pascalize_ident_path(path))
}

pub(crate) fn client_prefixed_type_name(client: &Ident, suffix: &str) -> String {
    format!("{client}{suffix}")
}

pub(crate) fn generated_auth_facade_type_name(client: &Ident) -> String {
    client_prefixed_type_name(client, "Auth")
}

pub(crate) fn generated_auth_handle_type_name(client: &Ident, credential: &Ident) -> String {
    format!(
        "{}{}Auth",
        client,
        generated_pascal_name(&credential.to_string())
    )
}

pub(crate) fn generated_acquire_as_trait_type_name(client: &Ident, credential: &Ident) -> String {
    format!(
        "{}AcquireAs{}Ext",
        client,
        generated_pascal_name(&credential.to_string())
    )
}

pub(crate) fn generated_endpoint_request_ext_trait_type_name(ep: &ResolvedEndpoint) -> String {
    let mut name = String::new();
    for scope in &ep.scope_modules {
        name.push_str(&generated_pascal_name(&scope.to_string()));
    }
    name.push_str(&generated_pascal_name(&ep.name.to_string()));
    name.push_str("RequestExt");
    name
}

pub(crate) fn generated_endpoint_method_name(raw: &str) -> String {
    pascal_to_snake(raw)
}

pub(crate) fn generated_pascal_name(raw: &str) -> String {
    raw.split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            let Some(first) = chars.next() else {
                return String::new();
            };
            let mut out = String::new();
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
            out
        })
        .collect::<String>()
}

fn pascalize_ident_path(path: &[Ident]) -> String {
    path.iter()
        .map(ToString::to_string)
        .map(|s| generated_pascal_name(&s))
        .collect::<String>()
}

fn pascal_to_snake(raw: &str) -> String {
    let mut out = String::new();
    let mut prev_lower_or_digit = false;
    for ch in raw.chars() {
        if ch.is_ascii_uppercase() {
            if prev_lower_or_digit {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
            prev_lower_or_digit = false;
        } else {
            out.push(ch);
            prev_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        }
    }
    out
}

fn doc_path(ep: &ResolvedEndpoint) -> String {
    let mut pieces = Vec::new();
    for piece in ep.scope_path_pieces.iter().chain(ep.route_pieces.iter()) {
        match piece {
            PathPiece::Static(value) => pieces.push(value.clone()),
            PathPiece::CxVar { field, .. } | PathPiece::EpVar { field } => {
                pieces.push(format!("{{{field}}}"));
            }
            PathPiece::Fmt(_) => pieces.push("{part}".to_string()),
        }
    }
    if pieces.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", pieces.join("/"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn setter_forms_match_current_public_surface() {
        let setter = FacadeSetter {
            field: parse_quote!(limit),
            ty: parse_quote!(u64),
            set_name: parse_quote!(limit),
            set_optional_name: parse_quote!(limit_opt),
            clear_name: parse_quote!(clear_limit),
            forms: vec![SetterForm::Set, SetterForm::SetOptional, SetterForm::Clear],
            set_doc: "Set optional request parameter `limit`.".to_string(),
            set_optional_doc:
                "Set or clear optional request parameter `limit` from an Option; None clears it."
                    .to_string(),
            clear_doc: "Clear optional request parameter `limit`.".to_string(),
        };

        assert_eq!(setter.forms.len(), 3);
        assert!(setter.forms.contains(&SetterForm::Clear));
    }
}
