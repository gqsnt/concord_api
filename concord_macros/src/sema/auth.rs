use super::*;

pub(super) fn analyze_auth_credentials(
    block: Option<&AuthCredentials>,
    auth_vars: &BTreeMap<String, VarInfo>,
    endpoint_outputs: &BTreeMap<EndpointTargetKey, Type>,
) -> Result<Vec<AuthCredentialIr>> {
    let Some(block) = block else {
        return Ok(Vec::new());
    };

    let mut seen: BTreeMap<String, Span> = BTreeMap::new();
    let mut out = Vec::new();
    for decl in &block.credentials {
        let name_key = decl.name.to_string();
        if seen.contains_key(&name_key) {
            return Err(syn::Error::new(
                decl.name.span(),
                format!("duplicate auth credential `{}`", decl.name),
            ));
        }
        seen.insert(name_key, decl.name.span());

        let kind = match &decl.kind {
            AuthCredentialKind::ApiKey { secret } => {
                validate_required_secret(secret, auth_vars)?;
                AuthCredentialKindIr::ApiKey {
                    secret: secret.ident.clone(),
                }
            }
            AuthCredentialKind::StaticBearer { secret } => {
                validate_required_secret(secret, auth_vars)?;
                AuthCredentialKindIr::StaticBearer {
                    secret: secret.ident.clone(),
                }
            }
            AuthCredentialKind::Basic { username, password } => {
                validate_required_secret(username, auth_vars)?;
                validate_required_secret(password, auth_vars)?;
                AuthCredentialKindIr::Basic {
                    username: username.ident.clone(),
                    password: password.ident.clone(),
                }
            }
            AuthCredentialKind::OAuth2ClientCredentials {
                token_url,
                client_id,
                client_secret,
                scope,
            } => {
                validate_oauth2_token_url(token_url)?;
                validate_required_secret(client_id, auth_vars)?;
                validate_required_secret(client_secret, auth_vars)?;
                AuthCredentialKindIr::OAuth2ClientCredentials {
                    token_url: token_url.clone(),
                    client_id: client_id.ident.clone(),
                    client_secret: client_secret.ident.clone(),
                    scope: scope.clone(),
                }
            }
            AuthCredentialKind::Endpoint { endpoint } => {
                let target = endpoint_target_from_path(endpoint)?;
                let output_ty = endpoint_outputs.get(&target.key()).ok_or_else(|| {
                    syn::Error::new(
                        endpoint.span(),
                        format!(
                            "unknown auth endpoint `{}` in credential source",
                            target.display_string()
                        ),
                    )
                })?;
                AuthCredentialKindIr::Endpoint {
                    target,
                    output_ty: output_ty.clone(),
                    material_shape: shape_from_type(output_ty),
                }
            }
        };

        out.push(AuthCredentialIr {
            name: decl.name.clone(),
            kind,
        });
    }

    Ok(out)
}

pub(super) fn endpoint_target_from_path(path: &syn::Path) -> Result<EndpointTargetIr> {
    if path.segments.is_empty() {
        return Err(syn::Error::new_spanned(
            path,
            "auth endpoint reference must be `Endpoint(Name)` or `Endpoint(scope::Name)`",
        ));
    }
    let mut scope_modules = Vec::new();
    for segment in &path.segments {
        if !matches!(segment.arguments, syn::PathArguments::None) {
            return Err(syn::Error::new_spanned(
                segment,
                "auth endpoint reference segments must not contain generic arguments",
            ));
        }
    }
    for segment in path
        .segments
        .iter()
        .take(path.segments.len().saturating_sub(1))
    {
        scope_modules.push(segment.ident.clone());
    }
    let endpoint = path
        .segments
        .last()
        .expect("checked for non-empty path above")
        .ident
        .clone();
    Ok(EndpointTargetIr {
        scope_modules,
        endpoint,
    })
}

pub(super) fn validate_required_secret(
    secret: &SecretRef,
    auth_vars: &BTreeMap<String, VarInfo>,
) -> Result<()> {
    let Some(info) = auth_vars.get(&secret.ident.to_string()) else {
        return Err(syn::Error::new(
            secret.ident.span(),
            format!(
                "unknown secret `secret.{}` in auth credential",
                secret.ident
            ),
        ));
    };
    if info.optional {
        return Err(syn::Error::new(
            secret.ident.span(),
            format!(
                "auth credential secret `secret.{}` must be required; optional secrets are not supported yet",
                secret.ident
            ),
        ));
    }
    Ok(())
}

