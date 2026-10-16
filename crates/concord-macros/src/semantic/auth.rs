//! Credential sources and effective authentication placements (§11).

use std::collections::HashMap;

use proc_macro2::Span;

use super::head::Potential;
use super::surface::{canonical_ident, prefixed_ident, private_prefixed_ident};
use super::{
    AuthChallenge, Child, Credential, CredentialSource, LocalAuth, Placement, PlacementFamily,
    PlacementKind, Resolver, Secret, Site, meta_from,
};
use crate::syntax::ast::{AuthOperation, AuthPlacementKind, CredentialSource as SourceKind};
use crate::syntax::diagnostics::{Code, coded, related};

pub(crate) fn resolve_credentials(resolver: &mut Resolver) -> syn::Result<()> {
    let Some(block) = resolver.credentials_source.take() else {
        return Ok(());
    };
    let client = resolver.model.owner_label(0);
    let secrets: Vec<Secret> = block
        .secrets
        .iter()
        .map(|secret| Secret {
            name: secret.name.clone(),
            logical: canonical_ident(&secret.name),
            meta: meta_from(&secret.attrs),
            field: private_prefixed_ident("s", &secret.name),
        })
        .collect();
    let secret_index = |name: &syn::Ident| {
        let logical = canonical_ident(name);
        secrets
            .iter()
            .position(|secret| secret.logical == logical)
            .ok_or_else(|| {
                coded(
                    name.span(),
                    Code::Auth,
                    format!(
                        "unknown secret `{logical}` in the credentials of {client}; declare `secret {logical};` first"
                    ),
                )
            })
    };
    let mut credentials = Vec::with_capacity(block.credentials.len());
    for declaration in &block.credentials {
        let source = match &declaration.source {
            SourceKind::ApiKey { secret } => CredentialSource::ApiKey {
                secret: secret_index(secret)?,
            },
            SourceKind::Bearer { secret } => CredentialSource::Bearer {
                secret: secret_index(secret)?,
            },
            SourceKind::Basic { username, password } => {
                let username = secret_index(username)?;
                let password = secret_index(password)?;
                if username == password {
                    return Err(coded(
                        declaration.name.span(),
                        Code::Auth,
                        format!(
                            "Basic credentials require two distinct secrets for username and password (credential `{}` of {client}); write `basic(secret.user, secret.password)` with two declared secrets",
                            canonical_ident(&declaration.name)
                        ),
                    ));
                }
                CredentialSource::Basic { username, password }
            }
            SourceKind::Endpoint { path } => {
                let context = format!(
                    "credential `{}` of {client}",
                    canonical_ident(&declaration.name)
                );
                let endpoint = find_endpoint(resolver, path, &context)?;
                let path_text = path
                    .segments
                    .iter()
                    .map(|segment| canonical_ident(&segment.ident))
                    .collect::<Vec<_>>()
                    .join("::");
                CredentialSource::Endpoint {
                    endpoint,
                    path_text,
                }
            }
            SourceKind::Custom { provider } => CredentialSource::Custom {
                provider: provider.clone(),
            },
        };
        let credential = Credential {
            name: declaration.name.clone(),
            logical: canonical_ident(&declaration.name),
            meta: meta_from(&declaration.attrs),
            source,
            field: private_prefixed_ident("c", &declaration.name),
            clear: prefixed_ident("clear", &declaration.name),
            replace: prefixed_ident("replace", &declaration.name),
            store_terminal: prefixed_ident("store_as", &declaration.name),
        };
        if let CredentialSource::Endpoint { endpoint, .. } = &credential.source {
            let index = credentials.len();
            resolver.model.endpoints[*endpoint].storage.push(index);
        }
        credentials.push(credential);
    }
    // §3.5: a dead required input cannot be retained. Every secret is a
    // required protected builder input, so one no credential source reads is
    // rejected rather than demanded from every caller for no effect.
    for (index, secret) in secrets.iter().enumerate() {
        let used = credentials
            .iter()
            .any(|credential| match credential.source {
                CredentialSource::ApiKey { secret: used }
                | CredentialSource::Bearer { secret: used } => used == index,
                CredentialSource::Basic { username, password } => {
                    username == index || password == index
                }
                CredentialSource::Endpoint { .. } | CredentialSource::Custom { .. } => false,
            });
        if !used {
            return Err(coded(
                secret.name.span(),
                Code::Auth,
                format!(
                    "secret `{0}` of {client} is never used by a credential source; every secret is a required builder input, so remove it or read it with `api_key(secret.{0})`, `bearer(secret.{0})` or `basic(...)`",
                    secret.logical
                ),
            ));
        }
    }
    resolver.model.client.secrets = secrets;
    resolver.model.client.credentials = credentials;
    Ok(())
}

