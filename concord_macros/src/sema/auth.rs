fn analyze_auth_credentials(
    block: Option<&AuthCredentials>,
    auth_vars: &BTreeMap<String, VarInfo>,
    endpoint_outputs: &BTreeMap<String, Type>,
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
                token_url.value().parse::<url::Url>().map_err(|err| {
                    syn::Error::new(token_url.span(), format!("invalid OAuth2 token URL: {err}"))
                })?;
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
                let endpoint_key = endpoint_ref_key(endpoint)?;
                let output_ty = endpoint_outputs.get(&endpoint_key).ok_or_else(|| {
                    syn::Error::new(
                        endpoint.span(),
                        format!("unknown auth endpoint `{endpoint_key}` in credential source"),
                    )
                })?;
                AuthCredentialKindIr::Endpoint {
                    endpoint: endpoint.clone(),
                    endpoint_key,
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

fn endpoint_ref_key(path: &syn::Path) -> Result<String> {
    if path.segments.is_empty() {
        return Err(syn::Error::new_spanned(
            path,
            "auth endpoint reference must be `Endpoint(Name)` or `Endpoint(scope::Name)`",
        ));
    }
    let mut out = Vec::new();
    for segment in &path.segments {
        if !matches!(segment.arguments, syn::PathArguments::None) {
            return Err(syn::Error::new_spanned(
                segment,
                "auth endpoint reference segments must not contain generic arguments",
            ));
        }
        out.push(segment.ident.to_string());
    }
    Ok(out.join("::"))
}

fn validate_required_secret(
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

fn resolve_auth_requirements(
    uses: &[NormAuthUse],
    credentials: &BTreeMap<String, AuthCredentialIr>,
    provenance: AuthUseProvenanceIr,
) -> Result<Vec<AuthUsePlanIr>> {
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

fn resolve_auth_use_kind(
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
        AuthUseKind::Bearer { credential } => AuthUseKindIr::Bearer {
            credential: credential.clone(),
        },
        AuthUseKind::Header { header, credential } => AuthUseKindIr::Header {
            header: header.clone(),
            credential: credential.clone(),
        },
        AuthUseKind::Query { key, credential } => AuthUseKindIr::Query {
            key: key.clone(),
            credential: credential.clone(),
        },
        AuthUseKind::Basic { credential } => AuthUseKindIr::Basic {
            credential: credential.clone(),
        },
        AuthUseKind::Certificate { credential } => AuthUseKindIr::Certificate {
            credential: credential.clone(),
        },
    };
    Ok(AuthUseIr { kind, provenance })
}

fn auth_use_credential_ident(u: &AuthUseKind) -> &Ident {
    match u {
        AuthUseKind::Bearer { credential }
        | AuthUseKind::Header { credential, .. }
        | AuthUseKind::Query { credential, .. }
        | AuthUseKind::Basic { credential }
        | AuthUseKind::Certificate { credential } => credential,
    }
}

fn auth_use_credential_ident_ir(u: &AuthUseIr) -> &Ident {
    match &u.kind {
        AuthUseKindIr::Bearer { credential }
        | AuthUseKindIr::Header { credential, .. }
        | AuthUseKindIr::Query { credential, .. }
        | AuthUseKindIr::Basic { credential }
        | AuthUseKindIr::Certificate { credential } => credential,
    }
}

fn auth_plan_references_credential(plans: &[AuthUsePlanIr], target: &Ident) -> bool {
    plans.iter().any(|plan| match plan {
        AuthUsePlanIr::Use(auth_use) => {
            auth_use_credential_ident_ir(auth_use) == target
        }
    })
}

fn validate_auth_usage_fit(u: &AuthUseKind, cred: &AuthCredentialIr) -> Result<()> {
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
        AuthUseKind::Certificate { .. } => matches!(shape, AuthMaterialShapeIr::Certificate),
    };

    if fits {
        return Ok(());
    }

    match u {
        AuthUseKind::Bearer { credential } => Err(syn::Error::new(
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
        AuthUseKind::Basic { credential } => Err(syn::Error::new(
            credential.span(),
            format!(
                "BasicAuth requires BasicCredential material; `{}` does not fit",
                cred.name
            ),
        )),
        AuthUseKind::Certificate { credential } => Err(syn::Error::new(
            credential.span(),
            format!(
                "CertificateAuth requires ClientCertificate material; `{}` does not fit",
                cred.name
            ),
        )),
    }
}

fn shape_from_type(ty: &Type) -> AuthMaterialShapeIr {
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
        "ClientCertificate" => AuthMaterialShapeIr::Certificate,
        _ => AuthMaterialShapeIr::Unknown,
    }
}

