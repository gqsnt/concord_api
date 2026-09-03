use std::collections::HashSet;

use syn::parse::ParseStream;
use syn::spanned::Spanned;
use syn::{Ident, LitStr, Token, Type, braced, parenthesized};

use super::ast::{
    AuthChallenge, AuthOperation, AuthPlacement, AuthPlacementKind, CredentialDecl,
    CredentialSource, CredentialsBlock, SecretDecl,
};
use super::diagnostics::{
    SpanError, validate_auth_header_name, validate_auth_query_name, validate_authorization_scheme,
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
            if body.peek(Token![:]) {
                return Err(body.error(
                    "v1 typed secrets were removed; write `secret NAME;` (the builder accepts `SecretString`)",
                ));
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
    if !input.peek(syn::token::Brace)
        && (input.peek(keyword::bearer)
            || input.peek(keyword::basic)
            || input.peek(keyword::authorization)
            || input.peek(keyword::header)
            || input.peek(keyword::query))
    {
        return Err(input.error(
            "v1 repeated `auth ...;` clauses were removed; group placements in `auth { ... }`",
        ));
    }
    if let Ok(spelling) = input.fork().parse::<Ident>() {
        match canonical_ident(&spelling).as_str() {
            "certificate" => {
                return Err(spelling.span().error(
                    "`auth certificate` is not DSL syntax; supply application client-identity PEM with `.configure_http(|http| http.identity_pem(pem))` on the generated builder",
                ));
            }
            "all" | "any" | "one_of" | "optional" => {
                return Err(spelling.span().error(
                    "authentication combinators are not DSL syntax; write ordered placements inside one `auth { ... }` block",
                ));
            }
            _ => {}
        }
    }
    let body;
    braced!(body in input);
    let mut placements = Vec::new();
    while !body.is_empty() {
        placements.push(parse_auth_placement(&body)?);
    }
    if placements.is_empty() {
        return Err(body.error("an authentication block requires at least one placement"));
    }
    Ok(AuthOperation::Modify {
        replace,
        clause_spans: vec![key_span],
        placements,
    })
}

fn parse_auth_placement(input: ParseStream<'_>) -> syn::Result<AuthPlacement> {
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
            validate_authorization_scheme(&scheme)?;
            Some(scheme)
        } else {
            None
        };
        input.parse::<Token![=]>()?;
        (AuthPlacementKind::Authorization(scheme), input.parse()?)
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
        if let Ok(spelling) = input.fork().parse::<Ident>() {
            match canonical_ident(&spelling).as_str() {
                "certificate" => {
                    return Err(spelling.span().error(
                        "`auth certificate` is not DSL syntax; supply application client-identity PEM with `.configure_http(|http| http.identity_pem(pem))` on the generated builder",
                    ));
                }
                "all" | "any" | "one_of" | "optional" => {
                    return Err(spelling.span().error(
                        "authentication combinators are not DSL syntax; declare the required ordered placements directly inside `auth { ... }`",
                    ));
                }
                _ => {}
            }
        }
        return Err(
            input.error(
                "auth placement must be `bearer`, `basic`, `authorization`, `header`, `query`, or `off`",
            )
        );
    };
    let challenge = if input.peek(keyword::recover) {
        input.parse::<keyword::recover>()?;
        input.parse::<keyword::on>()?;
        Some(if input.peek(keyword::unauthorized) {
            input.parse::<keyword::unauthorized>()?;
            AuthChallenge::Unauthorized
        } else if input.peek(keyword::unauthorized_or_forbidden) {
            input.parse::<keyword::unauthorized_or_forbidden>()?;
            AuthChallenge::UnauthorizedOrForbidden
        } else {
            return Err(input
                .error("recovery signal must be `unauthorized` or `unauthorized_or_forbidden`"));
        })
    } else if input.peek(keyword::challenge) {
        return Err(input.error(
            "v1 `challenge ...` was removed; use `recover on unauthorized`, `recover on unauthorized_or_forbidden`, or omit recovery",
        ));
    } else {
        None
    };
    input.parse::<Token![;]>()?;
    Ok(AuthPlacement {
        kind,
        credential,
        challenge,
    })
}

#[cfg(test)]
mod tests {
    use proc_macro2::Span;
    use quote::quote;
    use syn::LitStr;

    use crate::{MAX_AUTH_PLACEMENT_NAME_BYTES, MAX_AUTHORIZATION_SCHEME_BYTES};

