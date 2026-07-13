//! Parser for raw DSL syntax.
//!
//! This layer accepts current syntax only. It should not resolve inheritance
//! or names.

use crate::ast::*;
use crate::emit_helpers;
use crate::kw;
use crate::model::Scheme;
use proc_macro2::{Span, TokenStream as TokenStream2, TokenTree};
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{
    Expr, Ident, LitInt, LitStr, Path, Result, Token, Type, braced, bracketed, parenthesized, token,
};

impl Parse for RawApi {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let span = input.span();
        if input.peek(kw::api) {
            input.parse::<kw::api>()?;
            input.parse::<Token![!]>()?;
            let content;
            braced!(content in input);
            return parse_raw_api_body(&content, span);
        }
        parse_raw_api_body(input, span)
    }
}

fn parse_raw_api_body(input: ParseStream<'_>, span: Span) -> Result<RawApi> {
    let client: RawClient = input.parse()?;
    let mut items = Vec::new();
    while !input.is_empty() {
        items.push(input.parse::<RawItem>()?);
    }
    Ok(RawApi {
        span,
        client,
        items,
    })
}

impl Parse for RawClient {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let span = input.span();
        let client_kw: kw::client = input.parse()?;
        let client_span = client_kw.span;
        let name: Ident = input.parse()?;

        let content;
        braced!(content in input);
        let body_span = content.span();

        let mut scheme: Option<Scheme> = None;
        let mut host: Option<LitStr> = None;
        let mut vars: Option<VarsBlock> = None;
        let mut auth_vars: Option<VarsBlock> = None;
        let mut auth_credentials: Vec<AuthCredentialDecl> = Vec::new();
        let mut auth_uses: Vec<AuthUseDecl> = Vec::new();
        let mut default_behavior_uses: Vec<BehaviorUseSpec> = Vec::new();
        let mut rate_limit: Option<RateLimitProfilesBlock> = None;
        let mut behavior_profiles: Option<BehaviorProfilesBlock> = None;
        let mut policy = PolicyBlocks::default();
        let mut seen_default_block = false;

