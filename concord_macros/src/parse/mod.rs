//! Parser for raw DSL syntax.
//!
//! This layer is allowed to know about removed syntax, but only to reject it
//! with v5 replacement diagnostics. It should not resolve inheritance or names.

use crate::ast::*;
use crate::kw;
use crate::model::{Scheme, SetOp};
use proc_macro2::{Span, TokenStream as TokenStream2, TokenTree};
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{
    Expr, Ident, LitBool, LitInt, LitStr, Path, Result, Token, Type, braced, bracketed,
    parenthesized, token,
};

impl Parse for ApiFile {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let span = input.span();
        let client: ClientDef = input.parse()?;
        let mut items = Vec::new();
        while !input.is_empty() {
            items.push(input.parse::<Item>()?);
        }
        Ok(Self {
            span,
            client,
            items,
        })
    }
}

impl Parse for ClientDef {
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
        let mut cache_profiles: Option<CacheProfilesBlock> = None;
        let mut cache: Option<CacheSpec> = None;
        let mut retry_profiles: Option<RetryProfilesBlock> = None;
        let mut retry: Option<RetrySpec> = None;
        let mut rate_limit: Option<RateLimitProfilesBlock> = None;
        let mut policy = PolicyBlocks::default();
        let mut seen_default_block = false;

        while !content.is_empty() {
            if content.peek(kw::base) {
                content.parse::<kw::base>()?;
                let v: Ident = content.parse()?;
                scheme = Some(match v.to_string().as_str() {
                    "http" => Scheme::Http,
                    "https" => Scheme::Https,
                    _ => {
                        return Err(syn::Error::new(
                            v.span(),
                            "base scheme must be `http` or `https`",
                        ));
                    }
                });
                host = Some(content.parse::<LitStr>()?);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::scheme) {
                return Err(legacy_v5_error(
                    content.span(),
                    "scheme:",
                    "base https \"example.com\"",
                ));
            } else if content.peek(kw::vars) {
                return Err(legacy_v5_error(
                    content.span(),
                    "vars {}",
                    "one `var name: Type` declaration per variable",
                ));
            } else if content.peek(kw::var) {
                content.parse::<kw::var>()?;
                let decl: VarDeclNoWire = content.parse()?;
                vars.get_or_insert_with(|| VarsBlock { decls: Vec::new() })
                    .decls
                    .push(decl);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::secret) {
                content.parse::<kw::secret>()?;
                if content.peek(token::Brace) {
                    return Err(syn::Error::new(
                        content.span(),
                        "`secret {}` was removed in v5; use one `secret name: Type` declaration per secret",
                    ));
                } else {
                    let decl: VarDeclNoWire = content.parse()?;
                    auth_vars
                        .get_or_insert_with(|| VarsBlock { decls: Vec::new() })
                        .decls
                        .push(decl);
                }
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::auth) {
                content.parse::<kw::auth>()?;
                if content.peek(token::Brace) {
                    return Err(syn::Error::new(
                        content.span(),
                        "`auth { credential ... }` was removed in v5; use `credential name = ...` in the client body",
                    ));
                }
                auth_uses.push(parse_auth_use_decl_after_auth_keyword(&content)?);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::credential) {
                content.parse::<kw::credential>()?;
                auth_credentials.push(parse_auth_credential_after_keyword(&content, true)?);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::use_auth) {
                return Err(legacy_v5_error(
                    content.span(),
                    "use_auth",
                    "auth header/query/bearer/basic/certificate ...",
                ));
            } else if content.peek(kw::response) {
                return Err(legacy_v5_error(
                    content.span(),
                    "response custom",
                    "observe rate_limit MyObserver",
                ));
            } else if content.peek(kw::cache) {
                content.parse::<kw::cache>()?;
                cache_profiles
                    .get_or_insert_with(|| CacheProfilesBlock {
                        profiles: Vec::new(),
                        default: None,
                    })
                    .profiles
                    .push(parse_cache_profile_decl_after_keyword(&content)?);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::retry) {
                content.parse::<kw::retry>()?;
                retry_profiles
                    .get_or_insert_with(|| RetryProfilesBlock {
                        profiles: Vec::new(),
                        default: None,
                    })
                    .profiles
                    .push(parse_retry_profile_decl_after_keyword(&content)?);
                let _ = content.parse::<Option<Token![,]>>()?;
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
            } else if content.peek(kw::host) {
                return Err(legacy_v5_error(
                    content.span(),
                    "host:",
                    "base https \"example.com\" for the client root or `host [...]` in scopes",
                ));
            } else if content.peek(kw::default) {
                content.parse::<kw::default>()?;
                if seen_default_block {
                    return Err(syn::Error::new(
                        content.span(),
                        "multiple default blocks are not allowed in v5",
                    ));
                }
                seen_default_block = true;
                let default_content;
                braced!(default_content in content);
                parse_client_default_block(
                    &default_content,
                    &mut policy,
                    &mut auth_uses,
                    &mut cache,
                    &mut retry,
                    &mut rate_limit,
                )?;
                let _ = content.parse::<Option<Token![,]>>()?;
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
                policy.headers = Some(content.parse::<PolicyBlockTaggedHeaders>()?.0);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::header) {
                policy
                    .headers
                    .get_or_insert_with(|| PolicyBlock { stmts: Vec::new() })
                    .stmts
                    .push(parse_inline_policy_stmt(
                        &content,
                        PolicyBlockKind::Headers,
                    )?);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::query) {
                if content.peek2(token::Brace) {
                    policy.query = Some(content.parse::<PolicyBlockTaggedQuery>()?.0);
                } else {
                    policy
                        .query
                        .get_or_insert_with(|| PolicyBlock { stmts: Vec::new() })
                        .stmts
                        .push(parse_inline_policy_stmt(&content, PolicyBlockKind::Query)?);
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
                "missing `base https \"example.com\"` in client",
            )
        })?;
        let host = host.ok_or_else(|| {
            syn::Error::new(
                name.span(),
                "missing `base https \"example.com\"` in client",
            )
        })?;

