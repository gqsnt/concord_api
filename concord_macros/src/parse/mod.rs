// concord_macros/src/parse.rs
use crate::ast::*;
use crate::kw;
use proc_macro2::{Span, TokenStream as TokenStream2, TokenTree};
use syn::parse::{Parse, ParseStream};
use syn::{
    Expr, Ident, LitBool, LitInt, LitStr, Path, Result, Token, Type, braced, bracketed,
    parenthesized, token,
};

impl Parse for ApiFile {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let client: ClientDef = input.parse()?;
        let mut items = Vec::new();
        while !input.is_empty() {
            items.push(input.parse::<Item>()?);
        }
        Ok(Self { client, items })
    }
}

impl Parse for ClientDef {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        input.parse::<kw::client>()?;
        let name: Ident = input.parse()?;

        let content;
        braced!(content in input);

        let mut scheme: Option<SchemeLit> = None;
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

        while !content.is_empty() {
            if content.peek(kw::base) {
                content.parse::<kw::base>()?;
                let v: Ident = content.parse()?;
                scheme = Some(match v.to_string().as_str() {
                    "http" => SchemeLit::Http,
                    "https" => SchemeLit::Https,
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
                content.parse::<kw::scheme>()?;
                content.parse::<Token![:]>()?;
                let v: Ident = content.parse()?;
                scheme = Some(match v.to_string().as_str() {
                    "http" => SchemeLit::Http,
                    "https" => SchemeLit::Https,
                    _ => {
                        return Err(syn::Error::new(
                            v.span(),
                            "scheme must be `http` or `https`",
                        ));
                    }
                });
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::vars) {
                if vars.is_some() {
                    return Err(syn::Error::new(
                        name.span(),
                        "duplicate `vars {}` in client",
                    ));
                }
                vars = Some(content.parse::<VarsBlockTaggedVars>()?.0);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::secret) {
                content.parse::<kw::secret>()?;
                if content.peek(token::Brace) {
                    if auth_vars.is_some() {
                        return Err(syn::Error::new(
                            name.span(),
                            "duplicate `secret {}` in client",
                        ));
                    }
                    auth_vars = Some(parse_vars_block(&content)?);
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
                if !content.peek(token::Brace) {
                    auth_uses.push(parse_auth_use_decl_after_auth_keyword(&content)?);
                    let _ = content.parse::<Option<Token![,]>>()?;
                    continue;
                }
                if !auth_credentials.is_empty() {
                    return Err(syn::Error::new(
                        name.span(),
                        "duplicate `auth {}` in client",
                    ));
                }
                let block = parse_auth_block_after_keyword(&content)?;
                auth_credentials.extend(block.credentials);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::credential) {
                content.parse::<kw::credential>()?;
                auth_credentials.push(parse_auth_credential_after_keyword(&content, true)?);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::use_auth) {
                auth_uses.push(content.parse::<AuthUseDecl>()?);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::cache) {
                match parse_cache_decl(&content)? {
                    CacheDecl::Profiles(block) => {
                        if cache_profiles.is_some() {
                            return Err(syn::Error::new(
                                name.span(),
                                "duplicate cache profile block in client",
                            ));
                        }
                        cache_profiles = Some(block);
                    }
                    CacheDecl::Spec(spec) => {
                        if cache.is_some() {
                            return Err(syn::Error::new(
                                name.span(),
                                "duplicate client cache policy",
                            ));
                        }
                        cache = Some(spec);
                    }
                }
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::retry) {
                match parse_retry_decl(&content)? {
                    RetryDecl::Profiles(block) => {
                        if retry_profiles.is_some() {
                            return Err(syn::Error::new(
                                name.span(),
                                "duplicate retry profile block in client",
                            ));
                        }
                        retry_profiles = Some(block);
                    }
                    RetryDecl::Spec(spec) => {
                        if retry.is_some() {
                            return Err(syn::Error::new(
                                name.span(),
                                "duplicate client retry policy",
                            ));
                        }
                        retry = Some(spec);
                    }
                }
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::rate_limit) {
                if rate_limit.is_some() {
                    return Err(syn::Error::new(
                        name.span(),
                        "duplicate rate_limit block in client",
                    ));
                }
                rate_limit = Some(parse_rate_limit_profiles_decl(&content)?);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::host) {
                content.parse::<kw::host>()?;
                content.parse::<Token![:]>()?;
                host = Some(content.parse::<LitStr>()?);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::headers) {
                policy.headers = Some(content.parse::<PolicyBlockTaggedHeaders>()?.0);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::header) {
                policy
                    .headers
                    .get_or_insert_with(|| PolicyBlock { stmts: Vec::new() })
                    .stmts
                    .push(parse_v3_single_policy_stmt(
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
                        .push(parse_v3_single_policy_stmt(
                            &content,
                            PolicyBlockKind::Query,
                        )?);
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

        let scheme =
            scheme.ok_or_else(|| syn::Error::new(name.span(), "missing `scheme:` in client"))?;
        let host = host.ok_or_else(|| syn::Error::new(name.span(), "missing `host:` in client"))?;

        Ok(Self {
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

// Keep feature-domain macro chunks in separate files without widening helper visibility.
include!("auth.rs");
include!("endpoints.rs");
include!("retry.rs");
include!("cache.rs");
include!("rate_limit.rs");
include!("items.rs");
include!("policy.rs");