/// Resolves a root-relative `scope::scope::endpoint` path (forward references
/// are allowed because the whole tree is known).
///
/// `context` names the credential being resolved (DIAG-01).
fn find_endpoint(resolver: &Resolver, path: &syn::Path, context: &str) -> syn::Result<usize> {
    let segments: Vec<String> = path
        .segments
        .iter()
        .map(|segment| canonical_ident(&segment.ident))
        .collect();
    let mut owner = 0;
    for (index, segment) in segments.iter().enumerate() {
        let last = index + 1 == segments.len();
        let children = &resolver.model.owners[owner].children;
        if last {
            let found = children.iter().find_map(|child| match child {
                Child::Endpoint(endpoint)
                    if resolver.model.endpoints[*endpoint].logical == *segment =>
                {
                    Some(*endpoint)
                }
                _ => None,
            });
            return found.ok_or_else(|| {
                coded(
                    syn::spanned::Spanned::span(path),
                    Code::Auth,
                    format!("credential source endpoint `{}` does not name an endpoint (paths are root-relative: `scope::endpoint`); {context} must name a declared endpoint such as `endpoint auth::login`", segments.join("::")),
                )
            });
        }
        let found = children.iter().find_map(|child| match child {
            Child::Scope(scope) if resolver.model.owners[*scope].logical == *segment => {
                Some(*scope)
            }
            _ => None,
        });
        owner = found.ok_or_else(|| {
            coded(
                syn::spanned::Spanned::span(path),
                Code::Auth,
                format!(
                    "credential source path segment `{segment}` does not name a scope ({context}); write the root-relative path `scope::endpoint` of a declared endpoint"
                ),
            )
        })?;
    }
    Err(coded(
        syn::spanned::Spanned::span(path),
        Code::Auth,
        format!("empty credential source path ({context}); write `endpoint scope::endpoint`"),
    ))
}

fn resolve_local(
    resolver: &Resolver,
    site: Site,
    operation: &AuthOperation,
) -> syn::Result<LocalAuth> {
    let origin = resolver.site_display(site);
    let label = resolver.site_label(site);
    let credentials = &resolver.model.client.credentials;
    let placements =
        |placements: &[crate::syntax::ast::AuthPlacement]| -> syn::Result<Vec<Placement>> {
            placements
            .iter()
            .map(|placement| {
                let logical = canonical_ident(&placement.credential);
                let credential = credentials
                    .iter()
                    .position(|credential| credential.logical == logical)
                    .ok_or_else(|| {
                        coded(
                            placement.credential.span(),
                            Code::Auth,
                            format!("unknown credential `{logical}` in the auth placement of {label}; declare `credential {logical} = source;` in the client `credentials {{ ... }}` block"),
                        )
                    })?;
                let kind = match &placement.kind {
                    AuthPlacementKind::Bearer => PlacementKind::Bearer,
                    AuthPlacementKind::Basic => PlacementKind::Basic,
                    AuthPlacementKind::Authorization(scheme) => {
                        PlacementKind::Authorization(scheme.as_ref().map(syn::LitStr::value))
                    }
                    AuthPlacementKind::Header(name) => PlacementKind::Header(name.value()),
                    AuthPlacementKind::Query(name) => PlacementKind::Query(name.value()),
                };
                let source = &credentials[credential].source;
                let basic_source = matches!(source, CredentialSource::Basic { .. });
                let secret_only = matches!(source, CredentialSource::ApiKey { .. } | CredentialSource::Bearer { .. });
                if kind == PlacementKind::Basic && secret_only {
                    return Err(coded(
                        placement.span,
                        Code::Auth,
                        format!("`basic {logical}` in {label} requires a Basic credential capability; `{logical}` is a single-secret source, so write `bearer {logical}`, `authorization = {logical}`, `header \"Name\" = {logical}` or `query \"name\" = {logical}`"),
                    ));
                }
                if kind != PlacementKind::Basic && basic_source {
                    return Err(coded(
                        placement.span,
                        Code::Auth,
                        format!("`{logical}` is a Basic username/password source; only a `basic` placement can apply it, so write `basic {logical};` in {label}"),
                    ));
                }
                Ok(Placement {
                    kind,
                    credential,
                    challenge: placement.challenge.unwrap_or(AuthChallenge::Never),
                    origin: origin.clone(),
                    span: placement.span,
                })
            })
            .collect()
        };
    Ok(match operation {
        AuthOperation::Modify {
            replace: false,
            placements: list,
        } => LocalAuth::Append(placements(list)?),
        AuthOperation::Modify {
            replace: true,
            placements: list,
        } => LocalAuth::Replace(placements(list)?),
        AuthOperation::Off => LocalAuth::Off,
    })
}