    #[test]
    fn parses_secret_sources_generic_providers_and_auth_placements() {
        let source = super::super::parse(quote! {
            client AuthApi {
                timeout off;
                base "https://example.test";
                credentials {
                    secret token;
                    credential bearer = bearer(secret.token);
                    credential session = custom crate::SessionProvider;
                }
                auth { bearer bearer; }

            GET read() -> Bytes using Bytes {
                auth replace { header "X-Session" = session; }
            }
        }})
        .expect("credential syntax");
        let block = &source.client.credentials.as_ref().unwrap().value;

        assert_eq!(block.secrets.len(), 1);
        assert_eq!(block.credentials.len(), 2);
        assert!(source.client.attachment_site.auth.is_some());
    }

    #[test]
    fn rejects_certificate_and_authentication_combinator_ghost_syntax() {
        for (spelling, expected) in [
            (quote!(certificate), "`auth certificate` is not DSL syntax"),
            (quote!(all), "authentication combinators are not DSL syntax"),
            (quote!(any), "authentication combinators are not DSL syntax"),
            (
                quote!(one_of),
                "authentication combinators are not DSL syntax",
            ),
            (
                quote!(optional),
                "authentication combinators are not DSL syntax",
            ),
        ] {
            let source = quote! {
                client AuthApi {
                    timeout off;
                    base "https://example.test";
                    auth { #spelling; }
                }
            };
            let error = super::super::parse(source)
                .err()
                .expect("ghost authentication syntax must fail");
            assert!(error.to_string().contains(expected), "{error}");
        }
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

    #[test]
    fn parses_exact_and_static_scheme_authorization_placements() {
        let source = super::super::parse(quote! {
            client AuthApi {
                timeout off;
                base "https://example.test";
                credentials {
                    secret token;
                    credential key = api_key(secret.token);
                }

            GET exact() -> Bytes using Bytes {
                auth { authorization = key; }
            }
            GET scheme() -> Bytes using Bytes {
                auth { authorization "tOkEn" = key recover on unauthorized; }
            }
        }})
        .expect("custom Authorization syntax");

        let exact = match &source.client.children[0] {
            super::super::ast::SourceChild::Endpoint(endpoint) => endpoint,
            _ => panic!("exact endpoint"),
        };
        let scheme = match &source.client.children[1] {
            super::super::ast::SourceChild::Endpoint(endpoint) => endpoint,
            _ => panic!("scheme endpoint"),
        };
        assert!(matches!(
            exact.attachment_site.auth.as_ref(),
            Some(super::super::ast::AuthOperation::Modify { placements, .. })
                if matches!(
                    &placements[0].kind,
                    super::super::ast::AuthPlacementKind::Authorization(None)
                )
        ));
        assert!(matches!(
            scheme.attachment_site.auth.as_ref(),
            Some(super::super::ast::AuthOperation::Modify { placements, .. })
                if matches!(
                    &placements[0].kind,
                    super::super::ast::AuthPlacementKind::Authorization(Some(value))
                        if value.value() == "tOkEn"
                )
        ));
    }

    #[test]
    fn authorization_scheme_exact_limit_and_invalid_forms_are_diagnosed() {
        let exact = LitStr::new(
            &"T".repeat(MAX_AUTHORIZATION_SCHEME_BYTES),
            Span::call_site(),
        );
        super::validate_authorization_scheme(&exact).expect("exact scheme limit");

        let excess = LitStr::new(
            &"T".repeat(MAX_AUTHORIZATION_SCHEME_BYTES + 1),
            Span::call_site(),
        );
        assert_eq!(
            super::validate_authorization_scheme(&excess)
                .expect_err("first excess scheme")
                .to_string(),
            "an Authorization scheme permits at most 256 ASCII bytes"
        );

        for (invalid, expected) in [
            ("", "an Authorization scheme cannot be empty"),
            (
                "Token value",
                "an Authorization scheme must contain only ASCII HTTP token characters",
            ),
            (
                "Tökén",
                "an Authorization scheme must contain only ASCII HTTP token characters",
            ),
            (
                "Token:",
                "an Authorization scheme must contain only ASCII HTTP token characters",
            ),
        ] {
            let literal = LitStr::new(invalid, Span::call_site());
            assert_eq!(
                super::validate_authorization_scheme(&literal)
                    .expect_err("invalid scheme")
                    .to_string(),
                expected
            );
        }
    }
}