        while !content.is_empty() {
            if content.peek(kw::base) {
                content.parse::<kw::base>()?;
                if !content.peek(LitStr) {
                    return Err(syn::Error::new(
                        content.span(),
                        "base must use a single URL literal: `base \"https://example.com\"`",
                    ));
                }
                let base_url: LitStr = content.parse()?;
                let (parsed_scheme, parsed_host) = parse_base_url_literal(&base_url)?;
                scheme = Some(parsed_scheme);
                host = Some(LitStr::new(&parsed_host, base_url.span()));
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::var) {
                content.parse::<kw::var>()?;
                let decl: VarDeclNoWire = content.parse()?;
                vars.get_or_insert_with(|| VarsBlock { decls: Vec::new() })
                    .decls
                    .push(decl);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::secret) {
                content.parse::<kw::secret>()?;
                parse_client_secret_decl_into(&content, &mut auth_vars)?;
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::auth) {
                let fork = content.fork();
                fork.parse::<kw::auth>()?;
                if fork.peek(token::Brace) {
                    content.parse::<kw::auth>()?;
                    let auth_content;
                    braced!(auth_content in content);
                    parse_client_auth_group(&auth_content, &mut auth_vars, &mut auth_credentials)?;
                } else {
                    content.parse::<kw::auth>()?;
                    auth_uses.push(parse_auth_use_decl_after_auth_keyword(&content)?);
                }
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::credential) {
                content.parse::<kw::credential>()?;
                parse_client_credential_decl_into(&content, &mut auth_credentials)?;
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::retry) {
                return Err(removed_retry_syntax_error(&content)?);
            } else if content.peek(kw::rate_limit) {
                content.parse::<kw::rate_limit>()?;
                rate_limit
                    .get_or_insert_with(|| RateLimitProfilesBlock {
                        profiles: Vec::new(),
                        default: Vec::new(),
                        response_policy: None,
                    })
                    .profiles
                    .push(parse_rate_limit_profile_decl_after_keyword(&content)?);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::policies) {
                content.parse::<kw::policies>()?;
                let policy_content;
                braced!(policy_content in content);
                parse_client_policies_group(&policy_content, &mut rate_limit)?;
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::profile) {
                content.parse::<kw::profile>()?;
                behavior_profiles
                    .get_or_insert_with(|| BehaviorProfilesBlock {
                        profiles: Vec::new(),
                    })
                    .profiles
                    .push(parse_behavior_profile_decl_after_keyword(&content)?);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::profiles) {
                content.parse::<kw::profiles>()?;
                let behavior_content;
                braced!(behavior_content in content);
                while !behavior_content.is_empty() {
                    if behavior_content.peek(kw::behavior) {
                        let legacy: kw::behavior = behavior_content.parse()?;
                        return Err(legacy_behavior_keyword_error(legacy.span));
                    }
                    if !behavior_content.peek(kw::profile) {
                        let tt: TokenTree = behavior_content.parse()?;
                        return Err(syn::Error::new(
                            tt.span(),
                            "invalid item in profiles block; expected `profile NAME { ... }`",
                        ));
                    }
                    behavior_content.parse::<kw::profile>()?;
                    behavior_profiles
                        .get_or_insert_with(|| BehaviorProfilesBlock {
                            profiles: Vec::new(),
                        })
                        .profiles
                        .push(parse_behavior_profile_decl_after_keyword(
                            &behavior_content,
                        )?);
                    let _ = behavior_content.parse::<Option<Token![,]>>()?;
                }
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::behavior) {
                let legacy: kw::behavior = content.parse()?;
                return Err(legacy_behavior_keyword_error(legacy.span));
            } else if content.peek(kw::behaviors) {
                let legacy: kw::behaviors = content.parse()?;
                return Err(legacy_behaviors_keyword_error(legacy.span));
            } else if content.peek(kw::default) {
                let default_span = content.parse::<kw::default>()?.span;
                if seen_default_block {
                    return Err(syn::Error::new(
                        default_span,
                        "multiple default blocks are not allowed in the current DSL",
                    ));
                }
                seen_default_block = true;
                let default_content;
                braced!(default_content in content);
                parse_client_default_block(
                    &default_content,
                    &mut policy,
                    &mut auth_uses,
                    &mut default_behavior_uses,
                    &mut rate_limit,
                )?;
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::defaults) {
                let legacy: kw::defaults = content.parse()?;
                return Err(legacy_defaults_keyword_error(legacy.span));
            } else if content.peek(kw::observe) {
                content.parse::<kw::observe>()?;
                content.parse::<kw::rate_limit>()?;
                let observer: Path = content.parse()?;
                let block = rate_limit.get_or_insert_with(|| RateLimitProfilesBlock {
                    profiles: Vec::new(),
                    default: Vec::new(),
                    response_policy: None,
                });
                if block.response_policy.is_some() {
                    return Err(syn::Error::new(
                        observer.span(),
                        "duplicate rate_limit observer",
                    ));
                }
                block.response_policy = Some(observer);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::headers) {
                merge_policy_block(
                    &mut policy.headers,
                    content.parse::<PolicyBlockTaggedHeaders>()?.0,
                );
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::header) {
                push_policy_stmt(
                    &mut policy.headers,
                    parse_inline_policy_stmt(&content, PolicyBlockKind::Headers)?,
                );
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::query) {
                if content.peek2(token::Brace) {
                    merge_policy_block(
                        &mut policy.query,
                        content.parse::<PolicyBlockTaggedQuery>()?.0,
                    );
                } else {
                    push_policy_stmt(
                        &mut policy.query,
                        parse_inline_policy_stmt(&content, PolicyBlockKind::Query)?,
                    );
                }
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::timeout) {
                content.parse::<kw::timeout>()?;
                content.parse::<Token![:]>()?;
                policy.timeout = Some(content.parse::<Expr>()?);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else {
                let tt: proc_macro2::TokenTree = content.parse()?;
                return Err(syn::Error::new(
                    tt.span(),
                    "unexpected token in client block",
                ));
            }
        }

        let scheme = scheme.ok_or_else(|| {
            syn::Error::new(
                name.span(),
                "missing `base \"https://example.com\"` in client",
            )
        })?;
        let host = host.ok_or_else(|| {
            syn::Error::new(
                name.span(),
                "missing `base \"https://example.com\"` in client",
            )
        })?;

        Ok(Self {
            span,
            client_span,
            body_span,
            vars,
            auth_vars,
            auth: (!auth_credentials.is_empty()).then_some(AuthCredentials {
                credentials: auth_credentials,
            }),
            auth_uses,
            default_behavior_uses,
            name,
            scheme,
            host,
            policy,
            rate_limit,
            behavior_profiles,
        })
    }
}

