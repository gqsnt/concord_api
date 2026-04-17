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
        let mut auth: Option<AuthBlock> = None;
        let mut auth_uses: Vec<AuthUseDecl> = Vec::new();
        let mut cache_profiles: Option<CacheProfilesBlock> = None;
        let mut cache: Option<CacheSpec> = None;
        let mut retry_profiles: Option<RetryProfilesBlock> = None;
        let mut retry: Option<RetrySpec> = None;
        let mut rate_limit: Option<RateLimitProfilesBlock> = None;
        let mut policy = PolicyBlocks::default();

        while !content.is_empty() {
            if content.peek(kw::scheme) {
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
            } else if content.peek(kw::auth_vars) {
                if auth_vars.is_some() {
                    return Err(syn::Error::new(
                        name.span(),
                        "duplicate `auth_vars {}` in client",
                    ));
                }
                auth_vars = Some(content.parse::<VarsBlockTaggedAuthVars>()?.0);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::secret) {
                if auth_vars.is_some() {
                    return Err(syn::Error::new(
                        name.span(),
                        "duplicate `secret {}`/`auth_vars {}` in client",
                    ));
                }
                auth_vars = Some(content.parse::<VarsBlockTaggedSecret>()?.0);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::auth) {
                if auth.is_some() {
                    return Err(syn::Error::new(
                        name.span(),
                        "duplicate `auth {}` in client",
                    ));
                }
                auth = Some(content.parse::<AuthBlockTagged>()?.0);
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
            } else if content.peek(kw::query) {
                policy.query = Some(content.parse::<PolicyBlockTaggedQuery>()?.0);
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
            auth,
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

struct VarsBlockTaggedVars(VarsBlock);
struct VarsBlockTaggedAuthVars(VarsBlock);
struct VarsBlockTaggedSecret(VarsBlock);
impl Parse for VarsBlockTaggedVars {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        input.parse::<kw::vars>()?;
        Ok(Self(parse_vars_block(input)?))
    }
}
impl Parse for VarsBlockTaggedAuthVars {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        input.parse::<kw::auth_vars>()?;
        Ok(Self(parse_vars_block(input)?))
    }
}
impl Parse for VarsBlockTaggedSecret {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        input.parse::<kw::secret>()?;
        Ok(Self(parse_vars_block(input)?))
    }
}

struct AuthBlockTagged(AuthBlock);

impl Parse for AuthBlockTagged {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        input.parse::<kw::auth>()?;
        let content;
        braced!(content in input);
        let mut credentials = Vec::new();
        while !content.is_empty() {
            credentials.push(content.parse::<AuthCredentialDecl>()?);
            let _ = content.parse::<Option<Token![,]>>()?;
        }
        Ok(Self(AuthBlock { credentials }))
    }
}

impl Parse for AuthCredentialDecl {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        input.parse::<kw::credential>()?;
        let name: Ident = input.parse()?;
        input.parse::<Token![:]>()?;

        let kind_name: Ident = input.parse()?;
        let kind = match kind_name.to_string().as_str() {
            "ApiKey" => {
                let content;
                parenthesized!(content in input);
                let secret = parse_secret_ref(&content)?;
                if !content.is_empty() {
                    return Err(syn::Error::new(
                        content.span(),
                        "unexpected ApiKey arguments",
                    ));
                }
                AuthCredentialKind::ApiKey { secret }
            }
            "BearerToken" | "AccessToken" => {
                let content;
                parenthesized!(content in input);
                let secret = parse_secret_ref(&content)?;
                if !content.is_empty() {
                    return Err(syn::Error::new(
                        content.span(),
                        "unexpected bearer token arguments",
                    ));
                }
                AuthCredentialKind::StaticBearer { secret }
            }
            "Basic" => {
                let content;
                parenthesized!(content in input);
                let username = parse_secret_ref(&content)?;
                content.parse::<Token![,]>()?;
                let password = parse_secret_ref(&content)?;
                if !content.is_empty() {
                    return Err(syn::Error::new(
                        content.span(),
                        "unexpected Basic arguments",
                    ));
                }
                AuthCredentialKind::Basic { username, password }
            }
            "OAuth2ClientCredentials" => {
                parse_oauth2_client_credentials(input, kind_name.span())?.into()
            }
            "Endpoint" => {
                let content;
                parenthesized!(content in input);
                let endpoint: Ident = content.parse()?;
                if !content.is_empty() {
                    return Err(syn::Error::new(
                        content.span(),
                        "unexpected Endpoint arguments",
                    ));
                }
                AuthCredentialKind::Endpoint { endpoint }
            }
            "Custom" => {
                let provider_ty = parse_angle_type(input, kind_name.span(), "custom provider")?;
                let content;
                parenthesized!(content in input);
                let provider: Expr = content.parse()?;
                if !content.is_empty() {
                    return Err(syn::Error::new(
                        content.span(),
                        "unexpected Custom provider arguments",
                    ));
                }
                AuthCredentialKind::Custom {
                    provider_ty,
                    provider,
                }
            }
            _ => {
                return Err(syn::Error::new(
                    kind_name.span(),
                    "unknown auth credential kind; expected ApiKey, BearerToken, AccessToken, Basic, OAuth2ClientCredentials, Endpoint, or Custom<T>",
                ));
            }
        };

        Ok(Self { name, kind })
    }
}

struct OAuth2ClientCredentialsFields {
    token_url: LitStr,
    client_id: SecretRef,
    client_secret: SecretRef,
    scope: Option<LitStr>,
}

fn parse_oauth2_client_credentials(
    input: ParseStream<'_>,
    span: Span,
) -> Result<OAuth2ClientCredentialsFields> {
    let content;
    braced!(content in input);

    let mut token_url: Option<LitStr> = None;
    let mut client_id: Option<SecretRef> = None;
    let mut client_secret: Option<SecretRef> = None;
    let mut scope: Option<LitStr> = None;

    while !content.is_empty() {
        let key: Ident = content.parse()?;
        content.parse::<Token![:]>()?;
        match key.to_string().as_str() {
            "token_url" => set_once_lit(&mut token_url, key.span(), content.parse()?)?,
            "client_id" => {
                set_once_secret_ref(&mut client_id, key.span(), parse_secret_ref(&content)?)?
            }
            "client_secret" => {
                set_once_secret_ref(&mut client_secret, key.span(), parse_secret_ref(&content)?)?
            }
            "scope" => set_once_lit(&mut scope, key.span(), content.parse()?)?,
            _ => {
                return Err(syn::Error::new(
                    key.span(),
                    "unknown OAuth2ClientCredentials field; expected token_url, client_id, client_secret, or scope",
                ));
            }
        }

        if content.peek(Token![,]) {
            content.parse::<Token![,]>()?;
        } else if !content.is_empty() {
            let tt: TokenTree = content.parse()?;
            return Err(syn::Error::new(
                tt.span(),
                "expected `,` between OAuth2ClientCredentials fields",
            ));
        }
    }

    Ok(OAuth2ClientCredentialsFields {
        token_url: token_url
            .ok_or_else(|| syn::Error::new(span, "OAuth2ClientCredentials missing `token_url`"))?,
        client_id: client_id
            .ok_or_else(|| syn::Error::new(span, "OAuth2ClientCredentials missing `client_id`"))?,
        client_secret: client_secret.ok_or_else(|| {
            syn::Error::new(span, "OAuth2ClientCredentials missing `client_secret`")
        })?,
        scope,
    })
}

impl From<OAuth2ClientCredentialsFields> for AuthCredentialKind {
    fn from(v: OAuth2ClientCredentialsFields) -> Self {
        AuthCredentialKind::OAuth2ClientCredentials {
            token_url: v.token_url,
            client_id: v.client_id,
            client_secret: v.client_secret,
            scope: v.scope,
        }
    }
}

fn set_once_lit(out: &mut Option<LitStr>, span: Span, value: LitStr) -> Result<()> {
    if out.is_some() {
        return Err(syn::Error::new(span, "duplicate auth field"));
    }
    *out = Some(value);
    Ok(())
}

fn set_once_secret_ref(out: &mut Option<SecretRef>, span: Span, value: SecretRef) -> Result<()> {
    if out.is_some() {
        return Err(syn::Error::new(span, "duplicate auth field"));
    }
    *out = Some(value);
    Ok(())
}

fn parse_secret_ref(input: ParseStream<'_>) -> Result<SecretRef> {
    let ns: Ident = input.parse()?;
    if ns != "secret" && ns != "auth" {
        return Err(syn::Error::new(
            ns.span(),
            "auth credentials must reference secrets as `secret.name`",
        ));
    }
    input.parse::<Token![.]>()?;
    let ident: Ident = input.parse()?;
    Ok(SecretRef { ident })
}

fn parse_angle_type(input: ParseStream<'_>, span: Span, label: &'static str) -> Result<Type> {
    if !input.peek(Token![<]) {
        return Err(syn::Error::new(
            span,
            format!("expected `{label}` type parameter, e.g. Custom<MyProvider>(...)"),
        ));
    }
    input.parse::<Token![<]>()?;
    let ty: Type = input.parse()?;
    input.parse::<Token![>]>()?;
    Ok(ty)
}

impl Parse for AuthUseDecl {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        input.parse::<kw::use_auth>()?;

        if input.peek(kw::one_of) {
            input.parse::<kw::one_of>()?;
            return Ok(AuthUseDecl::OneOf(parse_auth_use_kinds_list(input)?));
        }

        if input.peek(token::Bracket) {
            return Ok(AuthUseDecl::AllOf(parse_auth_use_kinds_list(input)?));
        }

        Ok(AuthUseDecl::Single(parse_auth_use_kind(input)?))
    }
}

