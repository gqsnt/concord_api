impl Parse for Item {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        if input.peek(kw::prefix) || input.peek(kw::path) {
            Err(syn::Error::new(
                input.span(),
                "invalid top-level item; expected `scope` or endpoint",
            ))
        } else if input.peek(kw::scope) {
            Ok(Item::Layer(Box::new(input.parse::<LayerDefTaggedScope>()?.0)))
        } else {
            Ok(Item::Endpoint(Box::new(input.parse::<EndpointDef>()?)))
        }
    }
}

struct LayerDefTaggedScope(LayerDef);

impl Parse for LayerDefTaggedScope {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        input.parse::<kw::scope>()?;
        let name: Ident = input.parse()?;
        let params: Vec<VarDeclNoWire> = if input.peek(token::Paren) {
            parse_inline_var_decls(input, "scope param")?
        } else {
            Vec::new()
        };

        let content;
        braced!(content in input);

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
                return Err(syn::Error::new(
                    content.span(),
                    "scope params blocks are not supported; declare params in `scope name(...)`",
                ));
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
            } else if content.peek(kw::header) {
                policy
                    .headers
                    .get_or_insert_with(|| PolicyBlock { stmts: Vec::new() })
                    .stmts
                    .push(parse_inline_policy_stmt(&content, PolicyBlockKind::Headers)?);
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
                let t = content.parse::<Expr>()?;
                policy.timeout = Some(normalize_policy_expr(t));
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::use_auth) {
                return Err(syn::Error::new(
                    content.span(),
                    "`use_auth` was removed in v4; use `auth ...`",
                ));
            } else if content.peek(kw::auth) {
                content.parse::<kw::auth>()?;
                auth_uses.push(parse_auth_use_decl_after_auth_keyword(&content)?);
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
            } else if content.peek(kw::scope) {
                items.push(content.parse::<Item>()?);
            } else if content.peek(kw::prefix) {
                return Err(syn::Error::new(content.span(), "invalid item in scope"));
            } else {
                items.push(Item::Endpoint(Box::new(content.parse::<EndpointDef>()?)));
            }
        }

        // The normalized model stores one route-kind per layer.
        // Normalize `scope` into one or two nested internal layers.
        let outer = match (host_route, path_route) {
            (Some(host), Some(path)) => LayerDef {
                scope_name: Some(name),
                kind: LayerKind::Prefix,
                route: host,
                params,
                policy,
                auth_uses,
                cache,
                retry,
                rate_limit,
                rate_limit_keys,
                items: vec![Item::Layer(Box::new(LayerDef {
                    scope_name: None,
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
                }))],
            },
            (Some(host), None) => LayerDef {
                scope_name: Some(name),
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
                scope_name: Some(name),
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
                scope_name: Some(name),
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
        let (params, body) = if input.peek(token::Paren) {
            parse_endpoint_signature_args(input)?
        } else {
            (Vec::new(), None)
        };

        let alias = if input.peek(Token![as]) {
            input.parse::<Token![as]>()?;
            Some(input.parse::<Ident>()?)
        } else {
            None
        };

        let leading_parts = parse_endpoint_inline_parts(input, &name)?;

        if !input.peek(Token![->]) {
            return Err(syn::Error::new(
                input.span(),
                "endpoint declarations must use `METHOD Name(...) -> Response { ... }` (or `METHOD Name -> Response { ... }`)",
            ));
        }

        let (response, map) = parse_endpoint_response_spec(input)?;
        let trailing_parts = parse_endpoint_inline_parts(input, &name)?;
        let inline_parts = leading_parts.merge(trailing_parts, &name)?;

        if input.peek(token::Semi) {
            let _semi: token::Semi = input.parse()?;
            return Ok(Self {
                method,
                name,
                alias,
                route: inline_parts.route,
                params,
                policy: inline_parts.policy,
                auth_uses: inline_parts.auth_uses,
                cache: inline_parts.cache,
                retry: inline_parts.retry,
                rate_limit: inline_parts.rate_limit,
                rate_limit_keys: inline_parts.rate_limit_keys,
                paginate: inline_parts.paginate,
                body,
                response,
                map,
            });
        }

        if !input.peek(token::Brace) {
            return Ok(Self {
                method,
                name,
                alias,
                route: inline_parts.route,
                params,
                policy: inline_parts.policy,
                auth_uses: inline_parts.auth_uses,
                cache: inline_parts.cache,
                retry: inline_parts.retry,
                rate_limit: inline_parts.rate_limit,
                rate_limit_keys: inline_parts.rate_limit_keys,
                paginate: inline_parts.paginate,
                body,
                response,
                map,
            });
        }

        let content;
        braced!(content in input);
        let parts = inline_parts.merge(parse_endpoint_block_parts(&content, &name)?, &name)?;

        Ok(Self {
            method,
            name,
            alias,
            route: parts.route,
            params,
            policy: parts.policy,
            auth_uses: parts.auth_uses,
            cache: parts.cache,
            retry: parts.retry,
            rate_limit: parts.rate_limit,
            rate_limit_keys: parts.rate_limit_keys,
            paginate: parts.paginate,
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

