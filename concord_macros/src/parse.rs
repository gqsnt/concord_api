// concord_macros/src/parse.rs
use crate::ast::*;
use crate::kw;
use syn::parse::{Parse, ParseStream};
use syn::{braced, bracketed, token, Expr, Ident, LitStr, Path, Result, Token, Type};
use proc_macro2::{Span, TokenStream as TokenStream2, TokenTree};

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
        let mut policy = PolicyBlocks::default();

        while !content.is_empty() {
            if content.peek(kw::scheme) {
                content.parse::<kw::scheme>()?;
                content.parse::<Token![:]>()?;
                let v: Ident = content.parse()?;
                scheme = Some(match v.to_string().as_str() {
                    "http" => SchemeLit::Http,
                    "https" => SchemeLit::Https,
                    _ => return Err(syn::Error::new(v.span(), "scheme must be `http` or `https`")),
                });
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
                return Err(syn::Error::new(tt.span(), "unexpected token in client block"));
            }
        }

        let scheme = scheme.ok_or_else(|| syn::Error::new(name.span(), "missing `scheme:` in client"))?;
        let host = host.ok_or_else(|| syn::Error::new(name.span(), "missing `host:` in client"))?;

        Ok(Self {
            name,
            scheme,
            host,
            policy,
        })
    }
}

impl Parse for Item {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        if input.peek(kw::prefix) {
            Ok(Item::Layer(input.parse::<LayerDefTaggedPrefix>()?.0))
        } else if input.peek(kw::path) {
            Ok(Item::Layer(input.parse::<LayerDefTaggedPath>()?.0))
        } else {
            Ok(Item::Endpoint(input.parse::<EndpointDef>()?))
        }
    }
}

struct LayerDefTaggedPrefix(LayerDef);
struct LayerDefTaggedPath(LayerDef);

impl Parse for LayerDefTaggedPrefix {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        input.parse::<kw::prefix>()?;
        let route: RouteExpr = parse_route_expr_dot(input)?;
        let content;
        braced!(content in input);

        let mut policy = PolicyBlocks::default();
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
            } else if content.peek(kw::prefix) || content.peek(kw::path) {
                items.push(content.parse::<Item>()?);
            } else {
                // endpoint
                items.push(Item::Endpoint(content.parse::<EndpointDef>()?));
            }
        }

        Ok(Self(LayerDef {
            kind: LayerKind::Prefix,
            route,
            policy,
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
            } else if content.peek(kw::prefix) || content.peek(kw::path) {
                items.push(content.parse::<Item>()?);
            } else {
                items.push(Item::Endpoint(content.parse::<EndpointDef>()?));
            }
        }

        Ok(Self(LayerDef {
            kind: LayerKind::Path,
            route,
            policy,
            items,
        }))
    }
}

impl Parse for EndpointDef {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let method: Ident = input.parse()?;
        let name: Ident = input.parse()?;
        let route: RouteExpr = parse_route_expr_slash(input)?;

        let mut policy = PolicyBlocks::default();
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
                policy.timeout = Some(parse_expr_until_comma_or_endpoint_arrow(input)?);
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
                return Err(syn::Error::new(tt.span(), "unexpected token in endpoint; expected headers/query/timeout/paginate/body or `->`"));
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
            policy,
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
                        return Err(syn::Error::new(content.span(), "trailing `,` not allowed in paginate block"));
                    }
                } else {
                    return Err(syn::Error::new(content.span(), "expected `,` between paginate assignments"));
                }
            }
            let key: Ident = content.parse()?;
            content.parse::<Token![=]>()?;
            let value: Expr = content.parse()?;
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
        if kind == PolicyBlockKind::Headers {
            if let PolicyStmt::Set { op: SetOp::Push, .. } = &stmt {
                return Err(syn::Error::new(
                    stmt_span(&stmt),
                    "`+=` is not allowed in `headers {}` blocks (query-only operator)",
                ));
            }
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
            return Err(syn::Error::new(tt.span(), "expected `,` between policy statements"));
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
                decl: VarDeclShort { optional, ty, default },
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
            let b = content.parse::<Braced<TemplateVarDecl>>()?;
            pieces.push(FmtPiece::Var(b.inner));
        } else {
            let tt: TokenTree = content.parse()?;
            return Err(syn::Error::new(tt.span(), "expected string literal or `{var:Ty}` in fmt[...]"));
        }
        let _ = content.parse::<Option<Token![,]>>()?;
    }

    Ok(FmtSpec { span, require_all, pieces })
}



fn parse_policy_value(input: syn::parse::ParseStream<'_>) -> Result<PolicyValue> {
    if input.peek(kw::fmt) {
        return Ok(PolicyValue::Fmt(parse_fmt_spec(input)?));
    }

    Ok(PolicyValue::Expr(input.parse::<syn::Expr>()?))
}


fn parse_route_atom(input: ParseStream<'_>) -> Result<RouteAtom> {
    if input.peek(kw::fmt) {
        return Ok(RouteAtom::Fmt(parse_fmt_spec(input)?));
    }
    if input.peek(LitStr) {
        return Ok(RouteAtom::Static(input.parse::<LitStr>()?));
    }
    if input.peek(token::Brace) {
        let b = input.parse::<Braced<TemplateVarDecl>>()?;
        return Ok(RouteAtom::Var(b.inner));
    }
    let tt: proc_macro2::TokenTree = input.parse()?;
    Err(syn::Error::new(tt.span(), "expected string literal or `{var:Ty}` in route"))
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