fn parse_auth_use_kinds_list(input: ParseStream<'_>) -> Result<Vec<AuthUseKind>> {
    let content;
    bracketed!(content in input);
    let mut out = Vec::new();
    while !content.is_empty() {
        out.push(parse_auth_use_kind(&content)?);
        if content.peek(Token![,]) {
            content.parse::<Token![,]>()?;
        } else if !content.is_empty() {
            let tt: TokenTree = content.parse()?;
            return Err(syn::Error::new(
                tt.span(),
                "expected `,` between auth usages",
            ));
        }
    }
    Ok(out)
}

fn parse_auth_use_kind(input: ParseStream<'_>) -> Result<AuthUseKind> {
    let usage: Ident = input.parse()?;

    let kind = match usage.to_string().as_str() {
        "BearerAuth" => {
            let content;
            parenthesized!(content in input);
            let credential: Ident = content.parse()?;
            if !content.is_empty() {
                return Err(syn::Error::new(
                    content.span(),
                    "unexpected BearerAuth arguments",
                ));
            }
            AuthUseKind::Bearer { credential }
        }
        "HeaderAuth" => {
            let content;
            parenthesized!(content in input);
            let header: LitStr = content.parse()?;
            content.parse::<Token![,]>()?;
            let credential: Ident = content.parse()?;
            if !content.is_empty() {
                return Err(syn::Error::new(
                    content.span(),
                    "unexpected HeaderAuth arguments",
                ));
            }
            AuthUseKind::Header { header, credential }
        }
        "QueryAuth" => {
            let content;
            parenthesized!(content in input);
            let key: LitStr = content.parse()?;
            content.parse::<Token![,]>()?;
            let credential: Ident = content.parse()?;
            if !content.is_empty() {
                return Err(syn::Error::new(
                    content.span(),
                    "unexpected QueryAuth arguments",
                ));
            }
            AuthUseKind::Query { key, credential }
        }
        "BasicAuth" => {
            let content;
            parenthesized!(content in input);
            let credential: Ident = content.parse()?;
            if !content.is_empty() {
                return Err(syn::Error::new(
                    content.span(),
                    "unexpected BasicAuth arguments",
                ));
            }
            AuthUseKind::Basic { credential }
        }
        "CertificateAuth" => {
            let content;
            parenthesized!(content in input);
            let credential: Ident = content.parse()?;
            if !content.is_empty() {
                return Err(syn::Error::new(
                    content.span(),
                    "unexpected CertificateAuth arguments",
                ));
            }
            AuthUseKind::Certificate { credential }
        }
        "Custom" => {
            let usage_ty = parse_angle_type(input, usage.span(), "custom auth usage")?;
            let content;
            parenthesized!(content in input);
            let usage: Expr = content.parse()?;
            content.parse::<Token![,]>()?;
            let credential: Ident = content.parse()?;
            if !content.is_empty() {
                return Err(syn::Error::new(
                    content.span(),
                    "unexpected Custom auth usage arguments",
                ));
            }
            AuthUseKind::Custom {
                usage_ty,
                usage,
                credential,
            }
        }
        _ => {
            return Err(syn::Error::new(
                usage.span(),
                "unknown auth usage; expected BearerAuth, HeaderAuth, QueryAuth, BasicAuth, CertificateAuth, or Custom<T>",
            ));
        }
    };

    Ok(kind)
}

fn parse_vars_block(input: ParseStream<'_>) -> Result<VarsBlock> {
    let content;
    braced!(content in input);
    let mut decls = Vec::new();
    while !content.is_empty() {
        decls.push(content.parse::<VarDeclNoWire>()?);
        if content.peek(Token![,]) {
            content.parse::<Token![,]>()?;
            continue;
        }
        if !content.is_empty() {
            let tt: TokenTree = content.parse()?;
            return Err(syn::Error::new(
                tt.span(),
                "expected `,` between vars declarations",
            ));
        }
    }
    Ok(VarsBlock { decls })
}

fn parse_params_block(input: ParseStream<'_>) -> Result<Vec<VarDeclNoWire>> {
    input.parse::<kw::params>()?;
    let content;
    braced!(content in input);
    let mut decls = Vec::new();
    while !content.is_empty() {
        decls.push(content.parse::<VarDeclNoWire>()?);
        if content.peek(Token![,]) {
            content.parse::<Token![,]>()?;
            continue;
        }
        if !content.is_empty() {
            let tt: TokenTree = content.parse()?;
            return Err(syn::Error::new(
                tt.span(),
                "expected `,` between params declarations",
            ));
        }
    }
    Ok(decls)
}

enum RetryDecl {
    Profiles(RetryProfilesBlock),
    Spec(RetrySpec),
}

fn parse_retry_decl(input: ParseStream<'_>) -> Result<RetryDecl> {
    input.parse::<kw::retry>()?;

    if input.peek(kw::off) {
        input.parse::<kw::off>()?;
        return Ok(RetryDecl::Spec(RetrySpec::Off));
    }

    if input.peek(token::Brace) {
        let content;
        braced!(content in input);
        if content.peek(kw::profile) || content.peek(kw::default) {
            return Ok(RetryDecl::Profiles(parse_retry_profiles_body(&content)?));
        }
        return Ok(RetryDecl::Spec(RetrySpec::Patch(parse_retry_patch_body(
            &content,
        )?)));
    }

    Ok(RetryDecl::Spec(RetrySpec::Profile(input.parse()?)))
}

fn parse_retry_profiles_body(input: ParseStream<'_>) -> Result<RetryProfilesBlock> {
    let mut profiles = Vec::new();
    let mut default: Option<Ident> = None;

    while !input.is_empty() {
        if input.peek(kw::profile) {
            input.parse::<kw::profile>()?;
            let name: Ident = input.parse()?;
            let extends = if input.peek(kw::extends) {
                input.parse::<kw::extends>()?;
                Some(input.parse()?)
            } else {
                None
            };
            let content;
            braced!(content in input);
            profiles.push(RetryProfileDef {
                name,
                extends,
                patch: parse_retry_patch_body(&content)?,
            });
        } else if input.peek(kw::default) {
            if default.is_some() {
                return Err(syn::Error::new(input.span(), "duplicate retry default"));
            }
            input.parse::<kw::default>()?;
            default = Some(input.parse()?);
        } else {
            let tt: TokenTree = input.parse()?;
            return Err(syn::Error::new(
                tt.span(),
                "unexpected token in retry profile block",
            ));
        }
        let _ = input.parse::<Option<Token![,]>>()?;
    }

    Ok(RetryProfilesBlock { profiles, default })
}

fn parse_retry_patch_body(input: ParseStream<'_>) -> Result<RetryPatch> {
    let mut patch = RetryPatch::default();

    while !input.is_empty() {
        if input.peek(kw::attempts) {
            input.parse::<kw::attempts>()?;
            set_retry_patch_field(
                &mut patch.attempts,
                input.parse::<LitInt>()?,
                input.span(),
                "attempts",
            )?;
        } else if input.peek(kw::methods) {
            input.parse::<kw::methods>()?;
            let methods = parse_ident_list(input)?;
            set_retry_patch_field(&mut patch.methods, methods, input.span(), "methods")?;
        } else if input.peek(kw::on) {
            input.parse::<kw::on>()?;
            if input.peek(kw::status) {
                input.parse::<kw::status>()?;
                let statuses = parse_lit_int_list(input)?;
                set_retry_patch_field(&mut patch.statuses, statuses, input.span(), "status")?;
            } else if input.peek(kw::transport) {
                input.parse::<kw::transport>()?;
                let transport_errors = parse_ident_list(input)?;
                set_retry_patch_field(
                    &mut patch.transport_errors,
                    transport_errors,
                    input.span(),
                    "transport",
                )?;
            } else {
                let tt: TokenTree = input.parse()?;
                return Err(syn::Error::new(
                    tt.span(),
                    "expected `status[...]` or `transport[...]` after `on`",
                ));
            }
        } else if input.peek(kw::backoff) {
            input.parse::<kw::backoff>()?;
            if input.peek(kw::none) {
                input.parse::<kw::none>()?;
                set_retry_patch_field(
                    &mut patch.backoff,
                    RetryBackoffSpec::None,
                    input.span(),
                    "backoff",
                )?;
            } else {
                let tt: TokenTree = input.parse()?;
                return Err(syn::Error::new(
                    tt.span(),
                    "expected `none` after `backoff`",
                ));
            }
        } else if input.peek(kw::retry_after) {
            input.parse::<kw::retry_after>()?;
            if input.peek(kw::honor) {
                input.parse::<kw::honor>()?;
                set_retry_patch_field(
                    &mut patch.respect_retry_after,
                    true,
                    input.span(),
                    "retry_after",
                )?;
            } else {
                let tt: TokenTree = input.parse()?;
                return Err(syn::Error::new(
                    tt.span(),
                    "expected `honor` after `retry_after`",
                ));
            }
        } else if input.peek(kw::idempotency) {
            input.parse::<kw::idempotency>()?;
            if input.peek(kw::header) {
                input.parse::<kw::header>()?;
                let content;
                parenthesized!(content in input);
                let header: LitStr = content.parse()?;
                if !content.is_empty() {
                    return Err(syn::Error::new(
                        content.span(),
                        "unexpected idempotency header arguments",
                    ));
                }
                set_retry_patch_field(
                    &mut patch.idempotency,
                    RetryIdempotencySpec::Header(header),
                    input.span(),
                    "idempotency",
                )?;
            } else {
                let tt: TokenTree = input.parse()?;
                return Err(syn::Error::new(
                    tt.span(),
                    "expected `header(\"...\")` after `idempotency`",
                ));
            }
        } else {
            let tt: TokenTree = input.parse()?;
            return Err(syn::Error::new(
                tt.span(),
                "unexpected token in retry policy block",
            ));
        }
        let _ = input.parse::<Option<Token![,]>>()?;
    }

    Ok(patch)
}