pub(super) fn resolve_auth_requirements(
    uses: &[NormAuthUse],
    credentials: &BTreeMap<String, AuthCredentialIr>,
    provenance: AuthUseProvenanceIr,
) -> Result<Vec<AuthUsePlanIr>> {
    reject_duplicate_auth_materialization_keys(uses)?;

    let mut out = Vec::new();
    for u in uses {
        out.push(AuthUsePlanIr::Use(Box::new(resolve_auth_use_kind(
            &u.kind,
            credentials,
            provenance,
        )?)));
    }
    Ok(out)
}

pub(super) fn reject_duplicate_auth_materialization_keys(uses: &[NormAuthUse]) -> Result<()> {
    let mut headers: BTreeMap<String, String> = BTreeMap::new();
    let mut queries: BTreeMap<String, Span> = BTreeMap::new();

    for u in uses {
        match &u.kind {
            AuthUseKind::Header { header, .. } => {
                let normalized = header.value().to_ascii_lowercase();
                if let Some(first) = headers.insert(normalized, header.value()) {
                    return Err(syn::Error::new(
                        header.span(),
                        format!("duplicate auth header `{first}` in the same layer"),
                    ));
                }
            }
            AuthUseKind::Query { key, .. } => {
                let query = key.value();
                if queries.insert(query.clone(), key.span()).is_some() {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("duplicate auth query parameter `{query}` in the same layer"),
                    ));
                }
            }
            AuthUseKind::Bearer { .. } | AuthUseKind::Basic { .. } => {}
        }
    }

    Ok(())
}

pub(super) fn resolve_auth_use_kind(
    kind: &AuthUseKind,
    credentials: &BTreeMap<String, AuthCredentialIr>,
    provenance: AuthUseProvenanceIr,
) -> Result<AuthUseIr> {
    let credential = auth_use_credential_ident(kind);
    let cred = credentials.get(&credential.to_string()).ok_or_else(|| {
        syn::Error::new(
            credential.span(),
            unknown_name_message("auth credential", credential, credentials),
        )
    })?;
    validate_auth_usage_fit(kind, cred)?;

    let kind = match kind {
        AuthUseKind::Bearer {
            credential,
            challenge,
        } => AuthUseKindIr::Bearer {
            credential: credential.clone(),
            challenge: resolve_auth_challenge(challenge.as_ref())?,
        },
        AuthUseKind::Header {
            header,
            credential,
            challenge,
        } => AuthUseKindIr::Header {
            header: header.clone(),
            credential: credential.clone(),
            challenge: resolve_auth_challenge(challenge.as_ref())?,
        },
        AuthUseKind::Query {
            key,
            credential,
            challenge,
        } => AuthUseKindIr::Query {
            key: key.clone(),
            credential: credential.clone(),
            challenge: resolve_auth_challenge(challenge.as_ref())?,
        },
        AuthUseKind::Basic {
            credential,
            challenge,
        } => AuthUseKindIr::Basic {
            credential: credential.clone(),
            challenge: resolve_auth_challenge(challenge.as_ref())?,
        },
    };
    Ok(AuthUseIr { kind, provenance })
}

fn resolve_auth_challenge(challenge: Option<&Ident>) -> Result<AuthChallengePolicyIr> {
    let Some(challenge) = challenge else {
        return Ok(AuthChallengePolicyIr::Unauthorized);
    };
    match challenge.to_string().as_str() {
        "unauthorized" => Ok(AuthChallengePolicyIr::Unauthorized),
        "unauthorized_or_forbidden" => Ok(AuthChallengePolicyIr::UnauthorizedOrForbidden),
        "never_recover" => Ok(AuthChallengePolicyIr::NeverRecover),
        _ => Err(syn::Error::new(
            challenge.span(),
            "unknown auth challenge policy; expected `unauthorized`, `unauthorized_or_forbidden`, or `never_recover`",
        )),
    }
}

pub(super) fn auth_use_credential_ident(u: &AuthUseKind) -> &Ident {
    match u {
        AuthUseKind::Bearer { credential, .. }
        | AuthUseKind::Header { credential, .. }
        | AuthUseKind::Query { credential, .. }
        | AuthUseKind::Basic { credential, .. } => credential,
    }
}

