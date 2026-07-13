use crate::limits::DslScopeDepthGuard;

impl Parse for RawItem {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        if input.peek(kw::prefix) || input.peek(kw::path) {
            Err(syn::Error::new(
                input.span(),
                "invalid top-level item; expected `scope` or endpoint",
            ))
        } else if input.peek(kw::scope) {
            Ok(RawItem::Layer(Box::new(input.parse::<RawScopeTaggedScope>()?.0)))
        } else {
            Ok(RawItem::Endpoint(Box::new(input.parse::<RawEndpoint>()?)))
        }
    }
}

struct RawScopeTaggedScope(RawScope);

impl Parse for RawScopeTaggedScope {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let span = input.span();
        let scope_kw: kw::scope = input.parse()?;
        let scope_span = scope_kw.span;
        let name: Ident = input.parse()?;
        let _depth_guard = DslScopeDepthGuard::enter(scope_span)?;
        let params: Vec<VarDeclNoWire> = if input.peek(token::Paren) {
            parse_inline_var_decls(input, "scope param")?
        } else {
            Vec::new()
        };

        let content;
        braced!(content in input);
        let body_span = content.span();

        let mut policy = PolicyBlocks::default();
        let mut behavior_uses = Vec::new();
        let mut auth_uses: Vec<AuthUseDecl> = Vec::new();
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
                path_route = Some(parse_path_route_expr_bracket(&content)?);
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
                let t = content.parse::<Expr>()?;
                policy.timeout = Some(t);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::profile) {
                behavior_uses.push(parse_behavior_use_spec(&content)?);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::behavior) {
                let legacy: kw::behavior = content.parse()?;
                return Err(legacy_behavior_keyword_error(legacy.span));
            } else if content.peek(kw::auth) {
                content.parse::<kw::auth>()?;
                auth_uses.push(parse_auth_use_decl_after_auth_keyword(&content)?);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::retry) {
                return Err(removed_retry_syntax_error(&content)?);
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
                items.push(content.parse::<RawItem>()?);
            } else if content.peek(kw::prefix) {
                return Err(syn::Error::new(content.span(), "invalid item in scope"));
            } else {
                items.push(RawItem::Endpoint(Box::new(content.parse::<RawEndpoint>()?)));
            }
        }

        Ok(Self(RawScope {
            span,
            scope_span,
            body_span,
            scope_name: Some(name),
            host_route,
            path_route,
            params,
            policy,
            behavior_uses,
            auth_uses,
            rate_limit,
            rate_limit_keys,
            items,
        }))
    }
}

impl Parse for RawEndpoint {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let span = input.span();
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
            if !input.is_empty() && !input.peek(token::Brace) && !input.peek(token::Semi) {
                let tt: TokenTree = input.parse()?;
                return Err(unsupported_endpoint_clause_error(tt));
            }
            return Err(syn::Error::new(
                input.span(),
                "endpoint declarations must use `METHOD Name(...) -> Response { ... }` (or `METHOD Name -> Response { ... }`)",
            ));
        }

        let response = parse_endpoint_response_spec(input)?;
        let trailing_parts = parse_endpoint_inline_parts(input, &name)?;
        let inline_parts = leading_parts.merge(trailing_parts, &name)?;

        if input.peek(Token![->]) {
            return Err(syn::Error::new(
                input.span(),
                "duplicate endpoint response marker",
            ));
        }
        if input.peek(token::Semi) {
            let _semi: token::Semi = input.parse()?;
            return Ok(raw_endpoint(
                span,
                method,
                name,
                alias,
                inline_parts.route,
                params,
                inline_parts.policy,
                inline_parts.behavior_uses,
                inline_parts.auth_uses,
                inline_parts.rate_limit,
                inline_parts.rate_limit_keys,
                inline_parts.paginate,
                body,
                response,
            ));
        }

        if input.peek(token::Brace) {
            return Err(syn::Error::new(
                input.span(),
                "DSL-002 endpoint braced blocks are not supported; endpoint clauses must be written in the stanza",
            ));
        }

        Ok(raw_endpoint(
            span,
            method,
            name,
            alias,
            inline_parts.route,
            params,
            inline_parts.policy,
            inline_parts.behavior_uses,
            inline_parts.auth_uses,
            inline_parts.rate_limit,
            inline_parts.rate_limit_keys,
            inline_parts.paginate,
            body,
            response,
        ))
    }
}

#[allow(clippy::too_many_arguments)]
fn raw_endpoint(
    span: Span,
    method: Ident,
    name: Ident,
    alias: Option<Ident>,
    route: RouteExpr,
    params: Vec<VarDeclNoWire>,
    policy: PolicyBlocks,
    behavior_uses: Vec<BehaviorUseSpec>,
    auth_uses: Vec<AuthUseDecl>,
    rate_limit: Option<RateLimitSpec>,
    rate_limit_keys: Vec<RateLimitKeyBindingSpec>,
    paginate: Option<PaginateSpec>,
    body: RawRequestIo,
    response: RawResponseIo,
) -> RawEndpoint {
    RawEndpoint {
        line: RawEndpointLine {
            span,
            method: method.clone(),
            name: name.clone(),
            alias: alias.clone(),
        },
        span,
        method,
        name,
        alias,
        route,
        params,
        policy,
        behavior_uses,
        auth_uses,
        rate_limit,
        rate_limit_keys,
        paginate,
        body,
        response,
    }
}

fn unsupported_endpoint_clause_error(tt: TokenTree) -> syn::Error {
    if let TokenTree::Ident(ident) = &tt {
        if ident == "part" {
            return syn::Error::new(
                tt.span(),
                "`part[...]` is not supported; use `fmt[...]` route atoms",
            );
        }
        if ident == "body" {
            return syn::Error::new(
                tt.span(),
                "body stanza lines are not supported; declare body as an endpoint signature argument",
            );
        }
    }
    syn::Error::new(tt.span(), "DSL-001 unknown endpoint clause")
}

impl Parse for PaginateSpec {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        input.parse::<kw::paginate>()?;
        if input.peek(kw::endpoint_state) {
            let endpoint_state_kw: kw::endpoint_state = input.parse()?;
            return Err(syn::Error::new(
                endpoint_state_kw.span,
                "pagination no longer uses `endpoint_state ... bindings ...`; use `paginate Controller { ... }`",
            ));
        }
        let ctrl_ty: Type = input.parse()?;

        if input.peek(kw::bindings) {
            return Err(syn::Error::new(
                input.span(),
                "pagination no longer uses `bindings`; use `paginate Controller { ... }`",
            ));
        }

        if !input.peek(token::Brace) {
            return Ok(Self {
                ctrl_ty,
                assigns: Vec::new(),
            });
        }

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
            let value: Expr = content.parse()?;
            assigns.push(PaginateAssign { key, value });
            first = false;
        }

        Ok(Self {
            ctrl_ty,
            assigns,
        })
    }
}

