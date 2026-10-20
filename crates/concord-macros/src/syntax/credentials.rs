//! Credential sources and authentication placements (§11.1).
//!
//! `owner` parameters are the owning client/scope/endpoint label named by
//! every diagnostic (DIAG-01).

use std::collections::HashSet;

use syn::parse::ParseStream;
use syn::spanned::Spanned;
use syn::{Ident, LitStr, Token, Type, braced, parenthesized};

use super::ast::{
    AuthChallenge, AuthOperation, AuthPlacement, AuthPlacementKind, CredentialDecl,
    CredentialSource, CredentialsBlock, SecretDecl,
};
use super::diagnostics::{
    Code, coded, validate_auth_header_name, validate_auth_query_name, validate_authorization_scheme,
};
use super::keyword;
use super::values::{AttrSite, parse_attrs};
use crate::semantic::surface::canonical_ident;

pub(crate) fn parse_credentials(
    input: ParseStream<'_>,
    owner: &str,
) -> syn::Result<CredentialsBlock> {
    input.parse::<keyword::credentials>()?;
    let body;
    braced!(body in input);
    let mut secrets = Vec::new();
    let mut credentials = Vec::new();
    let mut secret_names = HashSet::new();
    let mut credential_names = HashSet::new();
    while !body.is_empty() {
        let attrs = parse_attrs(&body, AttrSite::Registry, "a credential declaration", owner)?;
        if body.peek(keyword::secret) {
            body.parse::<keyword::secret>()?;
            let name: Ident = body.parse()?;
            let logical = canonical_ident(&name);
            if body.peek(Token![?]) {
                return Err(coded(
                    body.span(),
                    Code::Auth,
                    format!(
                        "optional static secrets are not supported (secret `{logical}` of {owner}); use an endpoint-backed source (initially empty) or a custom provider that may report unavailable"
                    ),
                ));
            }
            if body.peek(Token![:]) {
                return Err(coded(
                    body.span(),
                    Code::Migrate,
                    format!(
                        "typed secrets were removed; write `secret {logical};` in {owner} (the builder accepts `SecretString`)"
                    ),
                ));
            }
            body.parse::<Token![;]>()?;
            if !secret_names.insert(logical.clone()) {
                return Err(coded(
                    name.span(),
                    Code::Auth,
                    format!("duplicate secret `{logical}` in {owner}; declare each secret once"),
                ));
            }
            secrets.push(SecretDecl { attrs, name });
            continue;
        }
        if !body.peek(keyword::credential) {
            return Err(coded(
                body.span(),
                Code::Auth,
                format!(
                    "the credentials block of {owner} accepts only `secret NAME;` and `credential NAME = source;`"
                ),
            ));
        }
        body.parse::<keyword::credential>()?;
        let name: Ident = body.parse()?;
        let logical = canonical_ident(&name);
        let context = format!("credential `{logical}` of {owner}");
        body.parse::<Token![=]>()?;
        let source = parse_credential_source(&body, &secret_names, &context)?;
        body.parse::<Token![;]>()?;
        if !credential_names.insert(logical.clone()) {
            return Err(coded(
                name.span(),
                Code::Auth,
                format!(
                    "duplicate credential `{logical}` in {owner}; declare each credential once (one secret may feed several credentials)"
                ),
            ));
        }
        credentials.push(CredentialDecl {
            attrs,
            name,
            source,
        });
    }
    Ok(CredentialsBlock {
        secrets,
        credentials,
    })
}

/// `context` names the credential being declared (``credential `x` of
/// client `Api` ``).
fn parse_credential_source(
    input: ParseStream<'_>,
    secrets: &HashSet<String>,
    context: &str,
) -> syn::Result<CredentialSource> {
    let one = |input: ParseStream<'_>, what: &str| -> syn::Result<Ident> {
        let content;
        parenthesized!(content in input);
        let secret = parse_secret_reference(&content, secrets, context)?;
        if !content.is_empty() {
            return Err(coded(
                content.span(),
                Code::Auth,
                format!(
                    "{what} of {context} requires exactly one secret reference; write `(secret.NAME)`"
                ),
            ));
        }
        Ok(secret)
    };
    if input.peek(keyword::api_key) {
        input.parse::<keyword::api_key>()?;
        return Ok(CredentialSource::ApiKey {
            secret: one(input, "an API-key source")?,
        });
    }
    if input.peek(keyword::bearer) {
        input.parse::<keyword::bearer>()?;
        return Ok(CredentialSource::Bearer {
            secret: one(input, "a bearer source")?,
        });
    }
    if input.peek(keyword::basic) {
        input.parse::<keyword::basic>()?;
        let content;
        parenthesized!(content in input);
        let username = parse_secret_reference(&content, secrets, context)?;
        content.parse::<Token![,]>()?;
        let password = parse_secret_reference(&content, secrets, context)?;
        if content.peek(Token![,]) {
            content.parse::<Token![,]>()?;
        }
        if !content.is_empty() {
            return Err(coded(
                content.span(),
                Code::Auth,
                format!(
                    "Basic credentials of {context} require exactly two secret references; write `basic(secret.user, secret.password)`"
                ),
            ));
        }
        return Ok(CredentialSource::Basic { username, password });
    }
    if input.peek(keyword::endpoint) {
        input.parse::<keyword::endpoint>()?;
        return Ok(CredentialSource::Endpoint {
            path: input.parse()?,
        });
    }
    if input.peek(keyword::custom) {
        input.parse::<keyword::custom>()?;
        let provider: Type = input.parse()?;
        if !matches!(provider, Type::Path(_)) {
            return Err(coded(
                provider.span(),
                Code::Auth,
                format!(
                    "a custom credential provider of {context} must be a Rust type path; write `custom crate::MyProvider`"
                ),
            ));
        }
        return Ok(CredentialSource::Custom { provider });
    }
    Err(coded(
        input.span(),
        Code::Auth,
        format!(
            "the source of {context} must be `api_key(..)`, `bearer(..)`, `basic(..)`, `endpoint PATH` or `custom Type`"
        ),
    ))
}