fn set_retry_patch_field<T>(
    out: &mut Option<T>,
    value: T,
    span: Span,
    field: &'static str,
) -> Result<()> {
    if out.is_some() {
        return Err(syn::Error::new(
            span,
            format!("duplicate retry `{field}` field"),
        ));
    }
    *out = Some(value);
    Ok(())
}

fn parse_ident_list(input: ParseStream<'_>) -> Result<Vec<Ident>> {
    let content;
    bracketed!(content in input);
    let mut out = Vec::new();
    while !content.is_empty() {
        out.push(content.parse()?);
        let _ = content.parse::<Option<Token![,]>>()?;
    }
    Ok(out)
}

fn parse_lit_int_list(input: ParseStream<'_>) -> Result<Vec<LitInt>> {
    let content;
    bracketed!(content in input);
    let mut out = Vec::new();
    while !content.is_empty() {
        out.push(content.parse()?);
        let _ = content.parse::<Option<Token![,]>>()?;
    }
    Ok(out)
}

enum CacheDecl {
    Profiles(CacheProfilesBlock),
    Spec(CacheSpec),
}

fn parse_cache_decl(input: ParseStream<'_>) -> Result<CacheDecl> {
    let fork = input.fork();
    fork.parse::<kw::cache>()?;
    if fork.peek(token::Brace) {
        let content;
        braced!(content in fork);
        if content.peek(kw::profile) || content.peek(kw::default) {
            return Ok(CacheDecl::Profiles(parse_cache_profiles_decl(input)?));
        }
    }
    Ok(CacheDecl::Spec(parse_cache_spec(input)?))
}

fn parse_cache_profiles_decl(input: ParseStream<'_>) -> Result<CacheProfilesBlock> {
    input.parse::<kw::cache>()?;
    let content;
    braced!(content in input);

    let mut profiles = Vec::new();
    let mut default = None;
    while !content.is_empty() {
        if content.peek(kw::profile) {
            content.parse::<kw::profile>()?;
            let name: Ident = content.parse()?;
            let extends = if content.peek(kw::extends) {
                content.parse::<kw::extends>()?;
                Some(content.parse()?)
            } else {
                None
            };
            let body;
            braced!(body in content);
            profiles.push(CacheProfileDef {
                name,
                extends,
                patch: parse_cache_patch_body(&body)?,
            });
        } else if content.peek(kw::default) {
            if default.is_some() {
                return Err(syn::Error::new(content.span(), "duplicate cache default"));
            }
            content.parse::<kw::default>()?;
            default = Some(content.parse()?);
        } else {
            let tt: TokenTree = content.parse()?;
            return Err(syn::Error::new(
                tt.span(),
                "unexpected token in cache block",
            ));
        }
        let _ = content.parse::<Option<Token![,]>>()?;
    }

    Ok(CacheProfilesBlock { profiles, default })
}

fn parse_cache_spec(input: ParseStream<'_>) -> Result<CacheSpec> {
    input.parse::<kw::cache>()?;
    if input.peek(kw::off) {
        input.parse::<kw::off>()?;
        return Ok(CacheSpec::Off);
    }

    let only = if input.peek(kw::only) {
        input.parse::<kw::only>()?;
        true
    } else {
        false
    };

    if input.peek(token::Brace) {
        let content;
        braced!(content in input);
        return Ok(CacheSpec::Patch {
            only,
            patch: parse_cache_patch_body(&content)?,
        });
    }

    Ok(CacheSpec::Profile {
        only,
        profile: input.parse()?,
    })
}

fn parse_cache_patch_body(input: ParseStream<'_>) -> Result<CachePatch> {
    let mut patch = CachePatch::default();
    while !input.is_empty() {
        if input.peek(kw::http) {
            if patch.http.is_some() {
                return Err(syn::Error::new(input.span(), "duplicate cache http mode"));
            }
            let token = input.parse::<kw::http>()?;
            patch.http = Some(token.span);
        } else if input.peek(kw::ttl) {
            if patch.ttl.is_some() {
                return Err(syn::Error::new(input.span(), "duplicate cache ttl"));
            }
            input.parse::<kw::ttl>()?;
            let amount: LitInt = input.parse()?;
            let unit = parse_rate_limit_duration_unit(input)?;
            patch.ttl = Some(CacheDurationSpec { amount, unit });
        } else if input.peek(kw::capacity) {
            if patch.capacity.is_some() {
                return Err(syn::Error::new(input.span(), "duplicate cache capacity"));
            }
            input.parse::<kw::capacity>()?;
            patch.capacity = Some(parse_cache_capacity(input)?);
        } else if input.peek(kw::max_body) {
            if patch.max_body.is_some() {
                return Err(syn::Error::new(input.span(), "duplicate cache max_body"));
            }
            input.parse::<kw::max_body>()?;
            patch.max_body = Some(parse_cache_size(input)?);
        } else if input.peek(kw::revalidate) {
            if patch.revalidate.is_some() {
                return Err(syn::Error::new(input.span(), "duplicate cache revalidate"));
            }
            input.parse::<kw::revalidate>()?;
            if !input.peek(LitBool) {
                return Err(syn::Error::new(
                    input.span(),
                    "expected `true` or `false` after `revalidate`",
                ));
            }
            patch.revalidate = Some(input.parse::<LitBool>()?);
        } else if input.peek(kw::shared) {
            if patch.shared.is_some() {
                return Err(syn::Error::new(input.span(), "duplicate cache shared"));
            }
            input.parse::<kw::shared>()?;
            patch.shared = Some(input.parse::<LitBool>()?);
        } else if input.peek(kw::on_error) {
            if patch.on_error.is_some() {
                return Err(syn::Error::new(input.span(), "duplicate cache on_error"));
            }
            input.parse::<kw::on_error>()?;
            patch.on_error = Some(if input.peek(kw::ignore) {
                input.parse::<kw::ignore>()?;
                CacheOnErrorSpec::Ignore
            } else if input.peek(kw::serve_stale) {
                input.parse::<kw::serve_stale>()?;
                CacheOnErrorSpec::ServeStale
            } else {
                let tt: TokenTree = input.parse()?;
                return Err(syn::Error::new(
                    tt.span(),
                    "expected `ignore` or `serve_stale` after `on_error`",
                ));
            });
        } else {
            let tt: TokenTree = input.parse()?;
            return Err(syn::Error::new(
                tt.span(),
                "unexpected token in cache policy block",
            ));
        }
        let _ = input.parse::<Option<Token![,]>>()?;
    }
    Ok(patch)
}

fn parse_cache_capacity(input: ParseStream<'_>) -> Result<CacheCapacitySpec> {
    let amount: LitInt = input.parse()?;
    if input.peek(kw::entries) {
        input.parse::<kw::entries>()?;
        Ok(CacheCapacitySpec::Entries { amount })
    } else {
        let unit = parse_cache_size_unit(input)?;
        Ok(CacheCapacitySpec::Bytes(CacheSizeSpec { amount, unit }))
    }
}

fn parse_cache_size(input: ParseStream<'_>) -> Result<CacheSizeSpec> {
    let amount: LitInt = input.parse()?;
    let unit = parse_cache_size_unit(input)?;
    Ok(CacheSizeSpec { amount, unit })
}

fn parse_cache_size_unit(input: ParseStream<'_>) -> Result<CacheSizeUnit> {
    if input.peek(kw::bytes) {
        input.parse::<kw::bytes>()?;
        Ok(CacheSizeUnit::Bytes)
    } else if input.peek(kw::kb) {
        input.parse::<kw::kb>()?;
        Ok(CacheSizeUnit::KiB)
    } else if input.peek(kw::kib) {
        input.parse::<kw::kib>()?;
        Ok(CacheSizeUnit::KiB)
    } else if input.peek(kw::mb) {
        input.parse::<kw::mb>()?;
        Ok(CacheSizeUnit::MiB)
    } else if input.peek(kw::mib) {
        input.parse::<kw::mib>()?;
        Ok(CacheSizeUnit::MiB)
    } else if input.peek(kw::gb) {
        input.parse::<kw::gb>()?;
        Ok(CacheSizeUnit::GiB)
    } else if input.peek(kw::gib) {
        input.parse::<kw::gib>()?;
        Ok(CacheSizeUnit::GiB)
    } else {
        let tt: TokenTree = input.parse()?;
        Err(syn::Error::new(
            tt.span(),
            "expected cache size unit `bytes`, `kb`/`kib`, `mb`/`mib`, or `gb`/`gib`",
        ))
    }
}

fn parse_rate_limit_profiles_decl(input: ParseStream<'_>) -> Result<RateLimitProfilesBlock> {
    input.parse::<kw::rate_limit>()?;
    let content;
    braced!(content in input);

    let mut profiles = Vec::new();
    let mut default = Vec::new();
    let mut response_policy = None;

    while !content.is_empty() {
        if content.peek(kw::response) {
            if response_policy.is_some() {
                return Err(syn::Error::new(
                    content.span(),
                    "duplicate rate_limit response policy",
                ));
            }
            content.parse::<kw::response>()?;
            let _ = content.parse::<Option<kw::custom>>()?;
            response_policy = Some(content.parse::<Path>()?);
        } else if content.peek(kw::profile) {
            content.parse::<kw::profile>()?;
            let name: Ident = content.parse()?;
            let extends = if content.peek(kw::extends) {
                content.parse::<kw::extends>()?;
                Some(content.parse()?)
            } else {
                None
            };
            let body;
            braced!(body in content);
            profiles.push(RateLimitProfileDef {
                name,
                extends,
                plan: parse_rate_limit_plan_body(&body)?,
            });
        } else if content.peek(kw::default) {
            if !default.is_empty() {
                return Err(syn::Error::new(
                    content.span(),
                    "duplicate rate_limit default",
                ));
            }
            content.parse::<kw::default>()?;
            default = parse_rate_limit_profile_list(&content)?;
        } else {
            let tt: TokenTree = content.parse()?;
            return Err(syn::Error::new(
                tt.span(),
                "unexpected token in rate_limit block",
            ));
        }
        let _ = content.parse::<Option<Token![,]>>()?;
    }

    Ok(RateLimitProfilesBlock {
        profiles,
        default,
        response_policy,
    })
}

