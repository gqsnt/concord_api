use std::collections::HashSet;

use syn::parse::ParseStream;
use syn::spanned::Spanned;
use syn::{Ident, LitStr, Token, Type, braced, parenthesized};

use super::ast::{
    AuthChallenge, AuthOperation, AuthPlacement, AuthPlacementKind, CredentialDecl,
    CredentialSource, CredentialsBlock, SecretDecl,
};
use super::diagnostics::{
    SpanError, is_named_marker, validate_auth_header_name, validate_auth_query_name,
};
use super::keyword;
use crate::semantic::surface::canonical_ident;

pub(super) fn parse_credentials(input: ParseStream<'_>) -> syn::Result<CredentialsBlock> {
    input.parse::<keyword::credentials>()?;
    let body;
    braced!(body in input);
    let mut secrets = Vec::new();
    let mut credentials = Vec::new();
    let mut secret_names = HashSet::new();
    let mut credential_names = HashSet::new();
    while !body.is_empty() {
        if body.peek(keyword::secret) {
            body.parse::<keyword::secret>()?;
            let name: Ident = body.parse()?;
            body.parse::<Token![:]>()?;
            let ty: Type = body.parse()?;
            if !is_named_marker(&ty, "String") {
                return Err(ty
                    .span()
                    .error("Concord 0.1 secrets are required `String` values without defaults"));
            }
            body.parse::<Token![;]>()?;
            let canonical = canonical_ident(&name);
            if !secret_names.insert(canonical.clone()) {
                return Err(name.span().error(format!("duplicate secret `{canonical}`")));
            }
            secrets.push(SecretDecl { name });
            continue;
        }
        if !body.peek(keyword::credential) {
            return Err(body
                .error("a credentials block accepts only `secret` and `credential` declarations"));
        }
        body.parse::<keyword::credential>()?;
        let name: Ident = body.parse()?;
        body.parse::<Token![=]>()?;
        let source = parse_credential_source(&body, &secret_names)?;
        body.parse::<Token![;]>()?;
        let canonical = canonical_ident(&name);
        if !credential_names.insert(canonical.clone()) {
            return Err(name
                .span()
                .error(format!("duplicate credential `{canonical}`")));
        }
        credentials.push(CredentialDecl { name, source });
    }
    Ok(CredentialsBlock {
        secrets,
        credentials,
    })
}

fn parse_credential_source(
    input: ParseStream<'_>,
    declared_secrets: &HashSet<String>,
) -> syn::Result<CredentialSource> {
    if input.peek(keyword::api_key) {
        input.parse::<keyword::api_key>()?;
        let content;
        parenthesized!(content in input);
        let secret = parse_secret_reference(&content, declared_secrets)?;
        if !content.is_empty() {
            return Err(content.error("an API-key source requires exactly one secret reference"));
        }
        return Ok(CredentialSource::ApiKey { secret });
    }
    if input.peek(keyword::bearer) {
        input.parse::<keyword::bearer>()?;
        let content;
        parenthesized!(content in input);
        let secret = parse_secret_reference(&content, declared_secrets)?;
        if !content.is_empty() {
            return Err(content.error("a bearer source requires exactly one secret reference"));
        }
        return Ok(CredentialSource::Bearer { secret });
    }
    if input.peek(keyword::basic) {
        input.parse::<keyword::basic>()?;
        let content;
        parenthesized!(content in input);
        let username = parse_secret_reference(&content, declared_secrets)?;
        content.parse::<Token![,]>()?;
        let password = parse_secret_reference(&content, declared_secrets)?;
        if content.peek(Token![,]) {
            content.parse::<Token![,]>()?;
        }
        if !content.is_empty() {
            return Err(content.error("Basic credentials require exactly two secret references"));
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
            return Err(provider
                .span()
                .error("a custom credential provider must be a Rust type path"));
        }
        return Ok(CredentialSource::Custom { provider });
    }
    Err(input
        .error("credential source must be `api_key`, `bearer`, `basic`, `endpoint`, or `custom`"))
}

fn parse_secret_reference(
    input: ParseStream<'_>,
    declared_secrets: &HashSet<String>,
) -> syn::Result<Ident> {
    input.parse::<keyword::secret>()?;
    input.parse::<Token![.]>()?;
    let name: Ident = input.parse()?;
    if !declared_secrets.contains(&canonical_ident(&name)) {
        return Err(name.span().error(format!(
            "unknown or not-yet-declared secret `{}`",
            canonical_ident(&name)
        )));
    }
    Ok(name)
}