fn parse_base_url_literal(base_url: &LitStr) -> Result<(Scheme, String)> {
    let raw = base_url.value();
    let (scheme, rest) = if let Some(rest) = raw.strip_prefix("https://") {
        (Scheme::Https, rest)
    } else if let Some(rest) = raw.strip_prefix("http://") {
        (Scheme::Http, rest)
    } else {
        return Err(syn::Error::new(
            base_url.span(),
            "base URL must start with `https://` or `http://`",
        ));
    };
    let host = rest.trim_end_matches('/');
    if host.is_empty()
        || host.contains('/')
        || host.contains('\\')
        || host.contains('@')
        || host.contains('?')
        || host.contains('#')
        || host.chars().any(|ch| ch.is_whitespace() || ch.is_control())
    {
        return Err(syn::Error::new(
            base_url.span(),
            "base URL must contain only scheme and host",
        ));
    }
    Ok((scheme, host.to_string()))
}

fn parse_client_auth_group(
    input: ParseStream<'_>,
    auth_vars: &mut Option<VarsBlock>,
    auth_credentials: &mut Vec<AuthCredentialDecl>,
) -> Result<()> {
    while !input.is_empty() {
        if input.peek(kw::secret) {
            input.parse::<kw::secret>()?;
            parse_client_secret_decl_into(input, auth_vars)?;
        } else if input.peek(kw::credential) {
            input.parse::<kw::credential>()?;
            parse_client_credential_decl_into(input, auth_credentials)?;
        } else {
            let tt: TokenTree = input.parse()?;
            return Err(syn::Error::new(
                tt.span(),
                "invalid item in auth block; expected secret or credential",
            ));
        }

        let _ = input.parse::<Option<Token![,]>>()?;
    }
    Ok(())
}

fn parse_client_secret_decl_into(
    input: ParseStream<'_>,
    auth_vars: &mut Option<VarsBlock>,
) -> Result<()> {
    let decl: VarDeclNoWire = input.parse()?;
    auth_vars
        .get_or_insert_with(|| VarsBlock { decls: Vec::new() })
        .decls
        .push(decl);
    Ok(())
}

fn parse_client_credential_decl_into(
    input: ParseStream<'_>,
    auth_credentials: &mut Vec<AuthCredentialDecl>,
) -> Result<()> {
    auth_credentials.push(parse_auth_credential_after_keyword(input, true)?);
    Ok(())
}

fn parse_client_policies_group(
    input: ParseStream<'_>,
    rate_limit: &mut Option<RateLimitProfilesBlock>,
) -> Result<()> {
    while !input.is_empty() {
        if input.peek(kw::retry) {
            return Err(removed_retry_syntax_error(input)?);
        } else if input.peek(kw::rate_limit) {
            let fork = input.fork();
            fork.parse::<kw::rate_limit>()?;
            if fork.peek(kw::off) || fork.peek(kw::only) {
                return Err(syn::Error::new(
                    fork.span(),
                    "default rate_limit policy is not allowed in policies block; use default { ... }",
                ));
            }
            if !fork.peek(Ident) {
                let tt: TokenTree = input.parse()?;
                return Err(syn::Error::new(
                    tt.span(),
                    "invalid item in policies block; expected rate_limit or observe",
                ));
            }
            fork.parse::<Ident>()?;
            if fork.peek(kw::extends) {
                fork.parse::<kw::extends>()?;
                fork.parse::<Ident>()?;
            }
            if !fork.peek(token::Brace) {
                return Err(syn::Error::new(
                    input.span(),
                    "default rate_limit policy is not allowed in policies block; use default { ... }",
                ));
            }

            input.parse::<kw::rate_limit>()?;
            rate_limit
                .get_or_insert_with(|| RateLimitProfilesBlock {
                    profiles: Vec::new(),
                    default: Vec::new(),
                    response_policy: None,
                })
                .profiles
                .push(parse_rate_limit_profile_decl_after_keyword(input)?);
        } else if input.peek(kw::observe) {
            input.parse::<kw::observe>()?;
            input.parse::<kw::rate_limit>()?;
            let observer: Path = input.parse()?;
            let block = rate_limit.get_or_insert_with(|| RateLimitProfilesBlock {
                profiles: Vec::new(),
                default: Vec::new(),
                response_policy: None,
            });
            if block.response_policy.is_some() {
                return Err(syn::Error::new(
                    observer.span(),
                    "duplicate rate_limit observer",
                ));
            }
            block.response_policy = Some(observer);
        } else {
            let tt: TokenTree = input.parse()?;
            return Err(syn::Error::new(
                tt.span(),
                "invalid item in policies block; expected rate_limit or observe",
            ));
        }

        let _ = input.parse::<Option<Token![,]>>()?;
    }

    Ok(())
}