fn parse_rate_limit_spec(input: ParseStream<'_>) -> Result<RateLimitSpec> {
    input.parse::<kw::rate_limit>()?;

    if input.peek(kw::off) {
        input.parse::<kw::off>()?;
        return Ok(RateLimitSpec::Off);
    }

    let only = if input.peek(kw::only) {
        input.parse::<kw::only>()?;
        true
    } else {
        false
    };

    if input.peek(token::Brace) {
        let content;
        braced!(content in input);
        return Ok(RateLimitSpec::Inline {
            only,
            plan: parse_rate_limit_plan_body(&content)?,
        });
    }

    Ok(RateLimitSpec::Profiles {
        only,
        profiles: parse_rate_limit_profile_list(input)?,
    })
}

fn parse_rate_limit_key_binding(input: ParseStream<'_>) -> Result<RateLimitKeyBindingSpec> {
    input.parse::<kw::rate_limit>()?;
    input.parse::<kw::key>()?;
    let name: Ident = input.parse()?;
    input.parse::<Token![=]>()?;
    let value: Ident = input.parse()?;
    Ok(RateLimitKeyBindingSpec { name, value })
}

fn parse_rate_limit_profile_list(input: ParseStream<'_>) -> Result<Vec<Ident>> {
    if input.peek(token::Bracket) {
        return parse_ident_list(input);
    }

    Ok(vec![input.parse()?])
}

fn parse_rate_limit_plan_body(input: ParseStream<'_>) -> Result<RateLimitPlanSpec> {
    let mut plan = RateLimitPlanSpec::default();

    while !input.is_empty() {
        if input.peek(kw::bucket) {
            plan.buckets.push(parse_rate_limit_bucket(input)?);
        } else {
            let tt: TokenTree = input.parse()?;
            return Err(syn::Error::new(
                tt.span(),
                "unexpected token in rate_limit plan; expected `bucket`",
            ));
        }
        let _ = input.parse::<Option<Token![,]>>()?;
    }

    Ok(plan)
}

fn parse_rate_limit_bucket(input: ParseStream<'_>) -> Result<RateLimitBucketSpec> {
    input.parse::<kw::bucket>()?;
    let kind: Ident = input.parse()?;
    input.parse::<kw::by>()?;
    let key = parse_rate_limit_key_list(input)?;
    let content;
    braced!(content in input);

    let mut cost = None;
    let mut windows = Vec::new();
    while !content.is_empty() {
        if content.peek(kw::cost) {
            if cost.is_some() {
                return Err(syn::Error::new(
                    content.span(),
                    "duplicate rate_limit bucket cost",
                ));
            }
            content.parse::<kw::cost>()?;
            cost = Some(content.parse::<LitInt>()?);
        } else if content.peek(kw::limit) {
            content.parse::<kw::limit>()?;
            let max: LitInt = content.parse()?;
            content.parse::<kw::every>()?;
            let every: LitInt = content.parse()?;
            let unit = parse_rate_limit_duration_unit(&content)?;
            windows.push(RateLimitWindowSpec { max, every, unit });
        } else {
            let tt: TokenTree = content.parse()?;
            return Err(syn::Error::new(
                tt.span(),
                "unexpected token in rate_limit bucket; expected `cost` or `limit`",
            ));
        }
        let _ = content.parse::<Option<Token![,]>>()?;
    }

    Ok(RateLimitBucketSpec {
        kind,
        key,
        cost,
        windows,
    })
}

fn parse_rate_limit_key_list(input: ParseStream<'_>) -> Result<Vec<RateLimitKeySpec>> {
    let content;
    bracketed!(content in input);
    let mut out = Vec::new();
    while !content.is_empty() {
        out.push(parse_rate_limit_key_spec(&content)?);
        let _ = content.parse::<Option<Token![,]>>()?;
    }
    Ok(out)
}

fn parse_rate_limit_key_spec(input: ParseStream<'_>) -> Result<RateLimitKeySpec> {
    if input.peek(LitStr) {
        return Ok(RateLimitKeySpec::Static(input.parse()?));
    }

    let first: Ident = input.parse()?;
    let first_s = first.to_string();
    if first_s == "route" && input.peek(Token![.]) {
        input.parse::<Token![.]>()?;
        let second: Ident = input.parse()?;
        if second == "host" {
            return Ok(RateLimitKeySpec::RouteHost);
        }
        return Err(syn::Error::new(
            second.span(),
            "unknown rate_limit route key; expected `route.host`",
        ));
    }

    match first_s.as_str() {
        "endpoint" => Ok(RateLimitKeySpec::Endpoint),
        "method" => Ok(RateLimitKeySpec::Method),
        _ => Ok(RateLimitKeySpec::Named(first)),
    }
}

fn parse_rate_limit_duration_unit(input: ParseStream<'_>) -> Result<RateLimitDurationUnit> {
    if input.peek(kw::second) {
        input.parse::<kw::second>()?;
        Ok(RateLimitDurationUnit::Seconds)
    } else if input.peek(kw::seconds) {
        input.parse::<kw::seconds>()?;
        Ok(RateLimitDurationUnit::Seconds)
    } else if input.peek(kw::minute) {
        input.parse::<kw::minute>()?;
        Ok(RateLimitDurationUnit::Minutes)
    } else if input.peek(kw::minutes) {
        input.parse::<kw::minutes>()?;
        Ok(RateLimitDurationUnit::Minutes)
    } else {
        let tt: TokenTree = input.parse()?;
        Err(syn::Error::new(
            tt.span(),
            "expected rate_limit duration unit `second(s)` or `minute(s)`",
        ))
    }
}

impl Parse for Item {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        if input.peek(kw::prefix) {
            Ok(Item::Layer(input.parse::<LayerDefTaggedPrefix>()?.0))
        } else if input.peek(kw::path) {
            Ok(Item::Layer(input.parse::<LayerDefTaggedPath>()?.0))
        } else if input.peek(kw::scope) {
            Ok(Item::Layer(input.parse::<LayerDefTaggedScope>()?.0))
        } else {
            Ok(Item::Endpoint(input.parse::<EndpointDef>()?))
        }
    }
}

struct LayerDefTaggedPrefix(LayerDef);
struct LayerDefTaggedPath(LayerDef);
struct LayerDefTaggedScope(LayerDef);

impl Parse for LayerDefTaggedPrefix {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        input.parse::<kw::prefix>()?;
        let route: RouteExpr = parse_route_expr_dot(input)?;
        let content;
        braced!(content in input);

        let mut policy = PolicyBlocks::default();
        let mut auth_uses: Vec<AuthUseDecl> = Vec::new();
        let mut cache: Option<CacheSpec> = None;
        let mut retry: Option<RetrySpec> = None;
        let mut rate_limit: Option<RateLimitSpec> = None;
        let mut items = Vec::new();

        while !content.is_empty() {
            if content.peek(kw::headers) {
                policy.headers = Some(content.parse::<PolicyBlockTaggedHeaders>()?.0);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::query) {
                policy.query = Some(content.parse::<PolicyBlockTaggedQuery>()?.0);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::timeout) {
                content.parse::<kw::timeout>()?;
                content.parse::<Token![:]>()?;
                policy.timeout = Some(content.parse::<Expr>()?);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::use_auth) {
                auth_uses.push(content.parse::<AuthUseDecl>()?);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::cache) {
                if cache.is_some() {
                    return Err(syn::Error::new(
                        content.span(),
                        "duplicate cache policy in prefix layer",
                    ));
                }
                match parse_cache_decl(&content)? {
                    CacheDecl::Spec(spec) => cache = Some(spec),
                    CacheDecl::Profiles(_) => {
                        return Err(syn::Error::new(
                            content.span(),
                            "cache profiles are only allowed in client blocks",
                        ));
                    }
                }
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::retry) {
                match parse_retry_decl(&content)? {
                    RetryDecl::Spec(spec) => {
                        if retry.is_some() {
                            return Err(syn::Error::new(
                                content.span(),
                                "duplicate retry policy in prefix layer",
                            ));
                        }
                        retry = Some(spec);
                    }
                    RetryDecl::Profiles(_) => {
                        return Err(syn::Error::new(
                            content.span(),
                            "retry profiles are only allowed in client blocks",
                        ));
                    }
                }
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::rate_limit) {
                if rate_limit.is_some() {
                    return Err(syn::Error::new(
                        content.span(),
                        "duplicate rate_limit policy in prefix layer",
                    ));
                }
                rate_limit = Some(parse_rate_limit_spec(&content)?);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::prefix) || content.peek(kw::path) || content.peek(kw::scope)
            {
                items.push(content.parse::<Item>()?);
            } else {
                // endpoint
                items.push(Item::Endpoint(content.parse::<EndpointDef>()?));
            }
        }

        Ok(Self(LayerDef {
            kind: LayerKind::Prefix,
            route,
            params: Vec::new(),
            policy,
            auth_uses,
            cache,
            retry,
            rate_limit,
            rate_limit_keys: Vec::new(),
            items,
        }))
    }
}

impl Parse for LayerDefTaggedPath {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        input.parse::<kw::path>()?;
        let route: RouteExpr = parse_route_expr_slash(input)?;
        let content;
        braced!(content in input);

        let mut policy = PolicyBlocks::default();
        let mut auth_uses: Vec<AuthUseDecl> = Vec::new();
        let mut cache: Option<CacheSpec> = None;
        let mut retry: Option<RetrySpec> = None;
        let mut rate_limit: Option<RateLimitSpec> = None;
        let mut items = Vec::new();

