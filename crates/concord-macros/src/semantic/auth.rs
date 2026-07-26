use std::collections::HashMap;

use proc_macro2::Span;
use syn::Ident;
use syn::spanned::Spanned;

use super::SpanError;
use super::model::{
    Child, Credential, CredentialSource, Endpoint, NormalizedConfig, ResolvedAuthPlacement,
    ResolvedChallengePolicy, ResolvedExpr, Secret, Value, ValueShape,
};
use super::surface::{
    RESERVED_GENERATED_METHODS, canonical_ident, insert_name, to_snake_case, value_setter_ident,
};
use crate::MAX_AUTH_PLACEMENTS;
use crate::syntax::ast::{
    AuthPlacementKind as SourceAuthPlacement, CredentialSource as SourceCredentialSource,
    CredentialsBlock,
};

pub(super) fn convert_credentials(
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
    let mut used_secrets = vec![false; secrets.len()];
    let mut resolve_secret = |secret: &Ident| {
        let index = secret_index[&canonical_ident(secret)];
        used_secrets[index] = true;
        index
    };
    let mut credentials = Vec::new();
    for credential in block.credentials {
        let source = match credential.source {
            SourceCredentialSource::ApiKey { secret } => CredentialSource::ApiKey {
                secret: resolve_secret(&secret),
            },
            SourceCredentialSource::Bearer { secret } => CredentialSource::Bearer {
                secret: resolve_secret(&secret),
            },
            SourceCredentialSource::Basic { username, password } => CredentialSource::Basic {
                username: resolve_secret(&username),
                password: resolve_secret(&password),
            },
            SourceCredentialSource::Endpoint { path } => CredentialSource::Endpoint {
                path,
                endpoint_id: None,
            },
            SourceCredentialSource::Custom { provider } => CredentialSource::Custom { provider },
        };
        credentials.push(Credential {
            name: credential.name,
            source,
        });
    }
    let mut unused = secrets
        .iter()
        .zip(used_secrets)
        .filter(|(_, used)| !used)
        .map(|(secret, _)| {
            secret.name.span().error(format!(
                "secret `{}` is never used by a credential source",
                canonical_ident(&secret.name)
            ))
        });
    if let Some(mut error) = unused.next() {
        for additional in unused {
            error.combine(additional);
        }
        return Err(error);
    }
    Ok((secrets, credentials))
}

pub(super) fn find_credential(credentials: &[Credential], name: &Ident) -> syn::Result<usize> {
    credentials
        .iter()
        .position(|credential| canonical_ident(&credential.name) == canonical_ident(name))
        .ok_or_else(|| {
            name.span()
                .error(format!("unknown credential `{}`", canonical_ident(name)))
        })
}

