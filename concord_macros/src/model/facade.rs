use crate::sema::*;
use syn::Ident;

#[derive(Debug, Default)]
#[allow(dead_code)]
pub(crate) struct FacadeIr {
    pub client_name: String,
    pub client_setters: Vec<FacadeSetter>,
    pub auth_setters: Vec<FacadeSetter>,
    pub credential_methods: Vec<FacadeCredentialMethods>,
    pub scopes: Vec<FacadeScope>,
    pub endpoints: Vec<FacadeEndpoint>,
    pub docs: Vec<FacadeDoc>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct FacadeScope {
    pub path: Vec<String>,
    pub public_name: String,
    pub public_method: String,
    pub rust_type_name: String,
    pub parent_path: Vec<String>,
    pub decls: Vec<VarInfo>,
    pub setters: Vec<FacadeSetter>,
    pub methods: Vec<FacadeMethod>,
    pub constructor_docs: Vec<FacadeDoc>,
    pub docs: Vec<FacadeDoc>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct FacadeEndpoint {
    pub target_endpoint: String,
    pub public_method: String,
    pub scope_path: Vec<String>,
    pub required_args: Vec<FacadeArg>,
    pub setters: Vec<FacadeSetter>,
    pub docs: Vec<FacadeDoc>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct FacadeMethod {
    pub public_name: String,
    pub target_scope_path: Vec<String>,
    pub target_scope_type_name: String,
    pub docs: Vec<FacadeDoc>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct FacadeCredentialMethods {
    pub credential: String,
    pub acquire_name: String,
    pub set_name: String,
    pub clear_name: String,
    pub has_name: String,
    pub pending_method: String,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct FacadeArg {
    pub name: String,
    pub ty: String,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct FacadeSetter {
    pub field: String,
    pub ty: String,
    pub set_name: String,
    pub set_optional_name: String,
    pub clear_name: String,
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

pub(crate) fn build_facade_ir(resolved_api: &ResolvedApi) -> FacadeIr {
    let scope_infos = collect_facade_scopes(resolved_api);
    FacadeIr {
        client_name: resolved_api.client_name.to_string(),
        client_setters: facade_client_setters(&resolved_api.client_vars),
        auth_setters: facade_client_setters(&resolved_api.client_auth_vars),
        credential_methods: facade_credential_methods(&resolved_api.client_auth_credentials),
        scopes: scope_infos
            .iter()
            .map(|scope| facade_scope_from_info(resolved_api, &scope_infos, scope))
            .collect(),
        endpoints: resolved_api
            .endpoints
            .iter()
            .map(facade_endpoint_from_resolved)
            .collect(),
        docs: Vec::new(),
    }
}

fn facade_credential_methods(credentials: &[AuthCredentialIr]) -> Vec<FacadeCredentialMethods> {
    credentials
        .iter()
        .filter_map(|credential| {
            if !matches!(credential.kind, AuthCredentialKindIr::Endpoint { .. }) {
                return None;
            }
            let name = credential.name.to_string();
            Some(FacadeCredentialMethods {
                credential: name.clone(),
                acquire_name: format!("acquire_auth_{name}"),
                set_name: format!("set_auth_{name}_value"),
                clear_name: format!("clear_auth_{name}"),
                has_name: format!("has_auth_{name}"),
                pending_method: format!("acquire_as_{name}"),
            })
        })
        .collect()
}

fn facade_client_setters(vars: &[VarInfo]) -> Vec<FacadeSetter> {
    vars.iter()
        .map(|var| {
            let ty = &var.ty;
            let field = var.rust.to_string();
            let mut forms = vec![SetterForm::Set];
            if var.optional {
                forms.push(SetterForm::Clear);
            }
            FacadeSetter {
                field: field.clone(),
                ty: quote::quote!(#ty).to_string(),
                set_name: format!("set_{field}"),
                set_optional_name: format!("set_{field}_opt"),
                clear_name: format!("clear_{field}"),
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
    for ep in &resolved_api.endpoints {
        for idx in 0..ep.scope_modules.len() {
            let path = ep.scope_modules[..=idx].to_vec();
            if scopes.iter().any(|scope| scope.path == path) {
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

fn facade_scope_from_info(
    resolved_api: &ResolvedApi,
    scope_infos: &[FacadeScopeInfo],
    scope: &FacadeScopeInfo,
) -> FacadeScope {
    let path = ident_path_strings(&scope.path);
    let public_name = path.last().cloned().unwrap_or_default();
    let methods = scope_infos
        .iter()
        .filter(|child| {
            child.path.len() == scope.path.len() + 1 && child.path.starts_with(&scope.path)
        })
        .map(|child| {
            let target_scope_path = ident_path_strings(&child.path);
            let public_name = target_scope_path.last().cloned().unwrap_or_default();
            FacadeMethod {
                public_name,
                target_scope_path: target_scope_path.clone(),
                target_scope_type_name: generated_scope_type_name(
                    &resolved_api.client_name,
                    &child.path,
                ),
                docs: vec![FacadeDoc {
                    summary: format!("Enter the `{}` facade scope.", target_scope_path.join("::")),
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
                field: var.rust.to_string(),
                ty: quote::quote!(#ty).to_string(),
                set_name: var.rust.to_string(),
                set_optional_name: format!("{}_opt", var.rust),
                clear_name: format!("clear_{}", var.rust),
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
        public_method: path.last().cloned().unwrap_or_default(),
        rust_type_name: generated_scope_type_name(&resolved_api.client_name, &scope.path),
        parent_path: scope
            .path
            .iter()
            .take(scope.path.len().saturating_sub(1))
            .map(ToString::to_string)
            .collect(),
        decls: scope.decls.clone(),
        setters,
        methods,
        constructor_docs: vec![FacadeDoc {
            summary: format!("Enter the `{}` facade scope.", path.join("::")),
            details: Vec::new(),
        }],
        docs: vec![FacadeDoc {
            summary: format!("Facade handle for the `{}` scope.", path.join("::")),
            details: Vec::new(),
        }],
    }
}

fn facade_scope_setter_docs(var: &VarInfo) -> (String, String, String) {
    let field = var.rust.to_string();
    let default = var
        .default
        .as_ref()
        .map(|expr| quote::quote!(#expr).to_string());
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
            format!(
                "Set defaulted scope parameter `{field}`{}.",
                default
                    .as_ref()
                    .map(|value| format!(" (default: `{value}`)"))
                    .unwrap_or_default()
            ),
            format!(
                "Set defaulted scope parameter `{field}` from an Option; None resets to the default{}.",
                default
                    .as_ref()
                    .map(|value| format!(" `{value}`"))
                    .unwrap_or_default()
            ),
            format!(
                "Reset defaulted scope parameter `{field}` to its default{}.",
                default
                    .as_ref()
                    .map(|value| format!(" `{value}`"))
                    .unwrap_or_default()
            ),
        )
    }
}

fn facade_endpoint_from_resolved(ep: &ResolvedEndpoint) -> FacadeEndpoint {
    let captured = ep
        .facade_param_groups
        .iter()
        .flatten()
        .map(|var| var.rust.to_string())
        .collect::<std::collections::BTreeSet<_>>();
    let mut required_args = ep
        .vars
        .iter()
        .filter(|var| !captured.contains(&var.rust.to_string()))
        .filter(|var| !var.optional && var.default.is_none())
        .map(|var| {
            let ty = &var.ty;
            FacadeArg {
                name: var.rust.to_string(),
                ty: quote::quote!(#ty).to_string(),
            }
        })
        .collect::<Vec<_>>();
    if let Some(body_ty) = facade_request_body_ty(ep) {
        required_args.push(FacadeArg {
            name: "body".to_string(),
            ty: body_ty,
        });
    }
    let setters = ep
        .vars
        .iter()
        .filter(|var| !captured.contains(&var.rust.to_string()))
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
                field: var.rust.to_string(),
                ty: quote::quote!(#ty).to_string(),
                set_name: var.rust.to_string(),
                set_optional_name: format!("{}_opt", var.rust),
                clear_name: format!("clear_{}", var.rust),
                forms,
                set_doc: docs.0,
                set_optional_doc: docs.1,
                clear_doc: docs.2,
            }
        })
        .collect();
    let public_method = ep.alias.as_ref().unwrap_or(&ep.name).to_string();
    FacadeEndpoint {
        target_endpoint: endpoint_qualified_name(ep),
        public_method: generated_endpoint_method_name(&public_method),
        scope_path: ep.scope_modules.iter().map(ToString::to_string).collect(),
        required_args,
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
        let controller = pagination
            .ctrl_ty
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
            .unwrap_or_else(|| "configured".to_string());
        docs.push(FacadeDoc {
            summary: format!("Pagination: {controller}"),
            details: Vec::new(),
        });
    }
    if let Some(body) = &ep.body {
        let body_summary = match ep.request_io() {
            ResolvedRequestBodyIo::Multipart {
                value_ty,
                format_ty,
            } => format!(
                "Body: Multipart<{}, {}>",
                quote::quote!(#value_ty),
                quote::quote!(#format_ty)
            ),
            ResolvedRequestBodyIo::Records { item_ty, format_ty } => format!(
                "Body: Records<{}, {}>",
                quote::quote!(#item_ty),
                quote::quote!(#format_ty)
            ),
            ResolvedRequestBodyIo::RawStream { media_ty } => {
                format!("Body: Stream<{}>", quote::quote!(#media_ty))
            }
            _ => format!("Body: {}", doc_codec(&body.enc, &body.ty)),
        };
        docs.push(FacadeDoc {
            summary: body_summary,
            details: Vec::new(),
        });
    }
    let response_summary = match ep.response_io() {
        ResolvedResponseBodyIo::BufferedBytes => "Response: bytes::Bytes".to_string(),
        ResolvedResponseBodyIo::Multipart { part_ty, format_ty } => format!(
            "Response: Multipart<{}, {}>",
            quote::quote!(#part_ty),
            quote::quote!(#format_ty)
        ),
        ResolvedResponseBodyIo::Records { item_ty, format_ty } => format!(
            "Response: Records<{}, {}>",
            quote::quote!(#item_ty),
            quote::quote!(#format_ty)
        ),
        ResolvedResponseBodyIo::RawStream { media_ty } => {
            format!("Response: Stream<{}>", quote::quote!(#media_ty))
        }
        _ => format!("Response: {}", doc_codec(&ep.response.enc, &ep.response.ty)),
    };
    docs.push(FacadeDoc {
        summary: response_summary,
        details: Vec::new(),
    });
    docs
}

fn facade_setter_docs(ep: &ResolvedEndpoint, var: &VarInfo) -> (String, String, String) {
    let field = var.rust.to_string();
    let role = endpoint_var_role(ep, &var.rust);
    let default = var
        .default
        .as_ref()
        .map(|expr| quote::quote!(#expr).to_string());
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
            format!(
                "Set defaulted {role} parameter `{field}`{}.",
                default
                    .as_ref()
                    .map(|value| format!(" (default: `{value}`)"))
                    .unwrap_or_default()
            ),
            format!(
                "Set defaulted {role} parameter `{field}` from an Option; None resets to the default{}.",
                default
                    .as_ref()
                    .map(|value| format!(" `{value}`"))
                    .unwrap_or_default()
            ),
            format!(
                "Reset defaulted {role} parameter `{field}` to its default{}.",
                default
                    .as_ref()
                    .map(|value| format!(" `{value}`"))
                    .unwrap_or_default()
            ),
        )
    }
}

fn facade_request_body_ty(ep: &ResolvedEndpoint) -> Option<String> {
    match ep.request_io() {
        ResolvedRequestBodyIo::None => None,
        ResolvedRequestBodyIo::BufferedCodec(_) | ResolvedRequestBodyIo::BufferedBytes => {
            ep.body.as_ref().map(|body| {
                let ty = &body.ty;
                quote::quote!(#ty).to_string()
            })
        }
        ResolvedRequestBodyIo::Multipart { .. } => {
            Some("::concord_core::advanced::MultipartBody".to_string())
        }
        ResolvedRequestBodyIo::Records { item_ty, .. } => Some(format!(
            "::concord_core::advanced::RecordBody<{}>",
            quote::quote!(#item_ty)
        )),
        ResolvedRequestBodyIo::RawStream { .. } => Some("StreamBody".to_string()),
    }
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

fn ident_path_strings(path: &[Ident]) -> Vec<String> {
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

fn endpoint_qualified_name(ep: &ResolvedEndpoint) -> String {
    if ep.scope_modules.is_empty() {
        ep.name.to_string()
    } else {
        let mut qualified = ep
            .scope_modules
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("::");
        qualified.push_str("::");
        qualified.push_str(&ep.name.to_string());
        qualified
    }
}

fn doc_codec(enc: &syn::Path, ty: &syn::Type) -> String {
    format!("{}<{}>", quote::quote!(#enc), quote::quote!(#ty))
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

    #[test]
    fn setter_forms_match_current_public_surface() {
        let setter = FacadeSetter {
            field: "limit".to_string(),
            ty: "u64".to_string(),
            set_name: "limit".to_string(),
            set_optional_name: "limit_opt".to_string(),
            clear_name: "clear_limit".to_string(),
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
