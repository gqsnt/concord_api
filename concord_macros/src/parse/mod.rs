//! Parser for raw DSL syntax.
//!
//! This layer accepts current syntax only. It should not resolve inheritance
//! or names.

use crate::ast::*;
use crate::emit_helpers;
use crate::kw;
use crate::model::{Scheme, SetOp};
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
        let mut retry_profiles: Option<RetryProfilesBlock> = None;
        let mut retry: Option<RetrySpec> = None;
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
            } else if content.peek(kw::policies) {
                content.parse::<kw::policies>()?;
                let policy_content;
                braced!(policy_content in content);
                parse_client_policies_group(&policy_content, &mut retry_profiles, &mut rate_limit)?;
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::behavior) {
                content.parse::<kw::behavior>()?;
                behavior_profiles
                    .get_or_insert_with(|| BehaviorProfilesBlock {
                        profiles: Vec::new(),
                    })
                    .profiles
                    .push(parse_behavior_profile_decl_after_keyword(&content)?);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::behaviors) {
                content.parse::<kw::behaviors>()?;
                let behavior_content;
                braced!(behavior_content in content);
                while !behavior_content.is_empty() {
                    if !behavior_content.peek(kw::behavior) {
                        let tt: TokenTree = behavior_content.parse()?;
                        return Err(syn::Error::new(
                            tt.span(),
                            "invalid item in behaviors block; expected behavior profile",
                        ));
                    }
                    behavior_content.parse::<kw::behavior>()?;
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
            } else if content.peek(kw::default) || content.peek(kw::defaults) {
                let default_span = if content.peek(kw::default) {
                    content.parse::<kw::default>()?.span
                } else {
                    content.parse::<kw::defaults>()?.span
                };
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
            retry_profiles,
            retry,
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
    retry_profiles: &mut Option<RetryProfilesBlock>,
    rate_limit: &mut Option<RateLimitProfilesBlock>,
) -> Result<()> {
    while !input.is_empty() {
        if input.peek(kw::retry) {
            let fork = input.fork();
            fork.parse::<kw::retry>()?;
            if fork.peek(kw::off) {
                return Err(syn::Error::new(
                    fork.span(),
                    "default retry policy is not allowed in policies block; use defaults { ... } or default { ... }",
                ));
            }
            if !fork.peek(Ident) {
                let tt: TokenTree = input.parse()?;
                return Err(syn::Error::new(
                    tt.span(),
                    "invalid item in policies block; expected retry, rate_limit, or observe",
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
                    "default retry policy is not allowed in policies block; use defaults { ... } or default { ... }",
                ));
            }

            input.parse::<kw::retry>()?;
            retry_profiles
                .get_or_insert_with(|| RetryProfilesBlock {
                    profiles: Vec::new(),
                    default: None,
                })
                .profiles
                .push(parse_retry_profile_decl_after_keyword(input)?);
        } else if input.peek(kw::rate_limit) {
            let fork = input.fork();
            fork.parse::<kw::rate_limit>()?;
            if fork.peek(kw::off) || fork.peek(kw::only) {
                return Err(syn::Error::new(
                    fork.span(),
                    "default rate_limit policy is not allowed in policies block; use defaults { ... } or default { ... }",
                ));
            }
            if !fork.peek(Ident) {
                let tt: TokenTree = input.parse()?;
                return Err(syn::Error::new(
                    tt.span(),
                    "invalid item in policies block; expected retry, rate_limit, or observe",
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
                    "default rate_limit policy is not allowed in policies block; use defaults { ... } or default { ... }",
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
                "invalid item in policies block; expected retry, rate_limit, or observe",
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
    retry: &mut Option<RetrySpec>,
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
        } else if input.peek(kw::behavior) {
            default_behavior_uses.push(parse_behavior_use_spec(input)?);
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
include!("auth.rs");
include!("endpoints.rs");
include!("retry.rs");
include!("rate_limit.rs");
include!("behavior.rs");
include!("items.rs");
include!("policy.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_current_api_into_raw_ast_with_endpoint_line_metadata() {
        let ast: RawApi = syn::parse_str(
            r#"
            client Api {
                base "https://example.com"
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
                    query {
                        id
                    }
                    -> Json<String>
            }
            "#,
        )
        .expect("current raw syntax parses");

        assert_eq!(ast.client.name, "Api");
        assert_eq!(ast.items.len(), 1);
        let RawItem::Layer(scope) = &ast.items[0] else {
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
        let RawItem::Endpoint(endpoint) = &scope.items[0] else {
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

    #[test]
    fn parses_custom_paginate_syntax() {
        let ast: RawApi = syn::parse_str(
            r#"
            client Api {
                base "https://example.com"
            }

            GET List
                path ["items"]
                paginate HeaderPagePagination
                -> Json<Vec<String>>
            "#,
        )
        .expect("custom paginate syntax parses");

        let RawItem::Endpoint(endpoint) = &ast.items[0] else {
            panic!("expected endpoint");
        };
        let paginate = endpoint.paginate.as_ref().expect("paginate");
        let ctrl_ty = &paginate.ctrl_ty;
        assert_eq!(quote::quote!(#ctrl_ty).to_string(), "HeaderPagePagination");
        assert!(paginate.assigns.is_empty());
    }

    #[test]
    fn parses_custom_paginate_syntax_with_assignments() {
        let ast: RawApi = syn::parse_str(
            r#"
            client Api {
                base "https://example.com"
            }

            GET List(page: u64 = 1, count: u64 = 2)
                path ["items"]
                paginate HeaderPagePagination {
                    page = page,
                    count = count
                }
                -> Json<Vec<String>>
            "#,
        )
        .expect("custom paginate syntax parses");

        let RawItem::Endpoint(endpoint) = &ast.items[0] else {
            panic!("expected endpoint");
        };
        let paginate = endpoint.paginate.as_ref().expect("paginate");
        let ctrl_ty = &paginate.ctrl_ty;
        assert_eq!(quote::quote!(#ctrl_ty).to_string(), "HeaderPagePagination");
        assert_eq!(paginate.assigns.len(), 2);
        assert_eq!(paginate.assigns[0].key.to_string(), "page");
        assert_eq!(paginate.assigns[1].key.to_string(), "count");
    }

    #[test]
    fn custom_pagination_single_object_syntax_resolves() {
        parses_custom_paginate_syntax_with_assignments();
    }

    #[test]
    fn parses_cursor_paginate_with_explicit_string_type() {
        let ast: RawApi = syn::parse_str(
            r#"
            client Api {
                base "https://example.com"
            }

            GET List(cursor?: String, count: u64 = 2)
                path ["items"]
                paginate CursorPagination<String> {
                    cursor = cursor,
                    per_page = count,
                    send_cursor_on_first = true,
                    stop_when_cursor_missing = false
                }
                -> Json<Vec<String>>
            "#,
        )
        .expect("cursor paginate syntax parses");

        let RawItem::Endpoint(endpoint) = &ast.items[0] else {
            panic!("expected endpoint");
        };
        let paginate = endpoint.paginate.as_ref().expect("paginate");
        let ctrl_ty = &paginate.ctrl_ty;
        assert_eq!(
            quote::quote!(#ctrl_ty).to_string(),
            "CursorPagination < String >"
        );
        assert_eq!(paginate.assigns.len(), 4);
    }

    #[test]
    fn parses_scope_with_host_and_path_preserves_raw_shape() {
        let ast: RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                }

                scope tenant(tenant_id: String) {
                    host [fmt["tenant-", tenant_id], "api"]
                    path ["v1"]

                    GET Ping
                        path ["ping"]
                        -> Json<String>
                }
            }
            "#,
        )
        .expect("scope with host and path parses");

        assert_eq!(ast.items.len(), 1);
        let RawItem::Layer(scope) = &ast.items[0] else {
            panic!("expected scope");
        };
        assert!(scope.host_route.is_some());
        assert!(scope.path_route.is_some());
        assert_eq!(scope.items.len(), 1);
        let RawItem::Endpoint(endpoint) = &scope.items[0] else {
            panic!("expected direct endpoint child");
        };
        assert_eq!(endpoint.line.name, "Ping");
    }

    #[test]
    fn parses_current_api_wrapper_and_base_url_literal() {
        let ast: RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                }

                GET Ping
                    path ["ping"]
                    -> Json<String>
            }
            "#,
        )
        .expect("current wrapped api syntax parses");

        assert_eq!(ast.client.name, "Api");
        assert_eq!(ast.client.host.value(), "example.com");
        assert_eq!(ast.items.len(), 1);
    }

    #[test]
    fn parses_current_nested_scopes() {
        let ast: RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                }

                scope org(org_id: u64) {
                    path ["orgs", org_id]

                    scope users {
                        path ["users"]

                        GET List
                            path ["list"]
                            -> Json<Vec<String>>
                    }
                }
            }
            "#,
        )
        .expect("nested current scopes parse");

        assert_eq!(ast.items.len(), 1);
    }

    #[test]
    fn parses_current_policy_profiles() {
        let ast: RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"

                    default {
                        retry read
                        rate_limit app
                    }

                    retry read {
                        max_attempts 2
                        methods [GET]
                    }

                    rate_limit app {
                        bucket application by [host] {
                            10 / 1s
                        }
                    }
                }
            }
            "#,
        )
        .expect("current policy profiles parse");

        assert!(ast.client.retry_profiles.is_some());
        assert!(ast.client.rate_limit.is_some());
    }

    #[test]
    fn parses_grouped_policy_profiles() {
        let ast: RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"

                    policies {
                        retry read {
                            max_attempts 2
                            methods [GET]
                        }

                        rate_limit app {
                            bucket application by [host] {
                                10 / 1s
                            }
                        }

                        observe rate_limit ExampleObserver
                    }
                }
            }
            "#,
        )
        .expect("grouped policy profiles parse");

        assert!(ast.client.retry_profiles.is_some());
        assert!(ast.client.rate_limit.is_some());
    }

    #[test]
    fn malformed_current_client_fails() {
        let err = syn::parse_str::<RawApi>(
            r#"
            api! {
                client Api {
                    base "ftp://example.com"
                }
            }
            "#,
        )
        .expect_err("invalid base URL scheme must fail");

        assert!(err.to_string().contains("base URL must start"));
    }

    #[test]
    fn endpoint_clauses_before_and_after_response_parse() {
        let ast: RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                }

                GET Search(q: String, page?: u32, count: u32 = 20)
                    path ["search"]
                    -> Json<String>
                    query {
                        q
                        page
                        count
                    }
                    timeout 10
            }
            "#,
        )
        .expect("endpoint clauses before and after response parse");

        let RawItem::Endpoint(endpoint) = &ast.items[0] else {
            panic!("expected endpoint");
        };
        assert_eq!(endpoint.params.len(), 3);
        assert!(endpoint.policy.query.is_some());
        assert!(endpoint.policy.timeout.is_some());
    }

    #[test]
    fn endpoint_response_after_response_parses() {
        let ast: RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                }

                POST Login(body: Json<LoginResponse>)
                    path ["login"]
                    -> Json<LoginResponse>
            }
            "#,
        )
        .expect("response after response marker parses");

        let RawItem::Endpoint(endpoint) = &ast.items[0] else {
            panic!("expected endpoint");
        };
        assert!(endpoint.body.is_some());
        assert_eq!(
            endpoint.response.marker,
            syn::parse_quote!(Json<LoginResponse>)
        );
        assert!(endpoint.response.had_angle_args);
    }

    #[test]
    fn endpoint_missing_response_fails() {
        let err = syn::parse_str::<RawApi>(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                }

                GET Ping
                    path ["ping"]
            }
            "#,
        )
        .expect_err("missing endpoint response must fail");

        assert!(err.to_string().contains("endpoint declarations must use"));
    }

    #[test]
    fn endpoint_duplicate_response_fails() {
        let err = syn::parse_str::<RawApi>(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                }

                GET Ping
                    path ["ping"]
                    -> Json<String>
                    -> Json<String>
            }
            "#,
        )
        .expect_err("duplicate response must fail");

        assert!(err.to_string().contains("duplicate endpoint response"));
    }

    #[test]
    fn endpoint_braced_block_fails() {
        let err = syn::parse_str::<RawApi>(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                }

                GET Ping
                    -> Json<String>
                    {
                        path ["ping"]
                    }
            }
            "#,
        )
        .expect_err("endpoint braced block must fail");

        assert!(err.to_string().contains("DSL-002"));
    }

    #[test]
    fn endpoint_unknown_clause_fails_with_code() {
        let err = syn::parse_str::<RawApi>(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                }

                GET Ping
                    frobnicate true
                    -> Json<String>
            }
            "#,
        )
        .expect_err("unknown endpoint clause must fail");

        assert!(err.to_string().contains("DSL-001"));
    }

    #[test]
    fn fmt_passes_in_host_path_query_and_header_contexts() {
        let ast: RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                    var trace_id: String
                }

                scope tenant(tenant_id: String) {
                    host [fmt["tenant-", tenant_id], "api"]
                    path [fmt["tenant-", tenant_id]]

                    GET Search(q: String)
                        path ["search"]
                        headers {
                            "x-trace" = fmt["trace-", vars.trace_id]
                        }
                        query {
                            "q" = fmt["prefix:", q]
                        }
                        -> Json<String>
                }
            }
            "#,
        )
        .expect("fmt should parse in all supported contexts");

        assert_eq!(ast.items.len(), 1);
    }

    #[test]
    fn fmt_empty_fails() {
        let err = syn::parse_str::<RawApi>(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                }

                GET Ping
                    path [fmt[]]
                    -> Json<String>
            }
            "#,
        )
        .expect_err("empty fmt should fail");

        assert!(
            err.to_string()
                .contains("fmt[...] requires at least one piece")
        );
    }

    #[test]
    fn fmt_nested_fails() {
        let err = syn::parse_str::<RawApi>(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                }

                GET Ping(id: String)
                    path [fmt["x", fmt[id]]]
                    -> Json<String>
            }
            "#,
        )
        .expect_err("nested fmt should fail");

        assert!(err.to_string().contains("nested fmt"));
    }

    #[test]
    fn fmt_path_slash_fails() {
        let err = syn::parse_str::<RawApi>(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                }

                GET Ping(id: String)
                    path [fmt["users/", id]]
                    -> Json<String>
            }
            "#,
        )
        .expect_err("slash inside path fmt should fail");

        assert!(err.to_string().contains("must not contain `/`"));
    }

    #[test]
    fn fmt_secret_ref_fails() {
        let err = syn::parse_str::<RawApi>(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                    secret api_key: String
                }

                GET Ping
                    path ["ping"]
                    query {
                        "key" = fmt["secret-", secret.api_key]
                    }
                    -> Json<String>
            }
            "#,
        )
        .expect_err("secret refs inside fmt should fail");

        assert!(err.to_string().contains("secret.* is not allowed"));
    }

    #[test]
    fn query_and_header_policy_operations_parse_in_order() {
        let ast: RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                }

                GET Search(q: String, cursor?: String, trace_id: String)
                    path ["search"]
                    query {
                        q
                        "cursor" = cursor
                        "tag" += q,
                        -"old"
                    }
                    headers {
                        "x-trace" = trace_id,
                        -"x-old"
                    }
                    -> Json<String>
            }
            "#,
        )
        .expect("query/header operations parse");

        let RawItem::Endpoint(endpoint) = &ast.items[0] else {
            panic!("expected endpoint");
        };
        assert_eq!(endpoint.policy.query.as_ref().unwrap().stmts.len(), 4);
        assert_eq!(endpoint.policy.headers.as_ref().unwrap().stmts.len(), 2);

        let query = endpoint.policy.query.as_ref().unwrap();
        match &query.stmts[0] {
            PolicyStmt::Set {
                key: KeySpec::Ident(key),
                value: PolicyValue::Expr(Expr::Path(path)),
                op: SetOp::Set,
            } => {
                assert_eq!(key.to_string(), "q");
                assert_eq!(path.path.segments.len(), 1);
                assert_eq!(path.path.segments[0].ident, "q");
            }
            other => panic!("query shorthand should remain raw syntax: {other:?}"),
        }
    }

    #[test]
    fn policy_values_parse_raw_secret_references() {
        let ast: RawApi = syn::parse_str(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                    secret token: String
                }

                GET HeaderRef
                    path ["header"]
                    headers {
                        "X-Token" = secret.token
                    }
                    -> Json<String>
            }
            "#,
        )
        .expect("secret references parse as raw syntax");

        let RawItem::Endpoint(endpoint) = &ast.items[0] else {
            panic!("expected endpoint");
        };
        let header = endpoint.policy.headers.as_ref().expect("headers parsed");
        match &header.stmts[0] {
            PolicyStmt::Set {
                value: PolicyValue::Expr(Expr::Field(field)),
                ..
            } => match &*field.base {
                Expr::Path(path) => {
                    assert_eq!(path.path.segments[0].ident, "secret");
                }
                other => panic!("expected raw secret path, got {other:?}"),
            },
            other => panic!("expected raw secret expression, got {other:?}"),
        }
    }

    #[test]
    fn header_identifier_key_fails() {
        let err = syn::parse_str::<RawApi>(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                }

                GET Search(trace_id: String)
                    path ["search"]
                    headers {
                        x_trace = trace_id
                    }
                    -> Json<String>
            }
            "#,
        )
        .expect_err("identifier header keys must fail");

        assert!(
            err.to_string()
                .contains("header keys must be explicit string literals")
        );
    }

    #[test]
    fn boolean_query_flag_fails() {
        let err = syn::parse_str::<RawApi>(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                }

                GET Search
                    path ["search"]
                    query {
                        "debug" = true
                    }
                    -> Json<String>
            }
            "#,
        )
        .expect_err("boolean query flags must fail");

        assert!(
            err.to_string()
                .contains("boolean query flags are not supported")
        );
    }

    #[test]
    fn unsupported_part_syntax_fails() {
        let err = syn::parse_str::<RawApi>(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                }

                GET Ping
                    part["ping"]
                    -> Json<String>
            }
            "#,
        )
        .expect_err("part syntax must fail");

        assert!(err.to_string().contains("`part[...]` is not supported"));
    }

    #[test]
    fn unsupported_attempts_retry_field_fails() {
        let err = syn::parse_str::<RawApi>(
            r#"
            api! {
                client Api {
                    base "https://example.com"

                    retry read {
                        attempts 2
                    }
                }

                GET Ping
                    path ["ping"]
                    retry read
                    -> Json<String>
            }
            "#,
        )
        .expect_err("attempts syntax must fail");

        assert!(err.to_string().contains("`attempts` is not supported"));
    }

    #[test]
    fn unsupported_body_stanza_fails() {
        let err = syn::parse_str::<RawApi>(
            r#"
            api! {
                client Api {
                    base "https://example.com"
                }

                POST Create
                    path ["items"]
                    body Json<String>
                    -> Json<String>
            }
            "#,
        )
        .expect_err("body stanza syntax must fail");

        assert!(
            err.to_string()
                .contains("body stanza lines are not supported")
        );
    }

    #[test]
    fn unsupported_auth_combinators_fail() {
        for auth in ["none", "any", "all"] {
            let source = format!(
                r#"
                api! {{
                    client Api {{
                        base "https://example.com"
                    }}

                    GET Ping
                        path ["ping"]
                        auth {auth}
                        -> Json<String>
                }}
                "#
            );
            let err = syn::parse_str::<RawApi>(&source).expect_err("auth combinator must fail");
            assert!(
                err.to_string()
                    .contains("auth none/any/all are not supported")
            );
        }
    }
}
