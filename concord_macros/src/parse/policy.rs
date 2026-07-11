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

fn merge_policy_block(slot: &mut Option<PolicyBlock>, mut block: PolicyBlock) {
    slot.get_or_insert_with(|| PolicyBlock { stmts: Vec::new() })
        .stmts
        .append(&mut block.stmts);
}

fn push_policy_stmt(slot: &mut Option<PolicyBlock>, stmt: PolicyStmt) {
    slot.get_or_insert_with(|| PolicyBlock { stmts: Vec::new() })
        .stmts
        .push(stmt);
}

fn parse_policy_block(input: ParseStream<'_>, kind: PolicyBlockKind) -> Result<PolicyBlock> {
    let content;
    braced!(content in input);
    let mut stmts = Vec::new();
    while !content.is_empty() {
        let stmt: PolicyStmt = if kind == PolicyBlockKind::Query {
            parse_query_policy_stmt(&content)?
        } else {
            content.parse()?
        };

        validate_policy_stmt_for_block(kind, &stmt)?;

        stmts.push(stmt);

        // 1.3: allow trailing commas, but still require commas between statements.
        if content.peek(Token![,]) {
            content.parse::<Token![,]>()?;
            // trailing comma is allowed => if block ends after this, we simply exit
            continue;
        }
        if kind == PolicyBlockKind::Query {
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

fn validate_policy_stmt_for_block(kind: PolicyBlockKind, stmt: &PolicyStmt) -> Result<()> {
    match (kind, stmt) {
        (PolicyBlockKind::Headers, PolicyStmt::Remove { key })
        | (PolicyBlockKind::Headers, PolicyStmt::Set { key, .. }) => {
            if matches!(key, KeySpec::Ident(_)) {
                return Err(syn::Error::new(
                    key_spec_span(key),
                    "header keys must be explicit string literals",
                ));
            }
        }
        (
            PolicyBlockKind::Query,
            PolicyStmt::Set {
                value: PolicyValue::Expr(Expr::Lit(expr_lit)),
                ..
            },
        ) if matches!(expr_lit.lit, syn::Lit::Bool(_)) => {
            return Err(syn::Error::new(
                expr_lit.span(),
                "boolean query flags are not supported; use an explicit typed parameter",
            ));
        }
        _ => {}
    }
    Ok(())
}

fn parse_query_policy_stmt(input: ParseStream<'_>) -> Result<PolicyStmt> {
    if input.peek(Ident) {
        let fork = input.fork();
        let ident: Ident = fork.parse()?;
            if !fork.peek(Token![=])
                && !fork.peek(Token![:])
                && !fork.peek(Token![?])
                && !fork.peek(Token![as])
            {
                input.parse::<Ident>()?;
                let value: Expr = syn::parse_quote!(#ident);
                return Ok(PolicyStmt::Set {
                    key: KeySpec::Ident(ident),
                    value: PolicyValue::Expr(value),
                });
            }
        }
    let stmt: PolicyStmt = input.parse()?;
    validate_policy_stmt_for_block(PolicyBlockKind::Query, &stmt)?;
    Ok(stmt)
}

fn parse_inline_policy_stmt(
    input: ParseStream<'_>,
    kind: PolicyBlockKind,
) -> Result<PolicyStmt> {
    let key = if kind == PolicyBlockKind::Headers {
        input.parse::<kw::header>()?;
        KeySpec::Str(input.parse::<LitStr>()?)
    } else {
        input.parse::<kw::query>()?;
        if input.peek(LitStr) {
            KeySpec::Str(input.parse::<LitStr>()?)
        } else {
            KeySpec::Ident(input.parse::<Ident>()?)
        }
    };

    if input.peek(Token![-]) {
        input.parse::<Token![-]>()?;
        return Ok(PolicyStmt::Remove { key });
    }

    input.parse::<Token![=]>()?;
    let value = PolicyValue::Expr(parse_expr_until_comma_or_endpoint_arrow(input)?);
    let stmt = PolicyStmt::Set { key, value };
    validate_policy_stmt_for_block(kind, &stmt)?;
    Ok(stmt)
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
                return Err(syn::Error::new(
                    input.span(),
                    "policy binds are not supported; declare parameters in the endpoint signature and assign with `\"key\" = ep.param`",
                ));
            }

            input.parse::<Token![=]>()?;
            let value: PolicyValue = parse_policy_value(input)?;
            return Ok(PolicyStmt::Set { key, value });
        }

        // ident start
        let ident: Ident = input.parse()?;

        // short bind removal
        if input.peek(Token![?]) || input.peek(Token![:]) {
            return Err(syn::Error::new(
                ident.span(),
                "policy parameter declarations are not supported; declare parameters in the endpoint signature and assign with `key = ep.param`",
            ));
        }

        let key = KeySpec::Ident(ident);

        if input.peek(Token![as]) {
            return Err(syn::Error::new(
                input.span(),
                "policy binds are not supported; declare parameters in the endpoint signature and assign with `key = ep.param`",
            ));
        }

        input.parse::<Token![=]>()?;
        let value: PolicyValue = parse_policy_value(input)?;
        Ok(PolicyStmt::Set { key, value })
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

impl Parse for RawIoSpec {
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

        let marker = Type::Path(tp.clone());
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

        let Some(last) = path.segments.last_mut() else {
            return Err(syn::Error::new_spanned(
                path,
                "codec spec expects a non-empty type path",
            ));
        };

        let had_angle_args = matches!(last.arguments, syn::PathArguments::AngleBracketed(_));
        let args: Vec<Type> = match &last.arguments {
            syn::PathArguments::AngleBracketed(ab) => {
                let mut out = Vec::new();

                for arg in ab.args.iter() {
                    match arg {
                        syn::GenericArgument::Type(t) => out.push(t.clone()),
                        _ => {
                            return Err(syn::Error::new_spanned(
                                arg,
                                "codec spec only supports type arguments: `Enc<T>`",
                            ));
                        }
                    }
                }

                out
            }
            syn::PathArguments::None => Vec::new(),
            other => {
                return Err(syn::Error::new_spanned(
                    other,
                    "codec spec expects angle-bracketed type arguments: `Enc<T>`",
                ));
            }
        };

        let ty: Type = args
            .first()
            .cloned()
            .unwrap_or_else(|| syn::parse_quote!(()));

        // Strip `<T>` from the encoding path so codegen can use `Decoded<Enc, T>`.
        last.arguments = syn::PathArguments::None;

        Ok(Self {
            marker,
            enc: path,
            ty,
            args,
            had_angle_args,
        })
    }
}

fn parse_fmt_spec(input: ParseStream<'_>) -> Result<FmtSpec> {
    let fmt_kw: kw::fmt = input.parse()?;
    let span = fmt_kw.span;
    let require_all = true;

    let content;
    bracketed!(content in input);

    let mut pieces: Vec<FmtPiece> = Vec::new();
    while !content.is_empty() {
        if content.peek(LitStr) {
            pieces.push(FmtPiece::Lit(content.parse::<LitStr>()?));
        } else if content.peek(kw::fmt) {
            return Err(syn::Error::new(
                content.span(),
                "nested fmt[...] is not allowed",
            ));
        } else if content.peek(token::Brace) {
            let inner;
            braced!(inner in content);
            if inner.peek(Ident) && inner.peek2(Token![.]) {
                let fork = inner.fork();
                let base: Ident = fork.parse()?;
                if (base == "vars" || base == "ep" || base == "secret") && fork.peek(Token![.]) {
                    let _dot: Token![.] = fork.parse()?;
                    let _name: Ident = fork.parse()?;
                    if fork.is_empty() {
                        let base: Ident = inner.parse()?;
                        inner.parse::<Token![.]>()?;
                        let name: Ident = inner.parse()?;
                        let scope = resolve_scoped_ref_base(&base)?;
                        if matches!(scope, RefScope::Auth) {
                            return Err(syn::Error::new(
                                name.span(),
                                "secret.* is not allowed in fmt[...]",
                            ));
                        }
                        pieces.push(FmtPiece::Ref(ScopedRef {
                            scope,
                            ident: name,
                            explicit: true,
                        }));
                        if !inner.is_empty() {
                            return Err(syn::Error::new(
                                inner.span(),
                                "unexpected tokens in scoped reference",
                            ));
                        }
                        let _ = content.parse::<Option<Token![,]>>()?;
                        continue;
                    }
                }
            }
            return Err(syn::Error::new(
                inner.span(),
                "template parameter declarations are not supported in `fmt[...]`; use identifier or scoped refs (`vars.x`, `ep.y`, `secret.z`)",
            ));
        } else if content.peek(Ident) {
            let sr = parse_scoped_ref_from_ident(&content)?;
            if matches!(sr.scope, RefScope::Auth) {
                return Err(syn::Error::new(
                    sr.ident.span(),
                    "secret.* is not allowed in fmt[...]",
                ));
            }
            pieces.push(FmtPiece::Ref(sr));
        } else {
            let tt: TokenTree = content.parse()?;
            return Err(syn::Error::new(
                tt.span(),
                "expected string literal, identifier, or scoped reference in `fmt[...]`",
            ));
        }
        let _ = content.parse::<Option<Token![,]>>()?;
    }

    if pieces.is_empty() {
        return Err(syn::Error::new(span, "fmt[...] requires at least one piece"));
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
    Ok(PolicyValue::Expr(input.parse()?))
}

fn parse_route_atom(input: ParseStream<'_>) -> Result<RouteAtom> {
    if input.peek(kw::fmt) {
        return Ok(RouteAtom::Fmt(parse_fmt_spec(input)?));
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
        // Prefer {vars.x}/{ep.y}/{secret.z} refs.
        if content.peek(Ident) && content.peek2(Token![.]) {
            let fork = content.fork();
            let base: Ident = fork.parse()?;
            if (base == "vars" || base == "ep" || base == "secret") && fork.peek(Token![.]) {
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
                    let scope = resolve_scoped_ref_base(&base)?;
                    return Ok(RouteAtom::Ref(ScopedRef {
                        scope,
                        ident: name,
                        explicit: true,
                    }));
                }
            }
        }
        return Err(syn::Error::new(
            content.span(),
            "route placeholder declarations are not supported; declare params in scope/endpoint signatures and reference them in route items",
        ));
    }
    let tt: proc_macro2::TokenTree = input.parse()?;
    Err(syn::Error::new(
        tt.span(),
        "expected string literal, identifier, scoped reference, or `fmt[...]` in route",
    ))
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

fn parse_path_route_expr_bracket(input: ParseStream<'_>) -> Result<RouteExpr> {
    let route = parse_route_expr_bracket(input)?;
    for atom in &route.atoms {
        if let RouteAtom::Fmt(spec) = atom {
            for piece in &spec.pieces {
                if let FmtPiece::Lit(lit) = piece && lit.value().contains('/') {
                    return Err(syn::Error::new(
                        lit.span(),
                        "path fmt string literals must not contain `/`; use separate path atoms",
                    ));
                }
            }
        }
    }
    Ok(route)
}

fn resolve_scoped_ref_base(base: &Ident) -> Result<RefScope> {
    if *base == "vars" {
        return Ok(RefScope::Cx);
    }
    if *base == "secret" {
        return Ok(RefScope::Auth);
    }
    if *base == "ep" {
        return Ok(RefScope::Ep);
    }
    if let Some(kind) = emit_helpers::public_expr_reserved_root_kind(base) {
        let message = match kind {
            emit_helpers::PublicExprForbiddenKind::Auth => {
                "auth references are not allowed in public route expressions; use an auth declaration/use instead"
            }
            emit_helpers::PublicExprForbiddenKind::Secret => {
                "secret references are only allowed in credential declarations"
            }
            emit_helpers::PublicExprForbiddenKind::GeneratedLocal => {
                "generated implementation locals are not part of the public DSL expression scope"
            }
            emit_helpers::PublicExprForbiddenKind::SecretExposure => {
                "secret exposure methods are not allowed in public route expressions"
            }
        };
        return Err(syn::Error::new(base.span(), message));
    }
    Err(syn::Error::new(
        base.span(),
        "unknown scoped reference prefix; expected `vars.` or `ep.`",
    ))
}

fn parse_scoped_ref_from_ident(input: ParseStream<'_>) -> Result<ScopedRef> {
    let first: Ident = input.parse()?;
    if input.peek(Token![.]) {
        input.parse::<Token![.]>()?;
        let second: Ident = input.parse()?;
        let scope = resolve_scoped_ref_base(&first)?;
        Ok(ScopedRef {
            scope,
            ident: second,
            explicit: true,
        })
    } else {
        Ok(ScopedRef {
            scope: RefScope::Ep,
            ident: first,
            explicit: false,
        })
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