pub(super) fn parse_auth(input: ParseStream<'_>) -> syn::Result<AuthOperation> {
    let key_span = input.span();
    input.parse::<keyword::auth>()?;
    if input.peek(keyword::off) {
        input.parse::<keyword::off>()?;
        input.parse::<Token![;]>()?;
        return Ok(AuthOperation::Off { key_span });
    }
    let replace = input.peek(keyword::replace);
    if replace {
        input.parse::<keyword::replace>()?;
    }
    let (kind, credential) = if input.peek(keyword::bearer) {
        input.parse::<keyword::bearer>()?;
        (AuthPlacementKind::Bearer, input.parse()?)
    } else if input.peek(keyword::basic) {
        input.parse::<keyword::basic>()?;
        (AuthPlacementKind::Basic, input.parse()?)
    } else if input.peek(keyword::header) {
        input.parse::<keyword::header>()?;
        let name: LitStr = input.parse()?;
        validate_auth_header_name(&name)?;
        input.parse::<Token![=]>()?;
        (AuthPlacementKind::Header(name), input.parse()?)
    } else if input.peek(keyword::query) {
        input.parse::<keyword::query>()?;
        let name: LitStr = input.parse()?;
        validate_auth_query_name(&name)?;
        input.parse::<Token![=]>()?;
        (AuthPlacementKind::Query(name), input.parse()?)
    } else {
        return Err(
            input.error("auth placement must be `bearer`, `basic`, `header`, `query`, or `off`")
        );
    };
    let challenge = if input.peek(keyword::challenge) {
        input.parse::<keyword::challenge>()?;
        Some(if input.peek(keyword::unauthorized) {
            input.parse::<keyword::unauthorized>()?;
            AuthChallenge::Unauthorized
        } else if input.peek(keyword::unauthorized_or_forbidden) {
            input.parse::<keyword::unauthorized_or_forbidden>()?;
            AuthChallenge::UnauthorizedOrForbidden
        } else if input.peek(keyword::never) {
            input.parse::<keyword::never>()?;
            AuthChallenge::Never
        } else {
            return Err(input.error(
                "challenge must be `unauthorized`, `unauthorized_or_forbidden`, or `never`",
            ));
        })
    } else {
        None
    };
    input.parse::<Token![;]>()?;
    Ok(AuthOperation::Modify {
        replace,
        placements: vec![AuthPlacement {
            kind,
            credential,
            challenge,
        }],
        clause_spans: vec![key_span],
    })
}

#[cfg(test)]
mod tests {
    use proc_macro2::Span;
    use quote::quote;
    use syn::LitStr;

    use crate::MAX_AUTH_PLACEMENT_NAME_BYTES;

    #[test]
    fn parses_secret_sources_generic_providers_and_auth_placements() {
        let source = super::super::parse(quote! {
            client AuthApi {
                base "https://example.test";
                credentials {
                    secret token: String;
                    credential bearer = bearer(secret.token);
                    credential session = custom crate::SessionProvider;
                }
                auth bearer bearer;
            }
            GET read() -> Bytes {
                auth replace header "X-Session" = session challenge never;
            }
        })
        .expect("credential syntax");
        let block = &source.client.credentials.as_ref().unwrap().value;

        assert_eq!(block.secrets.len(), 1);
        assert_eq!(block.credentials.len(), 2);
        assert!(source.client.layer.auth.is_some());
    }

    #[test]
    fn auth_header_name_exact_256_bytes_passes() {
        let value = "a".repeat(MAX_AUTH_PLACEMENT_NAME_BYTES);
        let name = LitStr::new(&value, Span::call_site());

        super::validate_auth_header_name(&name).expect("exact header-name limit");
    }

    #[test]
    fn auth_header_name_257_bytes_fails() {
        let value = "a".repeat(MAX_AUTH_PLACEMENT_NAME_BYTES + 1);
        let name = LitStr::new(&value, Span::call_site());
        let error = super::validate_auth_header_name(&name).expect_err("first excess header name");

        assert_eq!(
            error.to_string(),
            "an authentication header name permits at most 256 UTF-8 bytes"
        );
    }

    #[test]
    fn auth_query_name_exact_256_bytes_passes() {
        let value = "q".repeat(MAX_AUTH_PLACEMENT_NAME_BYTES);
        let name = LitStr::new(&value, Span::call_site());

        super::validate_auth_query_name(&name).expect("exact query-name limit");
    }

    #[test]
    fn auth_query_name_257_bytes_fails() {
        let value = "q".repeat(MAX_AUTH_PLACEMENT_NAME_BYTES + 1);
        let name = LitStr::new(&value, Span::call_site());
        let error = super::validate_auth_query_name(&name).expect_err("first excess query name");

        assert_eq!(
            error.to_string(),
            "an authentication query name permits at most 256 UTF-8 bytes"
        );
    }

    #[test]
    fn auth_name_limit_counts_utf8_bytes() {
        let exact_value = "é".repeat(MAX_AUTH_PLACEMENT_NAME_BYTES / 2);
        assert_eq!(exact_value.len(), MAX_AUTH_PLACEMENT_NAME_BYTES);
        let exact = LitStr::new(&exact_value, Span::call_site());
        super::validate_auth_query_name(&exact).expect("exact multi-byte query-name limit");

        let excess_value = format!("{exact_value}a");
        assert_eq!(
            excess_value.chars().count(),
            MAX_AUTH_PLACEMENT_NAME_BYTES / 2 + 1
        );
        assert_eq!(excess_value.len(), MAX_AUTH_PLACEMENT_NAME_BYTES + 1);
        let excess = LitStr::new(&excess_value, Span::call_site());
        super::validate_auth_query_name(&excess).expect_err("first excess UTF-8 byte");
    }
}
