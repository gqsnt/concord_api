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

fn parse_inline_var_decls(input: ParseStream<'_>, ctx: &'static str) -> Result<Vec<VarDeclNoWire>> {
    let content;
    parenthesized!(content in input);
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
                format!("expected `,` between {ctx} declarations"),
            ));
        }
    }
    Ok(decls)
}

struct EndpointBlockParts {
    route: RouteExpr,
    policy: PolicyBlocks,
    auth_uses: Vec<AuthUseDecl>,
    cache: Option<CacheSpec>,
    retry: Option<RetrySpec>,
    rate_limit: Option<RateLimitSpec>,
    rate_limit_keys: Vec<RateLimitKeyBindingSpec>,
    paginate: Option<PaginateSpec>,
}

fn parse_endpoint_response_spec(input: ParseStream<'_>) -> Result<(CodecSpec, Option<MapSpec>)> {
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

    Ok((response, map))
}

fn parse_endpoint_signature_args(
    input: ParseStream<'_>,
) -> Result<(Vec<VarDeclNoWire>, Option<CodecSpec>)> {
    let content;
    parenthesized!(content in input);

    let mut params = Vec::new();
    let mut body = None;

    while !content.is_empty() {
        if content.peek(kw::body) {
            if body.is_some() {
                return Err(syn::Error::new(
                    content.span(),
                    "duplicate `body` in endpoint signature",
                ));
            }
            content.parse::<kw::body>()?;
            content.parse::<Token![:]>()?;
            body = Some(content.parse::<CodecSpec>()?);
        } else {
            params.push(content.parse::<VarDeclNoWire>()?);
        }

        if content.peek(Token![,]) {
            content.parse::<Token![,]>()?;
            continue;
        }
        if !content.is_empty() {
            let tt: TokenTree = content.parse()?;
            return Err(syn::Error::new(
                tt.span(),
                "expected `,` between endpoint signature items",
            ));
        }
    }

    Ok((params, body))
}

fn parse_endpoint_block_parts(input: ParseStream<'_>, name: &Ident) -> Result<EndpointBlockParts> {
    let mut route = RouteExpr { atoms: Vec::new() };
    let mut policy = PolicyBlocks::default();
    let mut auth_uses: Vec<AuthUseDecl> = Vec::new();
    let mut cache: Option<CacheSpec> = None;
    let mut retry: Option<RetrySpec> = None;
    let mut rate_limit: Option<RateLimitSpec> = None;
    let mut rate_limit_keys = Vec::new();
    let mut paginate: Option<PaginateSpec> = None;

    while !input.is_empty() {
        if input.peek(kw::params) {
            return Err(syn::Error::new(
                name.span(),
                "endpoint params blocks are not supported; declare params in `Name(...)`",
            ));
        } else if input.peek(kw::path) {
            if !route.atoms.is_empty() {
                return Err(syn::Error::new(
                    input.span(),
                    "duplicate `path[...]` in endpoint",
                ));
            }
            input.parse::<kw::path>()?;
            route = parse_route_expr_bracket(input)?;
            let _ = input.parse::<Option<Token![,]>>()?;
        } else if input.peek(kw::headers) {
            policy.headers = Some(input.parse::<PolicyBlockTaggedHeaders>()?.0);
            let _ = input.parse::<Option<Token![,]>>()?;
        } else if input.peek(kw::query) {
            policy.query = Some(input.parse::<PolicyBlockTaggedQuery>()?.0);
            let _ = input.parse::<Option<Token![,]>>()?;
        } else if input.peek(kw::timeout) {
            input.parse::<kw::timeout>()?;
            input.parse::<Token![:]>()?;
            let t = parse_expr_until_comma_or_endpoint_arrow(input)?;
            policy.timeout = Some(normalize_policy_expr(t));
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
            return Err(syn::Error::new(
                name.span(),
                "endpoint body blocks are not supported; declare body in `Name(body: Codec<...>)`",
            ));
        } else if input.peek(Token![->]) {
            return Err(syn::Error::new(
                name.span(),
                "endpoint response blocks are not supported; declare response in endpoint header",
            ));
        } else {
            let tt: proc_macro2::TokenTree = input.parse()?;
            return Err(syn::Error::new(
                tt.span(),
                "unexpected token in endpoint block",
            ));
        }
    }

    Ok(EndpointBlockParts {
        route,
        policy,
        auth_uses,
        cache,
        retry,
        rate_limit,
        rate_limit_keys,
        paginate,
    })
}