        while !content.is_empty() {
            if content.peek(kw::headers) {
                policy.headers = Some(content.parse::<PolicyBlockTaggedHeaders>()?.0);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::query) {
                policy.query = Some(content.parse::<PolicyBlockTaggedQuery>()?.0);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::timeout) {
                content.parse::<kw::timeout>()?;
                content.parse::<Token![:]>()?;
                policy.timeout = Some(content.parse::<Expr>()?);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::use_auth) {
                auth_uses.push(content.parse::<AuthUseDecl>()?);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::cache) {
                if cache.is_some() {
                    return Err(syn::Error::new(
                        content.span(),
                        "duplicate cache policy in path layer",
                    ));
                }
                match parse_cache_decl(&content)? {
                    CacheDecl::Spec(spec) => cache = Some(spec),
                    CacheDecl::Profiles(_) => {
                        return Err(syn::Error::new(
                            content.span(),
                            "cache profiles are only allowed in client blocks",
                        ));
                    }
                }
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::retry) {
                match parse_retry_decl(&content)? {
                    RetryDecl::Spec(spec) => {
                        if retry.is_some() {
                            return Err(syn::Error::new(
                                content.span(),
                                "duplicate retry policy in path layer",
                            ));
                        }
                        retry = Some(spec);
                    }
                    RetryDecl::Profiles(_) => {
                        return Err(syn::Error::new(
                            content.span(),
                            "retry profiles are only allowed in client blocks",
                        ));
                    }
                }
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::rate_limit) {
                if rate_limit.is_some() {
                    return Err(syn::Error::new(
                        content.span(),
                        "duplicate rate_limit policy in path layer",
                    ));
                }
                rate_limit = Some(parse_rate_limit_spec(&content)?);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::prefix) || content.peek(kw::path) || content.peek(kw::scope)
            {
                items.push(content.parse::<Item>()?);
            } else {
                items.push(Item::Endpoint(content.parse::<EndpointDef>()?));
            }
        }

        Ok(Self(LayerDef {
            kind: LayerKind::Path,
            route,
            params: Vec::new(),
            policy,
            auth_uses,
            cache,
            retry,
            rate_limit,
            rate_limit_keys: Vec::new(),
            items,
        }))
    }
}

impl Parse for LayerDefTaggedScope {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        input.parse::<kw::scope>()?;
        let _name: Ident = input.parse()?;

        let content;
        braced!(content in input);

        let mut params: Vec<VarDeclNoWire> = Vec::new();
        let mut policy = PolicyBlocks::default();
        let mut auth_uses: Vec<AuthUseDecl> = Vec::new();
        let mut cache: Option<CacheSpec> = None;
        let mut retry: Option<RetrySpec> = None;
        let mut rate_limit: Option<RateLimitSpec> = None;
        let mut rate_limit_keys = Vec::new();
        let mut host_route: Option<RouteExpr> = None;
        let mut path_route: Option<RouteExpr> = None;
        let mut items = Vec::new();

        while !content.is_empty() {
            if content.peek(kw::params) {
                if !params.is_empty() {
                    return Err(syn::Error::new(
                        content.span(),
                        "duplicate `params {}` in scope",
                    ));
                }
                params = parse_params_block(&content)?;
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::host) {
                if host_route.is_some() {
                    return Err(syn::Error::new(
                        content.span(),
                        "duplicate `host[...]` in scope",
                    ));
                }
                content.parse::<kw::host>()?;
                host_route = Some(parse_route_expr_bracket(&content)?);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::path) {
                if path_route.is_some() {
                    return Err(syn::Error::new(
                        content.span(),
                        "duplicate `path[...]` in scope",
                    ));
                }
                content.parse::<kw::path>()?;
                path_route = Some(parse_route_expr_bracket(&content)?);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::headers) {
                policy.headers = Some(content.parse::<PolicyBlockTaggedHeaders>()?.0);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::query) {
                policy.query = Some(content.parse::<PolicyBlockTaggedQuery>()?.0);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::timeout) {
                content.parse::<kw::timeout>()?;
                content.parse::<Token![:]>()?;
                let t = content.parse::<Expr>()?;
                policy.timeout = Some(normalize_policy_expr(t));
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::use_auth) {
                auth_uses.push(content.parse::<AuthUseDecl>()?);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::cache) {
                if cache.is_some() {
                    return Err(syn::Error::new(
                        content.span(),
                        "duplicate cache policy in scope",
                    ));
                }
                match parse_cache_decl(&content)? {
                    CacheDecl::Spec(spec) => cache = Some(spec),
                    CacheDecl::Profiles(_) => {
                        return Err(syn::Error::new(
                            content.span(),
                            "cache profiles are only allowed in client blocks",
                        ));
                    }
                }
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::retry) {
                match parse_retry_decl(&content)? {
                    RetryDecl::Spec(spec) => {
                        if retry.is_some() {
                            return Err(syn::Error::new(
                                content.span(),
                                "duplicate retry policy in scope",
                            ));
                        }
                        retry = Some(spec);
                    }
                    RetryDecl::Profiles(_) => {
                        return Err(syn::Error::new(
                            content.span(),
                            "retry profiles are only allowed in client blocks",
                        ));
                    }
                }
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::rate_limit) {
                let fork = content.fork();
                fork.parse::<kw::rate_limit>()?;
                if fork.peek(kw::key) {
                    rate_limit_keys.push(parse_rate_limit_key_binding(&content)?);
                } else {
                    if rate_limit.is_some() {
                        return Err(syn::Error::new(
                            content.span(),
                            "duplicate rate_limit policy in scope",
                        ));
                    }
                    rate_limit = Some(parse_rate_limit_spec(&content)?);
                }
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::scope) || content.peek(kw::prefix) || content.peek(kw::path)
            {
                items.push(content.parse::<Item>()?);
            } else {
                items.push(Item::Endpoint(content.parse::<EndpointDef>()?));
            }
        }

        // Legacy IR only supports one route-kind per layer.
        // Normalize `scope` into one or two nested legacy layers.
        let outer = match (host_route, path_route) {
            (Some(host), Some(path)) => LayerDef {
                kind: LayerKind::Prefix,
                route: host,
                params,
                policy,
                auth_uses,
                cache,
                retry,
                rate_limit,
                rate_limit_keys,
                items: vec![Item::Layer(LayerDef {
                    kind: LayerKind::Path,
                    route: path,
                    params: Vec::new(),
                    policy: PolicyBlocks::default(),
                    auth_uses: Vec::new(),
                    cache: None,
                    retry: None,
                    rate_limit: None,
                    rate_limit_keys: Vec::new(),
                    items,
                })],
            },
            (Some(host), None) => LayerDef {
                kind: LayerKind::Prefix,
                route: host,
                params,
                policy,
                auth_uses,
                cache,
                retry,
                rate_limit,
                rate_limit_keys,
                items,
            },
            (None, Some(path)) => LayerDef {
                kind: LayerKind::Path,
                route: path,
                params,
                policy,
                auth_uses,
                cache,
                retry,
                rate_limit,
                rate_limit_keys,
                items,
            },
            (None, None) => LayerDef {
                kind: LayerKind::Path,
                route: RouteExpr { atoms: Vec::new() },
                params,
                policy,
                auth_uses,
                cache,
                retry,
                rate_limit,
                rate_limit_keys,
                items,
            },
        };

        Ok(Self(outer))
    }
}

