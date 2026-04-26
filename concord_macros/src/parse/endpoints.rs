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

impl EndpointBlockParts {
    fn empty() -> Self {
        Self {
            route: RouteExpr { atoms: Vec::new() },
            policy: PolicyBlocks::default(),
            auth_uses: Vec::new(),
            cache: None,
            retry: None,
            rate_limit: None,
            rate_limit_keys: Vec::new(),
            paginate: None,
        }
    }

    fn merge(mut self, other: Self, name: &Ident) -> Result<Self> {
        if !other.route.atoms.is_empty() {
            if !self.route.atoms.is_empty() {
                return Err(syn::Error::new(name.span(), "duplicate `path[...]` in endpoint"));
            }
            self.route = other.route;
        }
        if other.policy.headers.is_some() {
            if self.policy.headers.is_some() {
                return Err(syn::Error::new(name.span(), "duplicate headers policy in endpoint"));
            }
            self.policy.headers = other.policy.headers;
        }
        if other.policy.query.is_some() {
            if self.policy.query.is_some() {
                return Err(syn::Error::new(name.span(), "duplicate query policy in endpoint"));
            }
            self.policy.query = other.policy.query;
        }
        if other.policy.timeout.is_some() {
            if self.policy.timeout.is_some() {
                return Err(syn::Error::new(name.span(), "duplicate timeout policy in endpoint"));
            }
            self.policy.timeout = other.policy.timeout;
        }
        self.auth_uses.extend(other.auth_uses);
        if other.cache.is_some() {
            if self.cache.is_some() {
                return Err(syn::Error::new(name.span(), "duplicate cache policy in endpoint"));
            }
            self.cache = other.cache;
        }
        if other.retry.is_some() {
            if self.retry.is_some() {
                return Err(syn::Error::new(name.span(), "duplicate retry policy in endpoint"));
            }
            self.retry = other.retry;
        }
        if other.rate_limit.is_some() {
            if self.rate_limit.is_some() {
                return Err(syn::Error::new(name.span(), "duplicate rate_limit policy in endpoint"));
            }
            self.rate_limit = other.rate_limit;
        }
        self.rate_limit_keys.extend(other.rate_limit_keys);
        if other.paginate.is_some() {
            if self.paginate.is_some() {
                return Err(syn::Error::new(name.span(), "duplicate `paginate`"));
            }
            self.paginate = other.paginate;
        }
        Ok(self)
    }
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
    let mut parts = EndpointBlockParts::empty();