pub(super) fn auth_plan_references_credential(plans: &[AuthRequirementIr], target: &Ident) -> bool {
    plans.iter().any(|plan| &plan.credential == target)
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) enum AuthMaterializationTargetKey {
    Header(String),
    Query(String),
    Authorization,
}

impl AuthMaterializationTargetKey {
    fn display_string(&self) -> String {
        match self {
            AuthMaterializationTargetKey::Header(name) => format!("header `{name}`"),
            AuthMaterializationTargetKey::Query(key) => format!("query `{key}`"),
            AuthMaterializationTargetKey::Authorization => "Authorization".to_string(),
        }
    }
}

pub(super) fn final_auth_materialization_target(
    req: &AuthRequirementIr,
) -> AuthMaterializationTargetKey {
    match &req.placement {
        AuthPlacementIr::Header { name } => {
            let name = name.value();
            if name.eq_ignore_ascii_case("authorization") {
                AuthMaterializationTargetKey::Authorization
            } else {
                AuthMaterializationTargetKey::Header(name.to_ascii_lowercase())
            }
        }
        AuthPlacementIr::Query { key } => AuthMaterializationTargetKey::Query(key.value()),
        AuthPlacementIr::Bearer | AuthPlacementIr::Basic => {
            AuthMaterializationTargetKey::Authorization
        }
    }
}