impl Parse for EndpointDef {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let method: Ident = input.parse()?;
        let name: Ident = input.parse()?;
        if input.peek(token::Brace) {
            let content;
            braced!(content in input);

            let mut params: Vec<VarDeclNoWire> = Vec::new();
            let mut route = RouteExpr { atoms: Vec::new() };
            let mut policy = PolicyBlocks::default();
            let mut auth_uses: Vec<AuthUseDecl> = Vec::new();
            let mut cache: Option<CacheSpec> = None;
            let mut retry: Option<RetrySpec> = None;
            let mut rate_limit: Option<RateLimitSpec> = None;
            let mut rate_limit_keys = Vec::new();
            let mut paginate: Option<PaginateSpec> = None;
            let mut body: Option<CodecSpec> = None;
            let mut response: Option<CodecSpec> = None;
            let mut map: Option<MapSpec> = None;

            while !content.is_empty() {
                if content.peek(kw::params) {
                    if !params.is_empty() {
                        return Err(syn::Error::new(
                            content.span(),
                            "duplicate `params {}` in endpoint",
                        ));
                    }
                    params = parse_params_block(&content)?;
                    let _ = content.parse::<Option<Token![,]>>()?;
                } else if content.peek(kw::path) {
                    if !route.atoms.is_empty() {
                        return Err(syn::Error::new(
                            content.span(),
                            "duplicate `path[...]` in endpoint",
                        ));
                    }
                    content.parse::<kw::path>()?;
                    route = parse_route_expr_bracket(&content)?;
                    let _ = content.parse::<Option<Token![,]>>()?;
                } else if content.peek(kw::headers) {
                    policy.headers = Some(content.parse::<PolicyBlockTaggedHeaders>()?.0);
                    let _ = content.parse::<Option<Token![,]>>()?;
                } else if content.peek(kw::query) {
                    policy.query = Some(content.parse::<PolicyBlockTaggedQuery>()?.0);
                    let _ = content.parse::<Option<Token![,]>>()?;
                } else if content.peek(kw::timeout) {
                    content.parse::<kw::timeout>()?;
                    content.parse::<Token![:]>()?;
                    let t = parse_expr_until_comma_or_endpoint_arrow(&content)?;
                    policy.timeout = Some(normalize_policy_expr(t));
                    let _ = content.parse::<Option<Token![,]>>()?;
                } else if content.peek(kw::use_auth) {
                    auth_uses.push(content.parse::<AuthUseDecl>()?);
                    let _ = content.parse::<Option<Token![,]>>()?;
                } else if content.peek(kw::cache) {
                    if cache.is_some() {
                        return Err(syn::Error::new(
                            name.span(),
                            "duplicate cache policy in endpoint",
                        ));
                    }
                    match parse_cache_decl(&content)? {
                        CacheDecl::Spec(spec) => cache = Some(spec),
                        CacheDecl::Profiles(_) => {
                            return Err(syn::Error::new(
                                name.span(),
                                "cache profiles are only allowed in client blocks",
                            ));
                        }
                    }
                    let _ = content.parse::<Option<Token![,]>>()?;
                } else if content.peek(kw::retry) {
                    match parse_retry_decl(&content)? {
                        RetryDecl::Spec(spec) => {
                            if retry.is_some() {
                                return Err(syn::Error::new(
                                    name.span(),
                                    "duplicate retry policy in endpoint",
                                ));
                            }
                            retry = Some(spec);
                        }
                        RetryDecl::Profiles(_) => {
                            return Err(syn::Error::new(
                                name.span(),
                                "retry profiles are only allowed in client blocks",
                            ));
                        }
                    }
                    let _ = content.parse::<Option<Token![,]>>()?;
                } else if content.peek(kw::rate_limit) {
                    let fork = content.fork();
                    fork.parse::<kw::rate_limit>()?;
                    if fork.peek(kw::key) {
                        rate_limit_keys.push(parse_rate_limit_key_binding(&content)?);
                    } else {
                        if rate_limit.is_some() {
                            return Err(syn::Error::new(
                                name.span(),
                                "duplicate rate_limit policy in endpoint",
                            ));
                        }
                        rate_limit = Some(parse_rate_limit_spec(&content)?);
                    }
                    let _ = content.parse::<Option<Token![,]>>()?;
                } else if content.peek(kw::paginate) {
                    if paginate.is_some() {
                        return Err(syn::Error::new(name.span(), "duplicate `paginate`"));
                    }
                    paginate = Some(content.parse::<PaginateSpec>()?);
                    let _ = content.parse::<Option<Token![,]>>()?;
                } else if content.peek(kw::body) {
                    if body.is_some() {
                        return Err(syn::Error::new(name.span(), "duplicate `body`"));
                    }
                    content.parse::<kw::body>()?;
                    body = Some(content.parse::<CodecSpec>()?);
                    let _ = content.parse::<Option<Token![,]>>()?;
                } else if content.peek(Token![->]) {
                    content.parse::<Token![->]>()?;
                    response = Some(content.parse::<CodecSpec>()?);
                    map = if content.peek(Token![|]) {
                        content.parse::<Token![|]>()?;
                        let out_ty: Type = content.parse()?;
                        content.parse::<Token![=>]>()?;
                        let body: Expr = content.parse()?;
                        Some(MapSpec { out_ty, body })
                    } else {
                        None
                    };
                    let _ = content.parse::<Option<token::Semi>>()?;
                    let _ = content.parse::<Option<Token![,]>>()?;
                } else {
                    let tt: proc_macro2::TokenTree = content.parse()?;
                    return Err(syn::Error::new(
                        tt.span(),
                        "unexpected token in endpoint block",
                    ));
                }
            }

            let response = response.ok_or_else(|| {
                syn::Error::new(name.span(), "endpoint block is missing `-> <Codec>`")
            })?;

            return Ok(Self {
                method,
                name,
                route,
                params,
                policy,
                auth_uses,
                cache,
                retry,
                rate_limit,
                rate_limit_keys,
                paginate,
                body,
                response,
                map,
            });
        }

        let route: RouteExpr = parse_route_expr_slash(input)?;
        let params: Vec<VarDeclNoWire> = Vec::new();
        let mut policy = PolicyBlocks::default();
        let mut auth_uses: Vec<AuthUseDecl> = Vec::new();
        let mut cache: Option<CacheSpec> = None;
        let mut retry: Option<RetrySpec> = None;
        let mut rate_limit: Option<RateLimitSpec> = None;
        let mut rate_limit_keys = Vec::new();
        let mut paginate: Option<PaginateSpec> = None;
        let mut body: Option<CodecSpec> = None;

        // parse endpoint parts until `->`
        while !input.peek(Token![->]) {
            if input.peek(kw::headers) {
                policy.headers = Some(input.parse::<PolicyBlockTaggedHeaders>()?.0);
                let _ = input.parse::<Option<Token![,]>>()?;
            } else if input.peek(kw::query) {
                policy.query = Some(input.parse::<PolicyBlockTaggedQuery>()?.0);
                let _ = input.parse::<Option<Token![,]>>()?;
            } else if input.peek(kw::timeout) {
                input.parse::<kw::timeout>()?;
                input.parse::<Token![:]>()?;
                policy.timeout = Some(normalize_policy_expr(
                    parse_expr_until_comma_or_endpoint_arrow(input)?,
                ));
                let _ = input.parse::<Option<Token![,]>>()?;
            } else if input.peek(kw::use_auth) {
                auth_uses.push(input.parse::<AuthUseDecl>()?);
                let _ = input.parse::<Option<Token![,]>>()?;
            } else if input.peek(kw::cache) {
                if cache.is_some() {
                    return Err(syn::Error::new(
                        name.span(),
                        "duplicate cache policy in endpoint",
                    ));
                }
                match parse_cache_decl(input)? {
                    CacheDecl::Spec(spec) => cache = Some(spec),
                    CacheDecl::Profiles(_) => {
                        return Err(syn::Error::new(
                            name.span(),
                            "cache profiles are only allowed in client blocks",
                        ));
                    }
                }
                let _ = input.parse::<Option<Token![,]>>()?;
            } else if input.peek(kw::retry) {
                match parse_retry_decl(input)? {
                    RetryDecl::Spec(spec) => {
                        if retry.is_some() {
                            return Err(syn::Error::new(
                                name.span(),
                                "duplicate retry policy in endpoint",
                            ));
                        }
                        retry = Some(spec);
                    }
                    RetryDecl::Profiles(_) => {
                        return Err(syn::Error::new(
                            name.span(),
                            "retry profiles are only allowed in client blocks",
                        ));
                    }
                }
                let _ = input.parse::<Option<Token![,]>>()?;
            } else if input.peek(kw::rate_limit) {
                let fork = input.fork();
                fork.parse::<kw::rate_limit>()?;
                if fork.peek(kw::key) {
                    rate_limit_keys.push(parse_rate_limit_key_binding(input)?);
                } else {
                    if rate_limit.is_some() {
                        return Err(syn::Error::new(
                            name.span(),
                            "duplicate rate_limit policy in endpoint",
                        ));
                    }
                    rate_limit = Some(parse_rate_limit_spec(input)?);
                }
                let _ = input.parse::<Option<Token![,]>>()?;
            } else if input.peek(kw::paginate) {
                if paginate.is_some() {
                    return Err(syn::Error::new(name.span(), "duplicate `paginate`"));
                }
                paginate = Some(input.parse::<PaginateSpec>()?);
                let _ = input.parse::<Option<Token![,]>>()?;
            } else if input.peek(kw::body) {
                if body.is_some() {
                    return Err(syn::Error::new(name.span(), "duplicate `body`"));
                }
                input.parse::<kw::body>()?;
                body = Some(input.parse::<CodecSpec>()?);
                let _ = input.parse::<Option<Token![,]>>()?;
            } else {
                let tt: proc_macro2::TokenTree = input.parse()?;
                return Err(syn::Error::new(
                    tt.span(),
                    "unexpected token in endpoint; expected use_auth/headers/query/timeout/paginate/body or `->`",
                ));
            }
        }

        input.parse::<Token![->]>()?;
        let response: CodecSpec = input.parse()?;

        let map = if input.peek(Token![|]) {
            input.parse::<Token![|]>()?;
            let out_ty: Type = input.parse()?;
            input.parse::<Token![=>]>()?;
            let body: Expr = input.parse()?;
            Some(MapSpec { out_ty, body })
        } else {
            None
        };

        let _semi: token::Semi = input.parse()?;

        Ok(Self {
            method,
            name,
            route,
            params,
            policy,
            auth_uses,
            cache,
            retry,
            rate_limit,
            rate_limit_keys,
            paginate,
            body,
            response,
            map,
        })
    }
}

impl Parse for PaginateSpec {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        input.parse::<kw::paginate>()?;
        let ctrl_ty: Path = input.parse()?;

        let content;
        braced!(content in input);

        let mut assigns = Vec::new();
        let mut first = true;
        while !content.is_empty() {
            if !first {
                if content.peek(Token![,]) {
                    content.parse::<Token![,]>()?;
                    if content.is_empty() {
                        return Err(syn::Error::new(
                            content.span(),
                            "trailing `,` not allowed in paginate block",
                        ));
                    }
                } else {
                    return Err(syn::Error::new(
                        content.span(),
                        "expected `,` between paginate assignments",
                    ));
                }
            }
            let key: Ident = content.parse()?;
            content.parse::<Token![=]>()?;
            let value: Expr = normalize_policy_expr(content.parse()?);
            assigns.push(PaginateAssign { key, value });
            first = false;
        }

        Ok(Self { ctrl_ty, assigns })
    }
}

struct PolicyBlockTaggedHeaders(PolicyBlock);
struct PolicyBlockTaggedQuery(PolicyBlock);

impl Parse for PolicyBlockTaggedHeaders {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        input.parse::<kw::headers>()?;
        Ok(Self(parse_policy_block(input, PolicyBlockKind::Headers)?))
    }
}