    while !input.is_empty() {
        if input.peek(kw::params) {
            return Err(syn::Error::new(
                name.span(),
                "endpoint params blocks are not supported; declare params in `Name(...)`",
            ));
        } else if input.peek(kw::path) {
            if !parts.route.atoms.is_empty() {
                return Err(syn::Error::new(
                    input.span(),
                    "duplicate `path[...]` in endpoint",
                ));
            }
            input.parse::<kw::path>()?;
            parts.route = parse_route_expr_bracket(input)?;
            let _ = input.parse::<Option<Token![,]>>()?;
        } else if input.peek(kw::headers) {
            parts.policy.headers = Some(input.parse::<PolicyBlockTaggedHeaders>()?.0);
            let _ = input.parse::<Option<Token![,]>>()?;
        } else if input.peek(kw::header) {
            parts.policy.headers.get_or_insert_with(|| PolicyBlock { stmts: Vec::new() }).stmts.push(parse_v3_single_policy_stmt(input, PolicyBlockKind::Headers)?);
            let _ = input.parse::<Option<Token![,]>>()?;
        } else if input.peek(kw::query) {
            if input.peek2(token::Brace) {
                parts.policy.query = Some(input.parse::<PolicyBlockTaggedQuery>()?.0);
            } else {
                parts.policy.query.get_or_insert_with(|| PolicyBlock { stmts: Vec::new() }).stmts.push(parse_v3_single_policy_stmt(input, PolicyBlockKind::Query)?);
            }
            let _ = input.parse::<Option<Token![,]>>()?;
        } else if input.peek(kw::timeout) {
            input.parse::<kw::timeout>()?;
            input.parse::<Token![:]>()?;
            let t = parse_expr_until_comma_or_endpoint_arrow(input)?;
            parts.policy.timeout = Some(normalize_policy_expr(t));
            let _ = input.parse::<Option<Token![,]>>()?;
        } else if input.peek(kw::use_auth) {
            parts.auth_uses.push(input.parse::<AuthUseDecl>()?);
            let _ = input.parse::<Option<Token![,]>>()?;
        } else if input.peek(kw::auth) {
            input.parse::<kw::auth>()?;
            parts.auth_uses.push(parse_auth_use_decl_after_auth_keyword(input)?);
            let _ = input.parse::<Option<Token![,]>>()?;
        } else if input.peek(kw::cache) {
            if parts.cache.is_some() {
                return Err(syn::Error::new(
                    name.span(),
                    "duplicate cache policy in endpoint",
                ));
            }
            match parse_cache_decl(input)? {
                CacheDecl::Spec(spec) => parts.cache = Some(spec),
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
                    if parts.retry.is_some() {
                        return Err(syn::Error::new(
                            name.span(),
                            "duplicate retry policy in endpoint",
                        ));
                    }
                    parts.retry = Some(spec);
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
                parts.rate_limit_keys.push(parse_rate_limit_key_binding(input)?);
            } else {
                if parts.rate_limit.is_some() {
                    return Err(syn::Error::new(
                        name.span(),
                        "duplicate rate_limit policy in endpoint",
                    ));
                }
                parts.rate_limit = Some(parse_rate_limit_spec(input)?);
            }
            let _ = input.parse::<Option<Token![,]>>()?;
        } else if input.peek(kw::paginate) {
            if parts.paginate.is_some() {
                return Err(syn::Error::new(name.span(), "duplicate `paginate`"));
            }
            parts.paginate = Some(input.parse::<PaginateSpec>()?);
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

    Ok(parts)
}

fn parse_endpoint_inline_parts(input: ParseStream<'_>, name: &Ident) -> Result<EndpointBlockParts> {
    let mut parts = EndpointBlockParts::empty();
    loop {
        if input.is_empty() || input.peek(Token![->]) || input.peek(token::Brace) || input.peek(token::Semi) {
            break;
        }
        if input.peek(kw::path) {
            if !parts.route.atoms.is_empty() {
                return Err(syn::Error::new(input.span(), "duplicate `path[...]` in endpoint"));
            }
            input.parse::<kw::path>()?;
            parts.route = parse_route_expr_bracket(input)?;
        } else if input.peek(kw::headers) {
            if parts.policy.headers.is_some() {
                return Err(syn::Error::new(input.span(), "duplicate headers policy in endpoint"));
            }
            parts.policy.headers = Some(input.parse::<PolicyBlockTaggedHeaders>()?.0);
        } else if input.peek(kw::header) {
            parts
                .policy
                .headers
                .get_or_insert_with(|| PolicyBlock { stmts: Vec::new() })
                .stmts
                .push(parse_v3_single_policy_stmt(input, PolicyBlockKind::Headers)?);
        } else if input.peek(kw::query) {
            if input.peek2(token::Brace) {
                if parts.policy.query.is_some() {
                    return Err(syn::Error::new(input.span(), "duplicate query policy in endpoint"));
                }
                parts.policy.query = Some(input.parse::<PolicyBlockTaggedQuery>()?.0);
            } else {
                parts
                    .policy
                    .query
                    .get_or_insert_with(|| PolicyBlock { stmts: Vec::new() })
                    .stmts
                    .push(parse_v3_single_policy_stmt(input, PolicyBlockKind::Query)?);
            }
        } else if input.peek(kw::timeout) {
            input.parse::<kw::timeout>()?;
            if input.peek(Token![:]) {
                input.parse::<Token![:]>()?;
            }
            let t = parse_expr_until_comma_or_endpoint_arrow(input)?;
            parts.policy.timeout = Some(normalize_policy_expr(t));
        } else if input.peek(kw::use_auth) {
            parts.auth_uses.push(input.parse::<AuthUseDecl>()?);
        } else if input.peek(kw::auth) {
            input.parse::<kw::auth>()?;
            parts.auth_uses.push(parse_auth_use_decl_after_auth_keyword(input)?);
        } else if input.peek(kw::cache) {
            if parts.cache.is_some() {
                return Err(syn::Error::new(name.span(), "duplicate cache policy in endpoint"));
            }
            match parse_cache_decl(input)? {
                CacheDecl::Spec(spec) => parts.cache = Some(spec),
                CacheDecl::Profiles(_) => {
                    return Err(syn::Error::new(name.span(), "cache profiles are only allowed in client blocks"));
                }
            }
        } else if input.peek(kw::retry) {
            match parse_retry_decl(input)? {
                RetryDecl::Spec(spec) => {
                    if parts.retry.is_some() {
                        return Err(syn::Error::new(name.span(), "duplicate retry policy in endpoint"));
                    }
                    parts.retry = Some(spec);
                }
                RetryDecl::Profiles(_) => {
                    return Err(syn::Error::new(name.span(), "retry profiles are only allowed in client blocks"));
                }
            }
        } else if input.peek(kw::rate_limit) {
            let fork = input.fork();
            fork.parse::<kw::rate_limit>()?;
            if fork.peek(kw::key) {
                parts.rate_limit_keys.push(parse_rate_limit_key_binding(input)?);
            } else {
                if parts.rate_limit.is_some() {
                    return Err(syn::Error::new(name.span(), "duplicate rate_limit policy in endpoint"));
                }
                parts.rate_limit = Some(parse_rate_limit_spec(input)?);
            }
        } else if input.peek(kw::paginate) {
            if parts.paginate.is_some() {
                return Err(syn::Error::new(name.span(), "duplicate `paginate`"));
            }
            parts.paginate = Some(input.parse::<PaginateSpec>()?);
        } else {
            break;
        }
        let _ = input.parse::<Option<Token![,]>>()?;
    }
    Ok(parts)
}