pub(crate) fn validate_final_auth_materialization_targets(
    auth: &[AuthRequirementIr],
    endpoint_display: &str,
    endpoint_span: Span,
) -> Result<()> {
    let mut seen: BTreeMap<AuthMaterializationTargetKey, &AuthRequirementIr> = BTreeMap::new();
    for req in auth {
        let target = final_auth_materialization_target(req);
        if let Some(first) = seen.insert(target.clone(), req) {
            return Err(syn::Error::new(
                endpoint_span,
                format!(
                    "final endpoint `{endpoint_display}` has duplicate auth materialization target `{}` between `{}` and `{}`",
                    target.display_string(),
                    first.provenance.label,
                    req.provenance.label
                ),
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_oauth2_token_url(token_url: &syn::LitStr) -> Result<()> {
    validate_oauth2_token_url_raw(&token_url.value(), token_url.span())?;
    let url = token_url.value().parse::<url::Url>().map_err(|err| {
        syn::Error::new(token_url.span(), format!("invalid OAuth2 token URL: {err}"))
    })?;
    if url.scheme() != "https"
        || url.host_str().is_none_or(|host| host.is_empty())
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(syn::Error::new(
            token_url.span(),
            "OAuth2 token URL must be an https URL with a host, no userinfo, and no fragment",
        ));
    }
    Ok(())
}

pub(super) fn validate_oauth2_token_url_raw(token_url: &str, span: Span) -> Result<()> {
    let Some(rest) = token_url.strip_prefix("https://") else {
        return Err(syn::Error::new(
            span,
            "OAuth2 token URL must be an https URL with a host, no userinfo, and no fragment",
        ));
    };
    let authority = rest
        .split_once(['/', '?', '#'])
        .map(|(authority, _)| authority)
        .unwrap_or(rest);
    if authority.is_empty() || authority.contains('@') || authority.starts_with('/') {
        return Err(syn::Error::new(
            span,
            "OAuth2 token URL must be an https URL with a host, no userinfo, and no fragment",
        ));
    }
    Ok(())
}

pub(super) fn validate_auth_usage_fit(u: &AuthUseKind, cred: &AuthCredentialIr) -> Result<()> {
    let shape = match &cred.kind {
        AuthCredentialKindIr::ApiKey { .. } => AuthMaterialShapeIr::SecretValue,
        AuthCredentialKindIr::StaticBearer { .. }
        | AuthCredentialKindIr::OAuth2ClientCredentials { .. } => AuthMaterialShapeIr::AccessToken,
        AuthCredentialKindIr::Basic { .. } => AuthMaterialShapeIr::Basic,
        AuthCredentialKindIr::Endpoint { material_shape, .. } => *material_shape,
    };

    let fits = match u {
        AuthUseKind::Bearer { .. } => {
            matches!(
                shape,
                AuthMaterialShapeIr::AccessToken | AuthMaterialShapeIr::Unknown
            )
        }
        AuthUseKind::Header { .. } | AuthUseKind::Query { .. } => {
            matches!(
                shape,
                AuthMaterialShapeIr::SecretValue
                    | AuthMaterialShapeIr::AccessToken
                    | AuthMaterialShapeIr::Unknown
            )
        }
        AuthUseKind::Basic { .. } => matches!(shape, AuthMaterialShapeIr::Basic),
    };

    if fits {
        return Ok(());
    }

    match u {
        AuthUseKind::Bearer { credential, .. } => Err(syn::Error::new(
            credential.span(),
            format!(
                "BearerAuth requires an access-token credential; `{}` does not fit",
                cred.name
            ),
        )),
        AuthUseKind::Header { credential, .. } | AuthUseKind::Query { credential, .. } => {
            Err(syn::Error::new(
                credential.span(),
                format!(
                    "header/query auth requires a secret credential; `{}` does not fit",
                    cred.name
                ),
            ))
        }
        AuthUseKind::Basic { credential, .. } => Err(syn::Error::new(
            credential.span(),
            format!(
                "BasicAuth requires BasicCredential material; `{}` does not fit",
                cred.name
            ),
        )),
    }
}

pub(crate) fn materialize_auth_requirement(
    auth_use: &AuthUseIr,
    endpoint_step_prefix: &str,
    idx: usize,
) -> AuthRequirementIr {
    let (placement, credential, challenge) = match &auth_use.kind {
        AuthUseKindIr::Bearer {
            credential,
            challenge,
        } => (AuthPlacementIr::Bearer, credential, *challenge),
        AuthUseKindIr::Header {
            header,
            credential,
            challenge,
        } => (
            AuthPlacementIr::Header {
                name: header.clone(),
            },
            credential,
            *challenge,
        ),
        AuthUseKindIr::Query {
            key,
            credential,
            challenge,
        } => (
            AuthPlacementIr::Query { key: key.clone() },
            credential,
            *challenge,
        ),
        AuthUseKindIr::Basic {
            credential,
            challenge,
        } => (AuthPlacementIr::Basic, credential, *challenge),
    };
    AuthRequirementIr {
        credential: credential.clone(),
        placement,
        usage_id: auth_usage_id(&auth_use.kind).to_string(),
        step_id: format!("{endpoint_step_prefix}:{idx}:{credential}"),
        provenance: AuthProvenanceIr {
            label: provenance_label(auth_use.provenance),
        },
        challenge,
    }
}

pub(crate) fn materialize_auth_requirements(
    plans: &[AuthUsePlanIr],
    endpoint_step_prefix: &str,
    start_idx: usize,
) -> Vec<AuthRequirementIr> {
    plans
        .iter()
        .enumerate()
        .map(|(idx, plan)| match plan {
            AuthUsePlanIr::Use(auth_use) => materialize_auth_requirement(
                auth_use.as_ref(),
                endpoint_step_prefix,
                start_idx + idx,
            ),
        })
        .collect()
}

pub(super) fn auth_usage_id(kind: &AuthUseKindIr) -> &'static str {
    match kind {
        AuthUseKindIr::Bearer { .. } => "bearer",
        AuthUseKindIr::Header { .. } => "header",
        AuthUseKindIr::Query { .. } => "query",
        AuthUseKindIr::Basic { .. } => "basic",
    }
}

pub(super) fn provenance_label(provenance: AuthUseProvenanceIr) -> String {
    match provenance {
        AuthUseProvenanceIr::Client => "client".to_string(),
        AuthUseProvenanceIr::Scope(scope_id) => format!("scope:{scope_id}"),
        AuthUseProvenanceIr::Endpoint => "endpoint".to_string(),
    }
}

pub(super) fn shape_from_type(ty: &Type) -> AuthMaterialShapeIr {
    let Type::Path(type_path) = ty else {
        return AuthMaterialShapeIr::Unknown;
    };
    let Some(segment) = type_path.path.segments.last() else {
        return AuthMaterialShapeIr::Unknown;
    };
    match segment.ident.to_string().as_str() {
        "AccessToken" => AuthMaterialShapeIr::AccessToken,
        "ApiKey" => AuthMaterialShapeIr::SecretValue,
        "BasicCredential" => AuthMaterialShapeIr::Basic,
        _ => AuthMaterialShapeIr::Unknown,
    }
}
