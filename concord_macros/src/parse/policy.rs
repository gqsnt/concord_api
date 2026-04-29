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
    }
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

fn parse_query_policy_stmt(input: ParseStream<'_>) -> Result<PolicyStmt> {
    if input.peek(Ident) {
        let fork = input.fork();
        let ident: Ident = fork.parse()?;
        if !fork.peek(Token![=])
            && !fork.peek(Token![+=])
            && !fork.peek(Token![:])
            && !fork.peek(Token![?])
            && !fork.peek(Token![as])
        {
            input.parse::<Ident>()?;
            let value: Expr = syn::parse_quote!(#ident);
            return Ok(PolicyStmt::Set {
                key: KeySpec::Ident(ident),
                value: PolicyValue::Expr(normalize_policy_expr(value)),
                op: SetOp::Set,
            });
        }
    }
    input.parse()
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

    let op = if input.peek(Token![+=]) {
        if kind == PolicyBlockKind::Headers {
            return Err(syn::Error::new(
                input.span(),
                "`+=` is not allowed for singular `header` policy",
            ));
        }
        input.parse::<Token![+=]>()?;
        SetOp::Push
    } else {
        input.parse::<Token![=]>()?;
        SetOp::Set
    };
    let value = PolicyValue::Expr(normalize_policy_expr(parse_expr_until_comma_or_endpoint_arrow(
        input,
    )?));
    Ok(PolicyStmt::Set { key, value, op })
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
                if (base == "vars" || base == "ep" || base == "secret") && fork.peek(Token![.]) {
                    let _dot: Token![.] = fork.parse()?;
                    let _name: Ident = fork.parse()?;
                    if fork.is_empty() {
                        let base: Ident = inner.parse()?;
                        inner.parse::<Token![.]>()?;
                        let name: Ident = inner.parse()?;
                        let scope = resolve_scoped_ref_base(&base)?;
                        pieces.push(FmtPiece::Ref(ScopedRef { scope, ident: name }));
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
    let expr: syn::Expr = input.parse()?;
    Ok(PolicyValue::Expr(normalize_policy_expr(expr)))
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
                    return Ok(RouteAtom::Ref(ScopedRef { scope, ident: name }));
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
    Err(syn::Error::new(
        base.span(),
        "unknown scoped reference prefix; expected `vars.`, `ep.`, or `secret.`",
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
        })
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
                } else if *b == "ep" {
                    b.clone()
                } else {
                    return Expr::Field(syn::ExprField {
                        attrs: f.attrs,
                        base: Box::new(normalize_policy_expr(*f.base)),
                        dot_token: f.dot_token,
                        member: f.member,
                    });
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