        Ok(Self {
            span,
            client_span,
            body_span,
            vars,
            auth_vars,
            auth: (!auth_credentials.is_empty()).then_some(AuthBlock {
                credentials: auth_credentials,
            }),
            auth_uses,
            cache_profiles,
            cache,
            name,
            scheme,
            host,
            policy,
            retry_profiles,
            retry,
            rate_limit,
        })
    }
}

fn parse_client_default_block(
    input: ParseStream<'_>,
    policy: &mut PolicyBlocks,
    auth_uses: &mut Vec<AuthUseDecl>,
    cache: &mut Option<CacheSpec>,
    retry: &mut Option<RetrySpec>,
    rate_limit: &mut Option<RateLimitProfilesBlock>,
) -> Result<()> {
    while !input.is_empty() {
        if input.peek(kw::headers) {
            policy.headers = Some(input.parse::<PolicyBlockTaggedHeaders>()?.0);
        } else if input.peek(kw::header) {
            policy
                .headers
                .get_or_insert_with(|| PolicyBlock { stmts: Vec::new() })
                .stmts
                .push(parse_inline_policy_stmt(input, PolicyBlockKind::Headers)?);
        } else if input.peek(kw::query) {
            if input.peek2(token::Brace) {
                policy.query = Some(input.parse::<PolicyBlockTaggedQuery>()?.0);
            } else {
                policy
                    .query
                    .get_or_insert_with(|| PolicyBlock { stmts: Vec::new() })
                    .stmts
                    .push(parse_inline_policy_stmt(input, PolicyBlockKind::Query)?);
            }
        } else if input.peek(kw::timeout) {
            input.parse::<kw::timeout>()?;
            if input.peek(Token![:]) {
                input.parse::<Token![:]>()?;
            }
            policy.timeout = Some(normalize_policy_expr(input.parse::<Expr>()?));
        } else if input.peek(kw::auth) {
            input.parse::<kw::auth>()?;
            auth_uses.push(parse_auth_use_decl_after_auth_keyword(input)?);
        } else if input.peek(kw::cache) {
            if cache.is_some() {
                return Err(syn::Error::new(
                    input.span(),
                    "duplicate default cache policy",
                ));
            }
            match parse_cache_decl(input)? {
                CacheDecl::Spec(spec) => *cache = Some(spec),
            }
        } else if input.peek(kw::retry) {
            if retry.is_some() {
                return Err(syn::Error::new(
                    input.span(),
                    "duplicate default retry policy",
                ));
            }
            match parse_retry_decl(input)? {
                RetryDecl::Spec(spec) => *retry = Some(spec),
            }
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
        } else if input.peek(kw::use_auth) {
            return Err(legacy_v5_error(input.span(), "use_auth", "auth ..."));
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

// Keep feature-domain macro chunks in separate files without widening helper visibility.
include!("legacy.rs");
include!("auth.rs");
include!("endpoints.rs");
include!("retry.rs");
include!("cache.rs");
include!("rate_limit.rs");
include!("items.rs");
include!("policy.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_v5_api_into_raw_ast_with_endpoint_line_metadata() {
        let ast: RawApi = syn::parse_str(
            r#"
            client Api {
                base https "example.com"
                retry read {
                    max_attempts 2
                    methods [GET]
                }
            }

            scope users(id: u64) {
                path ["users", id]

                GET Show
                    as show
                    path ["profile"]
                    -> Json<String>
                {
                    query {
                        id
                    }
                }
            }
            "#,
        )
        .expect("v5 raw syntax parses");

        assert_eq!(ast.client.name, "Api");
        assert_eq!(ast.items.len(), 1);
        let Item::Layer(scope) = &ast.items[0] else {
            panic!("expected scope");
        };
        assert_eq!(
            scope
                .scope_name
                .as_ref()
                .map(ToString::to_string)
                .as_deref(),
            Some("users")
        );
        assert_eq!(scope.items.len(), 1);
        let Item::Endpoint(endpoint) = &scope.items[0] else {
            panic!("expected endpoint");
        };
        assert_eq!(endpoint.line.method, "GET");
        assert_eq!(endpoint.line.name, "Show");
        assert_eq!(
            endpoint
                .line
                .alias
                .as_ref()
                .map(ToString::to_string)
                .as_deref(),
            Some("show")
        );
        assert!(endpoint.policy.query.is_some());
    }
}