fn parse_secret_reference(
    input: ParseStream<'_>,
    secrets: &HashSet<String>,
    context: &str,
) -> syn::Result<Ident> {
    input.parse::<keyword::secret>().map_err(|_| {
        input.error(format!(
            "[C3-AUTH] expected `secret.NAME` in the source of {context}"
        ))
    })?;
    input.parse::<Token![.]>()?;
    let name: Ident = input.parse()?;
    if !secrets.contains(&canonical_ident(&name)) {
        return Err(coded(
            name.span(),
            Code::Auth,
            format!(
                "unknown or not-yet-declared secret `{}` in the source of {context}; declare `secret {};` earlier in the credentials block",
                canonical_ident(&name),
                canonical_ident(&name)
            ),
        ));
    }
    Ok(name)
}

pub(crate) fn parse_auth(input: ParseStream<'_>, owner: &str) -> syn::Result<AuthOperation> {
    input.parse::<keyword::auth>()?;
    if input.peek(keyword::off) {
        input.parse::<keyword::off>()?;
        input.parse::<Token![;]>()?;
        return Ok(AuthOperation::Off);
    }
    let replace = input.peek(keyword::replace);
    if replace {
        input.parse::<keyword::replace>()?;
    }
    if !input.peek(syn::token::Brace) {
        return Err(coded(
            input.span(),
            Code::Auth,
            format!(
                "group the auth placements of {owner} in `auth {{ ... }}` or `auth replace {{ ... }}`, or write `auth off;`"
            ),
        ));
    }
    let body;
    braced!(body in input);
    let mut placements = Vec::new();
    while !body.is_empty() {
        placements.push(parse_auth_placement(&body, owner)?);
    }
    Ok(AuthOperation::Modify {
        replace,
        placements,
    })
}

fn parse_auth_placement(input: ParseStream<'_>, owner: &str) -> syn::Result<AuthPlacement> {
    let span = input.span();
    let (kind, credential) = if input.peek(keyword::bearer) {
        input.parse::<keyword::bearer>()?;
        (AuthPlacementKind::Bearer, input.parse()?)
    } else if input.peek(keyword::basic) {
        input.parse::<keyword::basic>()?;
        (AuthPlacementKind::Basic, input.parse()?)
    } else if input.peek(keyword::authorization) {
        input.parse::<keyword::authorization>()?;
        let scheme = if input.peek(LitStr) {
            let scheme: LitStr = input.parse()?;
            validate_authorization_scheme(&scheme, owner)?;
            Some(scheme)
        } else {
            None
        };
        input.parse::<Token![=]>()?;
        (AuthPlacementKind::Authorization(scheme), input.parse()?)
    } else if input.peek(keyword::header) {
        input.parse::<keyword::header>()?;
        let name: LitStr = input.parse()?;
        validate_auth_header_name(&name, owner)?;
        input.parse::<Token![=]>()?;
        (AuthPlacementKind::Header(name), input.parse()?)
    } else if input.peek(keyword::query) {
        input.parse::<keyword::query>()?;
        let name: LitStr = input.parse()?;
        validate_auth_query_name(&name, owner)?;
        input.parse::<Token![=]>()?;
        (AuthPlacementKind::Query(name), input.parse()?)
    } else {
        return Err(coded(
            input.span(),
            Code::Auth,
            format!(
                "an auth placement of {owner} is `bearer`, `basic`, `authorization`, `header \"Name\" =` or `query \"name\" =` followed by a credential name"
            ),
        ));
    };
    let challenge = if input.peek(keyword::challenge) {
        input.parse::<keyword::challenge>()?;
        Some(if input.peek(keyword::never) {
            input.parse::<keyword::never>()?;
            AuthChallenge::Never
        } else if input.peek(keyword::unauthorized_or_forbidden) {
            input.parse::<keyword::unauthorized_or_forbidden>()?;
            AuthChallenge::UnauthorizedOrForbidden
        } else if input.peek(keyword::unauthorized) {
            input.parse::<keyword::unauthorized>()?;
            AuthChallenge::Unauthorized
        } else {
            return Err(coded(
                input.span(),
                Code::Auth,
                format!(
                    "`challenge` in {owner} requires `never`, `unauthorized` or `unauthorized_or_forbidden`"
                ),
            ));
        })
    } else {
        None
    };
    input.parse::<Token![;]>()?;
    Ok(AuthPlacement {
        kind,
        credential,
        challenge,
        span,
    })
}