pub(super) fn validate_auth_compatibility(
    placement: &SourceAuthPlacement,
    source: &CredentialSource,
    span: &Ident,
) -> syn::Result<()> {
    let compatible = match source {
        CredentialSource::ApiKey { .. } => {
            matches!(
                placement,
                SourceAuthPlacement::Header(_) | SourceAuthPlacement::Query(_)
            )
        }
        CredentialSource::Bearer { .. } => matches!(placement, SourceAuthPlacement::Bearer),
        CredentialSource::Basic { .. } => matches!(placement, SourceAuthPlacement::Basic),
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

pub(super) fn validate_effective_auth(config: &NormalizedConfig) -> syn::Result<()> {
    if let Some(first_excess) = config.auth.get(MAX_AUTH_PLACEMENTS) {
        return Err(first_excess.span.error(format!(
            "an effective authentication plan permits at most {MAX_AUTH_PLACEMENTS} placements"
        )));
    }
    let challenge_count = config
        .auth
        .iter()
        .filter(|placement| placement.challenge != ResolvedChallengePolicy::Never)
        .count();
    if challenge_count > 1 {
        let span = config
            .auth
            .iter()
            .find(|placement| placement.challenge != ResolvedChallengePolicy::Never)
            .map(|placement| placement.span)
            .unwrap_or_else(Span::call_site);
        return Err(span.error(
            "one effective authentication plan may contain at most one challenge-enabled placement",
        ));
    }
    for (index, placement) in config.auth.iter().enumerate() {
        match &placement.kind {
            ResolvedAuthPlacement::Bearer | ResolvedAuthPlacement::Basic => {
                if config
                    .auth
                    .iter()
                    .enumerate()
                    .any(|(other, candidate)| {
                        other != index
                            && (matches!(
                                &candidate.kind,
                                ResolvedAuthPlacement::Bearer | ResolvedAuthPlacement::Basic
                            ) || matches!(
                                &candidate.kind,
                                ResolvedAuthPlacement::Header(name)
                                    if name.value().eq_ignore_ascii_case("authorization")
                            ))
                    })
                {
                    return Err(placement.span.error(
                        "effective authentication placements collide on `Authorization`",
                    ));
                }
            }
            ResolvedAuthPlacement::Header(name) => {
                let canonical = name.value().to_ascii_lowercase();
                if canonical == "authorization"
                    || config.headers.iter().any(|header| {
                        header.canonical_name == canonical
                            && resolved_expr_is_guaranteed_present(&header.value)
                    })
                    || config.auth[..index].iter().any(|candidate| {
                        matches!(&candidate.kind, ResolvedAuthPlacement::Header(other) if other.value().eq_ignore_ascii_case(&name.value()))
                    })
                {
                    return Err(name.span().error(
                        "authentication header collides with an effective public or authentication header",
                    ));
                }
            }
            ResolvedAuthPlacement::Query(name) => {
                if config.query.iter().any(|query| {
                    query.key.value() == name.value()
                        && resolved_expr_is_guaranteed_present(&query.value)
                }) || config.auth[..index].iter().any(|candidate| {
                    matches!(&candidate.kind, ResolvedAuthPlacement::Query(other) if other.value() == name.value())
                }) {
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

pub(super) fn validate_credential_names(
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
            canonical_ident(&value_setter_ident(&value.name)),
            value.name.span(),
            "generated client builder method",
        )?;
        if value.accepts_absence() {
            insert_name(
                &mut builder,
                format!("clear_{}", canonical_ident(&value.name)),
                value.name.span(),
                "generated client builder method",
            )?;
        }
        if value.has_default() {
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

pub(super) fn resolve_endpoint_credentials(
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

pub(super) fn validate_credential_cycles(
    credentials: &[Credential],
    children: &[Child],
) -> syn::Result<()> {
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

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::super::model::Api;
    use super::super::test_support;
    use super::*;

    fn resolve(tokens: proc_macro2::TokenStream) -> syn::Result<Api> {
        let source = crate::syntax::parse(tokens)?;
        super::super::resolve(source)
    }

    #[test]
    fn normalizes_generic_credential_sources_and_effective_auth() {
        let api = test_support::resolve_tokens(quote! {
            client AuthApi {
                base "https://example.test";
                credentials {
                    secret token: String;
                    credential bearer = bearer(secret.token);
                }
                auth bearer bearer;
            }
            GET read() -> Bytes;
        });
        let endpoint = test_support::endpoint(&api.client.children, "read");

        assert_eq!(api.client.credentials.len(), 1);
        assert!(matches!(
            api.client.credentials[0].source,
            CredentialSource::Bearer { secret: 0 }
        ));
        assert_eq!(endpoint.config.auth.len(), 1);
        assert_eq!(endpoint.config.auth[0].credential, 0);
        assert_eq!(
            endpoint.config.auth[0].challenge,
            ResolvedChallengePolicy::Never
        );
    }

    #[test]
    fn unused_secret_is_rejected() {
        let error = resolve(quote! {
            client AuthApi {
                base "https://example.test";
                credentials {
                    secret unused: String;
                }
            }
            GET read() -> Bytes;
        })
        .err()
        .expect("unused secret");

        assert_eq!(
            error.to_string(),
            "secret `unused` is never used by a credential source"
        );
    }

    #[test]
    fn all_builtin_secret_uses_count_as_used() {
        resolve(quote! {
            client AuthApi {
                base "https://example.test";
                credentials {
                    secret api_key: String;
                    secret token: String;
                    secret username: String;
                    secret password: String;
                    credential api = api_key(secret.api_key);
                    credential bearer = bearer(secret.token);
                    credential basic = basic(secret.username, secret.password);
                }
            }
            GET read() -> Bytes;
        })
        .expect("all built-in credential sources consume their declared secrets");
    }

    #[test]
    fn basic_username_and_password_each_count_as_used() {
        resolve(quote! {
            client AuthApi {
                base "https://example.test";
                credentials {
                    secret username: String;
                    secret password: String;
                    credential basic = basic(secret.username, secret.password);
                }
            }
            GET read() -> Bytes;
        })
        .expect("both Basic components consume their declared secrets");
    }

    #[test]
    fn one_secret_used_by_multiple_credentials_is_valid() {
        resolve(quote! {
            client AuthApi {
                base "https://example.test";
                credentials {
                    secret shared: String;
                    credential first = api_key(secret.shared);
                    credential second = api_key(secret.shared);
                }
            }
            GET read() -> Bytes;
        })
        .expect("one declared secret may be shared by credentials");
    }

    #[test]
    fn multiple_unused_secrets_report_deterministically() {
        let error = resolve(quote! {
            client AuthApi {
                base "https://example.test";
                credentials {
                    secret first: String;
                    secret used: String;
                    secret second: String;
                    credential bearer = bearer(secret.used);
                }
            }
            GET read() -> Bytes;
        })
        .err()
        .expect("multiple unused secrets");
        let diagnostic = error.into_compile_error().to_string();
        let first = diagnostic
            .find("secret `first` is never used by a credential source")
            .expect("first declaration diagnostic");
        let second = diagnostic
            .find("secret `second` is never used by a credential source")
            .expect("second declaration diagnostic");

        assert_eq!(
            diagnostic
                .matches("is never used by a credential source")
                .count(),
            2
        );
        assert!(first < second);
    }
}