impl Parse for PolicyBlockTaggedQuery {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        input.parse::<kw::query>()?;
        Ok(Self(parse_policy_block(input, PolicyBlockKind::Query)?))
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum PolicyBlockKind {
    Headers,
    Query,
}

fn key_spec_span(key: &KeySpec) -> Span {
    match key {
        KeySpec::Ident(id) => id.span(),
        KeySpec::Str(s) => s.span(),
    }
}

fn stmt_span(stmt: &PolicyStmt) -> Span {
    match stmt {
        PolicyStmt::Remove { key } => key_spec_span(key),
        PolicyStmt::Set { key, .. } => key_spec_span(key),
        PolicyStmt::Bind { key, .. } => key_spec_span(key),
        PolicyStmt::BindShort { ident_key, .. } => ident_key.span(),
    }
}

fn parse_policy_block(input: ParseStream<'_>, kind: PolicyBlockKind) -> Result<PolicyBlock> {
    let content;
    braced!(content in input);
    let mut stmts = Vec::new();
    while !content.is_empty() {
        let stmt: PolicyStmt = content.parse()?;

        // 1.2: `+=` is query-only. Forbid in `headers {}` with a direct diagnostic.
        if kind == PolicyBlockKind::Headers
            && let PolicyStmt::Set {
                op: SetOp::Push, ..
            } = &stmt
        {
            return Err(syn::Error::new(
                stmt_span(&stmt),
                "`+=` is not allowed in `headers {}` blocks (query-only operator)",
            ));
        }

        stmts.push(stmt);

        // 1.3: allow trailing commas, but still require commas between statements.
        if content.peek(Token![,]) {
            content.parse::<Token![,]>()?;
            // trailing comma is allowed => if block ends after this, we simply exit
            continue;
        }
        if !content.is_empty() {
            let tt: TokenTree = content.parse()?;
            return Err(syn::Error::new(
                tt.span(),
                "expected `,` between policy statements",
            ));
        }
    }
    Ok(PolicyBlock { stmts })
}

impl Parse for PolicyStmt {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        if input.peek(Token![-]) {
            input.parse::<Token![-]>()?;
            let key = input.parse::<KeySpec>()?;
            return Ok(PolicyStmt::Remove { key });
        }

        // key or short bind start
        if input.peek(LitStr) {
            let key = KeySpec::Str(input.parse::<LitStr>()?);
            if input.peek(Token![as]) {
                input.parse::<Token![as]>()?;
                let decl = input.parse::<VarDeclNoWire>()?;
                return Ok(PolicyStmt::Bind { key, decl });
            }

            // set/push
            let op = if input.peek(Token![+=]) {
                input.parse::<Token![+=]>()?;
                SetOp::Push
            } else {
                input.parse::<Token![=]>()?;
                SetOp::Set
            };
            let value: PolicyValue = parse_policy_value(input)?;
            return Ok(PolicyStmt::Set { key, value, op });
        }

        // ident start
        let ident: Ident = input.parse()?;

        // short bind: ident ? : Type (= Expr)?
        if input.peek(Token![?]) || input.peek(Token![:]) {
            let optional = input.parse::<Option<Token![?]>>()?.is_some();
            input.parse::<Token![:]>()?;
            let ty: Type = input.parse()?;
            let default = if input.peek(Token![=]) {
                input.parse::<Token![=]>()?;
                Some(input.parse::<Expr>()?)
            } else {
                None
            };
            return Ok(PolicyStmt::BindShort {
                ident_key: ident.clone(),
                decl: VarDeclShort {
                    optional,
                    ty,
                    default,
                },
            });
        }

        let key = KeySpec::Ident(ident);

        if input.peek(Token![as]) {
            input.parse::<Token![as]>()?;
            let decl = input.parse::<VarDeclNoWire>()?;
            return Ok(PolicyStmt::Bind { key, decl });
        }

        let op = if input.peek(Token![+=]) {
            input.parse::<Token![+=]>()?;
            SetOp::Push
        } else {
            input.parse::<Token![=]>()?;
            SetOp::Set
        };
        let value: PolicyValue = parse_policy_value(input)?;
        Ok(PolicyStmt::Set { key, value, op })
    }
}

impl Parse for KeySpec {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        if input.peek(LitStr) {
            Ok(KeySpec::Str(input.parse()?))
        } else {
            Ok(KeySpec::Ident(input.parse()?))
        }
    }
}

impl Parse for VarDeclNoWire {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let rust: Ident = input.parse()?;
        let optional = input.parse::<Option<Token![?]>>()?.is_some();
        input.parse::<Token![:]>()?;
        let ty: Type = input.parse()?;
        let default = if input.peek(Token![=]) {
            input.parse::<Token![=]>()?;
            Some(input.parse::<Expr>()?)
        } else {
            None
        };
        Ok(Self {
            rust,
            optional,
            ty,
            default,
        })
    }
}

impl Parse for CodecSpec {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        // Parse as a Rust type path so we can accept `Enc<T>` directly.
        // Example: `JsonEncoding<MyType>` or `crate::codec::JsonEncoding<MyType>`.
        let tp: syn::TypePath = input.parse()?;

        if tp.qself.is_some() {
            return Err(syn::Error::new_spanned(
                tp,
                "codec spec does not support qualified paths; use `Enc<T>`",
            ));
        }

        let mut path = tp.path;

        if path.segments.is_empty() {
            return Err(syn::Error::new_spanned(
                path,
                "codec spec expects an encoding type like `Enc<T>`",
            ));
        }

        // Only allow generic args on the last segment.
        if path.segments.len() > 1 {
            for seg in path.segments.iter().take(path.segments.len() - 1) {
                if !matches!(seg.arguments, syn::PathArguments::None) {
                    return Err(syn::Error::new_spanned(
                        seg,
                        "codec spec only supports generic arguments on the last path segment: `Enc<T>`",
                    ));
                }
            }
        }

        let last = path.segments.last_mut().unwrap();

        // Extract exactly one type argument `T` from `Enc<T>`.
        // If there is no `<T>`, default to `()` (useful for NoContentEncoding).
        let ty: Type = match &last.arguments {
            syn::PathArguments::AngleBracketed(ab) => {
                let mut found: Option<Type> = None;

                for arg in ab.args.iter() {
                    match arg {
                        syn::GenericArgument::Type(t) => {
                            if found.is_some() {
                                return Err(syn::Error::new_spanned(
                                    ab,
                                    "codec spec expects exactly one type argument: `Enc<T>`",
                                ));
                            }
                            found = Some(t.clone());
                        }
                        _ => {
                            return Err(syn::Error::new_spanned(
                                arg,
                                "codec spec only supports a single type argument: `Enc<T>`",
                            ));
                        }
                    }
                }

                found.ok_or_else(|| {
                    syn::Error::new_spanned(ab, "codec spec expects a type argument: `Enc<T>`")
                })?
            }
            syn::PathArguments::None => syn::parse_quote!(()),
            other => {
                return Err(syn::Error::new_spanned(
                    other,
                    "codec spec expects angle-bracketed type arguments: `Enc<T>`",
                ));
            }
        };

        // Strip `<T>` from the encoding path so codegen can use `Decoded<Enc, T>`.
        last.arguments = syn::PathArguments::None;

        Ok(Self { enc: path, ty })
    }
}

fn parse_fmt_spec(input: ParseStream<'_>) -> Result<FmtSpec> {
    let fmt_kw: kw::fmt = input.parse()?;
    let span = fmt_kw.span;
    let require_all = input.parse::<Option<Token![?]>>()?.is_some();

    let content;
    bracketed!(content in input);

    let mut pieces: Vec<FmtPiece> = Vec::new();
    while !content.is_empty() {
        if content.peek(LitStr) {
            pieces.push(FmtPiece::Lit(content.parse::<LitStr>()?));
        } else if content.peek(token::Brace) {
            let inner;
            braced!(inner in content);
            // Prefer {cx.x}/{ep.y}/{auth.z} refs.
            if inner.peek(Ident) && inner.peek2(Token![.]) {
                let fork = inner.fork();
                let base: Ident = fork.parse()?;
                if (base == "cx" || base == "ep" || base == "auth") && fork.peek(Token![.]) {
                    let _dot: Token![.] = fork.parse()?;
                    let _name: Ident = fork.parse()?;
                    if fork.is_empty() {
                        // Commit on real stream.
                        let base: Ident = inner.parse()?;
                        inner.parse::<Token![.]>()?;
                        let name: Ident = inner.parse()?;
                        if !inner.is_empty() {
                            return Err(syn::Error::new(
                                inner.span(),
                                "unexpected tokens in fmt ref",
                            ));
                        }
                        let scope = if base == "cx" {
                            RefScope::Cx
                        } else if base == "ep" {
                            RefScope::Ep
                        } else {
                            RefScope::Auth
                        };
                        pieces.push(FmtPiece::Ref(ScopedRef { scope, ident: name }));
                    } else {
                        // fallthrough to decl
                        let d: TemplateVarDecl = inner.parse()?;
                        pieces.push(FmtPiece::Var(d));
                    }
                } else {
                    let d: TemplateVarDecl = inner.parse()?;
                    pieces.push(FmtPiece::Var(d));
                }
            } else {
                let d: TemplateVarDecl = inner.parse()?;
                pieces.push(FmtPiece::Var(d));
            }
        } else {
            let tt: TokenTree = content.parse()?;
            return Err(syn::Error::new(
                tt.span(),
                "expected string literal or `{var:Ty}` in fmt[...]",
            ));
        }
        let _ = content.parse::<Option<Token![,]>>()?;
    }

    Ok(FmtSpec {
        span,
        require_all,
        pieces,
    })
}