fn compose(inherited: &[Placement], local: Option<&LocalAuth>) -> Vec<Placement> {
    match local {
        None => inherited.to_vec(),
        Some(LocalAuth::Append(placements)) => {
            inherited.iter().chain(placements).cloned().collect()
        }
        Some(LocalAuth::Replace(placements)) => placements.clone(),
        Some(LocalAuth::Off) => Vec::new(),
    }
}

fn validate(
    resolver: &Resolver,
    site: Site,
    placements: &[Placement],
    potential: &Potential,
    span: Span,
) -> syn::Result<()> {
    if placements.len() > crate::MAX_AUTH_PLACEMENTS {
        return Err(coded(
            span,
            Code::Auth,
            format!(
                "{} has {} effective auth placements; at most {} are allowed, so drop inherited placements with `auth replace {{ ... }}`",
                resolver.site_label(site),
                placements.len(),
                crate::MAX_AUTH_PLACEMENTS
            ),
        ));
    }
    let mut targets: HashMap<(PlacementFamily, String), &Placement> = HashMap::new();
    let mut challenger: Option<&Placement> = None;
    for placement in placements {
        let target = placement.kind.target();
        if let Some(first) = targets.insert(target.clone(), placement) {
            let error = coded(
                placement.span,
                Code::Auth,
                format!(
                    "auth placement target `{}` of {} is already claimed by an effective placement from {}; use `auth replace {{ ... }}` to select a different placement",
                    target.1,
                    resolver.site_label(site),
                    first.origin
                ),
            );
            return Err(related(error, first.span, "the earlier placement is here"));
        }
        if placement.challenge != AuthChallenge::Never {
            if let Some(first) = challenger {
                let error = coded(
                    placement.span,
                    Code::Auth,
                    format!(
                        "at most one effective auth placement of {} may attribute a challenge; keep `challenge ...` on one placement and write `challenge never` (or omit it) on the others",
                        resolver.site_label(site)
                    ),
                );
                return Err(related(
                    error,
                    first.span,
                    "the other challenge-attributing placement is here",
                ));
            }
            challenger = Some(placement);
        }
        if let Some(writer) = potential.contains(target.0, &target.1) {
            let error = coded(
                placement.span,
                Code::Auth,
                format!(
                    "private auth target `{}` of {} collides with a public {} writer ({}); public and private-auth targets have distinct owners, so rename or remove the public writer",
                    target.1,
                    resolver.site_label(site),
                    match target.0 {
                        PlacementFamily::Query => "query",
                        PlacementFamily::Header => "header",
                    },
                    writer.source
                ),
            );
            return Err(related(error, writer.span, "the public writer is here"));
        }
    }
    Ok(())
}

pub(crate) fn resolve_owner(resolver: &mut Resolver, owner: usize) -> syn::Result<()> {
    let site = Site::Owner(owner);
    let source = resolver.owner_sources[owner].site.auth.take();
    let local = source
        .as_ref()
        .map(|clause| resolve_local(resolver, site, &clause.value))
        .transpose()?;
    let inherited = resolver.model.owners[owner]
        .parent
        .map(|parent| resolver.model.owners[parent].effective_auth.clone())
        .unwrap_or_default();
    let effective = compose(&inherited, local.as_ref());
    let span = source
        .as_ref()
        .map_or(resolver.model.owners[owner].span, |clause| clause.key_span);
    validate(
        resolver,
        site,
        &effective,
        &resolver.owner_potential[owner],
        span,
    )?;
    let entry = &mut resolver.model.owners[owner];
    entry.auth = local;
    entry.effective_auth = effective;
    Ok(())
}