fn parse_client_default_block(
    input: ParseStream<'_>,
    policy: &mut PolicyBlocks,
    auth_uses: &mut Vec<AuthUseDecl>,
    default_behavior_uses: &mut Vec<BehaviorUseSpec>,
    rate_limit: &mut Option<RateLimitProfilesBlock>,
) -> Result<()> {
    while !input.is_empty() {
        if input.peek(kw::headers) {
            merge_policy_block(
                &mut policy.headers,
                input.parse::<PolicyBlockTaggedHeaders>()?.0,
            );
        } else if input.peek(kw::header) {
            push_policy_stmt(
                &mut policy.headers,
                parse_inline_policy_stmt(input, PolicyBlockKind::Headers)?,
            );
        } else if input.peek(kw::query) {
            if input.peek2(token::Brace) {
                merge_policy_block(
                    &mut policy.query,
                    input.parse::<PolicyBlockTaggedQuery>()?.0,
                );
            } else {
                push_policy_stmt(
                    &mut policy.query,
                    parse_inline_policy_stmt(input, PolicyBlockKind::Query)?,
                );
            }
        } else if input.peek(kw::timeout) {
            input.parse::<kw::timeout>()?;
            if input.peek(Token![:]) {
                input.parse::<Token![:]>()?;
            }
            policy.timeout = Some(input.parse::<Expr>()?);
        } else if input.peek(kw::auth) {
            input.parse::<kw::auth>()?;
            auth_uses.push(parse_auth_use_decl_after_auth_keyword(input)?);
        } else if input.peek(kw::profile) {
            default_behavior_uses.push(parse_behavior_use_spec(input)?);
        } else if input.peek(kw::behavior) {
            let legacy: kw::behavior = input.parse()?;
            return Err(legacy_behavior_keyword_error(legacy.span));
        } else if input.peek(kw::retry) {
            return Err(removed_retry_syntax_error(input)?);
        } else if input.peek(kw::rate_limit) {
            let spec = parse_rate_limit_spec(input)?;
            let RateLimitSpec::Profiles {
                only: false,
                profiles,
            } = spec
            else {
                return Err(syn::Error::new(
                    input.span(),
                    "client default rate_limit must be a profile or profile list",
                ));
            };
            let block = rate_limit.get_or_insert_with(|| RateLimitProfilesBlock {
                profiles: Vec::new(),
                default: Vec::new(),
                response_policy: None,
            });
            if !block.default.is_empty() {
                return Err(syn::Error::new(
                    input.span(),
                    "duplicate default rate_limit policy",
                ));
            }
            block.default = profiles;
        } else {
            let tt: TokenTree = input.parse()?;
            return Err(syn::Error::new(
                tt.span(),
                "unexpected token in client default block",
            ));
        }
        let _ = input.parse::<Option<Token![,]>>()?;
    }
    Ok(())
}

fn removed_retry_syntax_error(input: ParseStream<'_>) -> Result<syn::Error> {
    let retry: kw::retry = input.parse()?;
    Ok(syn::Error::new(
        retry.span,
        "retry DSL was removed; configure client-level `RetryMode` when constructing the client",
    ))
}

fn legacy_behavior_keyword_error(span: Span) -> syn::Error {
    syn::Error::new(
        span,
        "`behavior` is not valid current DSL; use `profile` for profile declarations and attachments",
    )
}

fn legacy_behaviors_keyword_error(span: Span) -> syn::Error {
    syn::Error::new(
        span,
        "`behaviors` is not valid current DSL; use `profiles { profile NAME { ... } }`",
    )
}

fn legacy_defaults_keyword_error(span: Span) -> syn::Error {
    syn::Error::new(
        span,
        "`defaults` is not valid current DSL; use `default { ... }`",
    )
}

// Keep feature-domain macro chunks in separate files without widening helper visibility.
include!("auth.rs");
include!("endpoints.rs");
include!("rate_limit.rs");
include!("behavior.rs");
include!("items.rs");
include!("policy.rs");

#[cfg(test)]
mod tests;