fn parse_part_spec(input: ParseStream<'_>) -> Result<FmtSpec> {
    let part_kw: kw::part = input.parse()?;
    let span = part_kw.span;
    let require_all = true;

    let content;
    bracketed!(content in input);

    let mut pieces: Vec<FmtPiece> = Vec::new();
    while !content.is_empty() {
        if content.peek(LitStr) {
            pieces.push(FmtPiece::Lit(content.parse::<LitStr>()?));
        } else if content.peek(token::Brace) {
            let inner;
            braced!(inner in content);
            if inner.peek(Ident) && inner.peek2(Token![.]) {
                let fork = inner.fork();
                let base: Ident = fork.parse()?;
                if (base == "cx" || base == "ep" || base == "auth") && fork.peek(Token![.]) {
                    let _dot: Token![.] = fork.parse()?;
                    let _name: Ident = fork.parse()?;
                    if fork.is_empty() {
                        let base: Ident = inner.parse()?;
                        inner.parse::<Token![.]>()?;
                        let name: Ident = inner.parse()?;
                        let scope = if base == "cx" {
                            RefScope::Cx
                        } else if base == "auth" {
                            RefScope::Auth
                        } else {
                            RefScope::Ep
                        };
                        pieces.push(FmtPiece::Ref(ScopedRef { scope, ident: name }));
                    } else {
                        let d: TemplateVarDecl = inner.parse()?;
                        pieces.push(FmtPiece::Var(d));
                    }
                } else {
                    let d: TemplateVarDecl = inner.parse()?;
                    pieces.push(FmtPiece::Var(d));
                }
            } else {
                let d: TemplateVarDecl = inner.parse()?;
                pieces.push(FmtPiece::Var(d));
            }
        } else if content.peek(Ident) {
            let sr = parse_scoped_ref_from_ident(&content)?;
            pieces.push(FmtPiece::Ref(sr));
        } else {
            let tt: TokenTree = content.parse()?;
            return Err(syn::Error::new(
                tt.span(),
                "expected string literal, identifier, or `{var:Ty}` in part[...]",
            ));
        }
        let _ = content.parse::<Option<Token![,]>>()?;
    }

    Ok(FmtSpec {
        span,
        require_all,
        pieces,
    })
}

fn parse_policy_value(input: syn::parse::ParseStream<'_>) -> Result<PolicyValue> {
    if input.peek(kw::fmt) {
        return Ok(PolicyValue::Fmt(parse_fmt_spec(input)?));
    }
    if input.peek(kw::part) {
        return Ok(PolicyValue::Fmt(parse_part_spec(input)?));
    }

    let expr: syn::Expr = input.parse()?;
    Ok(PolicyValue::Expr(normalize_policy_expr(expr)))
}

fn parse_route_atom(input: ParseStream<'_>) -> Result<RouteAtom> {
    if input.peek(kw::fmt) {
        return Ok(RouteAtom::Fmt(parse_fmt_spec(input)?));
    }
    if input.peek(kw::part) {
        return Ok(RouteAtom::Fmt(parse_part_spec(input)?));
    }
    if input.peek(LitStr) {
        return Ok(RouteAtom::Static(input.parse::<LitStr>()?));
    }
    if input.peek(Ident) {
        let sr = parse_scoped_ref_from_ident(input)?;
        return Ok(RouteAtom::Ref(sr));
    }
    if input.peek(token::Brace) {
        let content;
        braced!(content in input);
        // Prefer {cx.x}/{ep.y}/{auth.z} refs.
        if content.peek(Ident) && content.peek2(Token![.]) {
            let fork = content.fork();
            let base: Ident = fork.parse()?;
            if (base == "cx" || base == "ep" || base == "auth") && fork.peek(Token![.]) {
                let _dot: Token![.] = fork.parse()?;
                let _name: Ident = fork.parse()?;
                if fork.is_empty() {
                    // Commit on real stream.
                    let base: Ident = content.parse()?;
                    content.parse::<Token![.]>()?;
                    let name: Ident = content.parse()?;
                    if !content.is_empty() {
                        return Err(syn::Error::new(
                            content.span(),
                            "unexpected tokens in route ref",
                        ));
                    }
                    let scope = if base == "cx" {
                        RefScope::Cx
                    } else if base == "ep" {
                        RefScope::Ep
                    } else {
                        RefScope::Auth
                    };
                    return Ok(RouteAtom::Ref(ScopedRef { scope, ident: name }));
                }
            }
        }
        // Fallback: declaration placeholder.
        let d: TemplateVarDecl = syn::parse2::<TemplateVarDecl>(content.parse::<TokenStream2>()?)?;
        return Ok(RouteAtom::Var(d));
    }
    let tt: proc_macro2::TokenTree = input.parse()?;
    Err(syn::Error::new(
        tt.span(),
        "expected string literal or `{var:Ty}` in route",
    ))
}

fn parse_route_expr_slash(input: ParseStream<'_>) -> Result<RouteExpr> {
    let mut atoms: Vec<RouteAtom> = Vec::new();
    atoms.push(parse_route_atom(input)?);
    while input.peek(Token![/]) {
        input.parse::<Token![/]>()?;
        atoms.push(parse_route_atom(input)?);
    }
    Ok(RouteExpr { atoms })
}

fn parse_route_expr_dot(input: ParseStream<'_>) -> Result<RouteExpr> {
    let mut atoms: Vec<RouteAtom> = Vec::new();
    atoms.push(parse_route_atom(input)?);
    while input.peek(Token![.]) {
        input.parse::<Token![.]>()?;
        atoms.push(parse_route_atom(input)?);
    }
    Ok(RouteExpr { atoms })
}

fn parse_route_expr_bracket(input: ParseStream<'_>) -> Result<RouteExpr> {
    let content;
    bracketed!(content in input);
    let mut atoms: Vec<RouteAtom> = Vec::new();
    while !content.is_empty() {
        atoms.push(parse_route_atom(&content)?);
        if content.peek(Token![,]) {
            content.parse::<Token![,]>()?;
            continue;
        }
        if !content.is_empty() {
            let tt: TokenTree = content.parse()?;
            return Err(syn::Error::new(
                tt.span(),
                "expected `,` between route items",
            ));
        }
    }
    Ok(RouteExpr { atoms })
}

fn parse_scoped_ref_from_ident(input: ParseStream<'_>) -> Result<ScopedRef> {
    let first: Ident = input.parse()?;
    if input.peek(Token![.]) {
        input.parse::<Token![.]>()?;
        let second: Ident = input.parse()?;
        if first == "vars" || first == "cx" {
            Ok(ScopedRef {
                scope: RefScope::Cx,
                ident: second,
            })
        } else if first == "secret" || first == "auth" {
            Ok(ScopedRef {
                scope: RefScope::Auth,
                ident: second,
            })
        } else {
            Ok(ScopedRef {
                scope: RefScope::Ep,
                ident: second,
            })
        }
    } else {
        Ok(ScopedRef {
            scope: RefScope::Ep,
            ident: first,
        })
    }
}

fn normalize_policy_expr(expr: Expr) -> Expr {
    match expr {
        Expr::Path(p) => {
            if p.qself.is_none() && p.path.segments.len() == 1 {
                let seg = &p.path.segments[0];
                let id = &seg.ident;
                if (*id != "vars")
                    && (*id != "secret")
                    && (*id != "cx")
                    && (*id != "auth")
                    && (*id != "ep")
                    && id
                        .to_string()
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_lowercase())
                {
                    return syn::parse_quote!(ep.#id);
                }
            }
            Expr::Path(p)
        }
        Expr::Field(mut f) => {
            if let Expr::Path(base_path) = &*f.base
                && base_path.qself.is_none()
                && base_path.path.segments.len() == 1
            {
                let b = &base_path.path.segments[0].ident;
                let nb: Ident = if *b == "vars" {
                    Ident::new("cx", b.span())
                } else if *b == "secret" {
                    Ident::new("auth", b.span())
                } else if *b == "cx" || *b == "ep" || *b == "auth" {
                    b.clone()
                } else {
                    Ident::new("ep", b.span())
                };
                f.base = Box::new(syn::parse_quote!(#nb));
            } else {
                f.base = Box::new(normalize_policy_expr(*f.base));
            }
            Expr::Field(f)
        }
        Expr::Cast(mut c) => {
            c.expr = Box::new(normalize_policy_expr(*c.expr));
            Expr::Cast(c)
        }
        Expr::Paren(mut p) => {
            p.expr = Box::new(normalize_policy_expr(*p.expr));
            Expr::Paren(p)
        }
        Expr::Reference(mut r) => {
            r.expr = Box::new(normalize_policy_expr(*r.expr));
            Expr::Reference(r)
        }
        Expr::Unary(mut u) => {
            u.expr = Box::new(normalize_policy_expr(*u.expr));
            Expr::Unary(u)
        }
        Expr::Binary(mut b) => {
            b.left = Box::new(normalize_policy_expr(*b.left));
            b.right = Box::new(normalize_policy_expr(*b.right));
            Expr::Binary(b)
        }
        other => other,
    }
}

fn parse_expr_until_comma_or_endpoint_arrow(input: ParseStream<'_>) -> Result<Expr> {
    let mut ts = TokenStream2::new();

    // Small closure-awareness:
    // If the timeout expr is a closure like `|x| -> T { ... }`, we must not stop on that `->`.
    // We only stop on `->` when it is NOT immediately after a top-level closure parameter list.
    let mut in_closure_params = false;
    let mut just_closed_closure_params = false;

    while !input.is_empty() {
        if input.peek(Token![,]) {
            break;
        }

        if input.peek(Token![->]) {
            if just_closed_closure_params {
                // This `->` belongs to a closure return type; consume it into the expr stream.
                let t1: TokenTree = input.parse()?;
                let t2: TokenTree = input.parse()?;
                ts.extend([t1, t2]);
                just_closed_closure_params = false;
                continue;
            }
            // This is the endpoint `->` delimiter.
            break;
        }

        let tt: TokenTree = input.parse()?;

        // Track top-level closure `|...|` so we don't confuse its `->` with the endpoint `->`.
        match &tt {
            TokenTree::Punct(p) if p.as_char() == '|' => {
                if !in_closure_params {
                    in_closure_params = true;
                    just_closed_closure_params = false;
                } else {
                    in_closure_params = false;
                    just_closed_closure_params = true;
                }
            }
            _ => {
                if just_closed_closure_params {
                    // Any token other than the closure `->` cancels the "just closed params" state.
                    just_closed_closure_params = false;
                }
            }
        }

        ts.extend(std::iter::once(tt));
    }

    syn::parse2::<Expr>(ts)
}