pub(crate) fn resolve_endpoint(resolver: &mut Resolver, endpoint: usize) -> syn::Result<()> {
    let site = Site::Endpoint(endpoint);
    let source = resolver.endpoint_sources[endpoint].site.auth.take();
    let local = source
        .as_ref()
        .map(|clause| resolve_local(resolver, site, &clause.value))
        .transpose()?;
    let owner = resolver.model.endpoints[endpoint].owner;
    let effective = compose(&resolver.model.owners[owner].effective_auth, local.as_ref());
    let span = source
        .as_ref()
        .map_or(resolver.model.endpoints[endpoint].span, |clause| {
            clause.key_span
        });
    validate(
        resolver,
        site,
        &effective,
        &resolver.endpoint_potential[endpoint],
        span,
    )?;
    // Challenge statuses are disjoint from accepted payload and absence statuses.
    let entry = &resolver.model.endpoints[endpoint];
    for placement in &effective {
        let statuses: &[u16] = match placement.challenge {
            AuthChallenge::Never => &[],
            AuthChallenge::Unauthorized => &[401],
            AuthChallenge::UnauthorizedOrForbidden => &[401, 403],
        };
        for status in statuses {
            if entry.status.contains(*status)
                || entry
                    .absence
                    .as_ref()
                    .is_some_and(|absence| absence.contains(status))
            {
                let error = coded(
                    entry.span,
                    Code::Auth,
                    format!(
                        "{} accepts status {status}, which overlaps the effective auth challenge attributed by {}; remove {status} from `status`/`absent on` or write `challenge never` on that placement",
                        resolver.site_label(site),
                        placement.origin
                    ),
                );
                return Err(related(
                    error,
                    placement.span,
                    "the challenge placement is here",
                ));
            }
        }
    }
    let entry = &mut resolver.model.endpoints[endpoint];
    entry.auth = local;
    entry.effective_auth = effective;
    Ok(())
}

/// Rejects credential-source cycles and incompatible source endpoints.
pub(crate) fn validate_credential_graph(resolver: &Resolver) -> syn::Result<()> {
    let credentials = &resolver.model.client.credentials;
    for (index, credential) in credentials.iter().enumerate() {
        let CredentialSource::Endpoint { endpoint, .. } = &credential.source else {
            continue;
        };
        let source = &resolver.model.endpoints[*endpoint];
        if source.absence.is_some() || source.pagination.is_some() {
            return Err(coded(
                credential.name.span(),
                Code::Auth,
                format!(
                    "credential `{}` of {} names source endpoint `{}`, which must return complete credential material; `absent on` and pagination are incompatible with `store_as_{}`, so remove them from the source endpoint",
                    credential.logical,
                    resolver.model.owner_label(0),
                    source.display,
                    credential.logical
                ),
            ));
        }
        // Walk the source endpoints reachable through effective placements.
        let mut stack = vec![*endpoint];
        let mut visited = vec![false; resolver.model.endpoints.len()];
        while let Some(current) = stack.pop() {
            if std::mem::replace(&mut visited[current], true) {
                continue;
            }
            for placement in &resolver.model.endpoints[current].effective_auth {
                if placement.credential == index {
                    let error = coded(
                        credential.name.span(),
                        Code::Auth,
                        format!(
                            "credential `{}` of {} is obtained from `{}`, whose effective authentication requires the same credential; the source graph has a cycle, so authenticate the source endpoint with another credential or write `auth off;` on it",
                            credential.logical,
                            resolver.model.owner_label(0),
                            source.display
                        ),
                    );
                    return Err(related(
                        error,
                        placement.span,
                        "the cyclic placement is here",
                    ));
                }
                if let CredentialSource::Endpoint { endpoint: next, .. } =
                    &credentials[placement.credential].source
                {
                    stack.push(*next);
                }
            }
        }
    }
    Ok(())
}
