// concord_macros/src/parse.rs
use crate::ast::*;
use crate::kw;
use syn::parse::{Parse, ParseStream};
use syn::{braced, bracketed, token, Expr, Ident, LitStr, Path, Result, Token, Type};
use proc_macro2::{TokenStream as TokenStream2, TokenTree};


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
                let _ = content.parse::<Option<Token![;]>>()?;
            } else if content.peek(kw::host) {
                content.parse::<kw::host>()?;
                content.parse::<Token![:]>()?;
                host = Some(content.parse::<LitStr>()?);
                let _ = content.parse::<Option<Token![,]>>()?;
                let _ = content.parse::<Option<Token![;]>>()?;
            } else if content.peek(kw::headers) {
                policy.headers = Some(content.parse::<PolicyBlockTaggedHeaders>()?.0);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::query) {
                policy.query = Some(content.parse::<PolicyBlockTaggedQuery>()?.0);
                let _ = content.parse::<Option<Token![,]>>()?;
            } else if content.peek(kw::timeout) {
                content.parse::<kw::timeout>()?;
                content.parse::<Token![:]>()?;
                policy.timeout = Some(parse_expr_until_delim(&content)?);
                let _ = content.parse::<Option<Token![,]>>()?;
                let _ = content.parse::<Option<Token![;]>>()?;
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
        let template: LitStr = input.parse()?;
        let content;
        braced!(content in input);

        let mut policy = PolicyBlocks::default();
        let mut items = Vec::new();

        while !content.is_empty() {
            if content.peek(kw::headers) {
                policy.headers = Some(content.parse::<PolicyBlockTaggedHeaders>()?.0);
            } else if content.peek(kw::query) {
                policy.query = Some(content.parse::<PolicyBlockTaggedQuery>()?.0);
            } else if content.peek(kw::timeout) {
                content.parse::<kw::timeout>()?;
                content.parse::<Token![:]>()?;
                policy.timeout = Some(parse_expr_until_delim(&content)?);
                let _ = content.parse::<Option<Token![,]>>()?;
                let _ = content.parse::<Option<Token![;]>>()?;
            } else if content.peek(kw::prefix) || content.peek(kw::path) {
                items.push(content.parse::<Item>()?);
            } else {
                // endpoint
                items.push(Item::Endpoint(content.parse::<EndpointDef>()?));
            }
        }

        Ok(Self(LayerDef {
            kind: LayerKind::Prefix,
            template,
            policy,
            items,
        }))
    }
}

impl Parse for LayerDefTaggedPath {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        input.parse::<kw::path>()?;
        let template: LitStr = input.parse()?;
        let content;
        braced!(content in input);

        let mut policy = PolicyBlocks::default();
        let mut items = Vec::new();

        while !content.is_empty() {
            if content.peek(kw::headers) {
                policy.headers = Some(content.parse::<PolicyBlockTaggedHeaders>()?.0);
            } else if content.peek(kw::query) {
                policy.query = Some(content.parse::<PolicyBlockTaggedQuery>()?.0);
            } else if content.peek(kw::timeout) {
                content.parse::<kw::timeout>()?;
                content.parse::<Token![:]>()?;
                policy.timeout = Some(parse_expr_until_delim(&content)?);
                let _ = content.parse::<Option<Token![,]>>()?;
                let _ = content.parse::<Option<Token![;]>>()?;
            } else if content.peek(kw::prefix) || content.peek(kw::path) {
                items.push(content.parse::<Item>()?);
            } else {
                items.push(Item::Endpoint(content.parse::<EndpointDef>()?));
            }
        }

        Ok(Self(LayerDef {
            kind: LayerKind::Path,
            template,
            policy,
            items,
        }))
    }
}

impl Parse for EndpointDef {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let method: Ident = input.parse()?;
        let name: Ident = input.parse()?;
        let route: LitStr = input.parse()?;

        let mut policy = PolicyBlocks::default();
        let mut paginate: Option<PaginateSpec> = None;
        let mut body: Option<CodecSpec> = None;

        // parse endpoint parts until `->`
        while !input.peek(Token![->]) {
            if input.peek(kw::headers) {
                policy.headers = Some(input.parse::<PolicyBlockTaggedHeaders>()?.0);
            } else if input.peek(kw::query) {
                policy.query = Some(input.parse::<PolicyBlockTaggedQuery>()?.0);
            } else if input.peek(kw::timeout) {
                input.parse::<kw::timeout>()?;
                input.parse::<Token![:]>()?;
                policy.timeout = Some(parse_expr_until_delim(input)?);
                let _ = input.parse::<Option<Token![,]>>()?;
                let _ = input.parse::<Option<Token![;]>>()?;
            } else if input.peek(kw::paginate) {
                if paginate.is_some() {
                    return Err(syn::Error::new(name.span(), "duplicate `paginate`"));
                }
                paginate = Some(input.parse::<PaginateSpec>()?);
            } else if input.peek(kw::body) {
                if body.is_some() {
                    return Err(syn::Error::new(name.span(), "duplicate `body`"));
                }
                input.parse::<kw::body>()?;
                body = Some(input.parse::<CodecSpec>()?);
                let _ = input.parse::<Option<Token![;]>>()?;
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

        let semi: token::Semi = input.parse()?;

        Ok(Self {
            method,
            name,
            route,
            policy,
            paginate,
            body,
            response,
            map,
            semi,
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
        while !content.is_empty() {
            let key: Ident = content.parse()?;
            content.parse::<Token![=]>()?;
            let value: Expr = content.parse()?;
            content.parse::<Token![;]>()?;
            assigns.push(PaginateAssign { key, value });
        }

        Ok(Self { ctrl_ty, assigns })
    }
}

struct PolicyBlockTaggedHeaders(PolicyBlock);
struct PolicyBlockTaggedQuery(PolicyBlock);

impl Parse for PolicyBlockTaggedHeaders {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        input.parse::<kw::headers>()?;
        Ok(Self(parse_policy_block(input)?))
    }
}

impl Parse for PolicyBlockTaggedQuery {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        input.parse::<kw::query>()?;
        Ok(Self(parse_policy_block(input)?))
    }
}

fn parse_policy_block(input: ParseStream<'_>) -> Result<PolicyBlock> {
    let content;
    braced!(content in input);
    let mut stmts = Vec::new();
    while !content.is_empty() {
        stmts.push(content.parse::<PolicyStmt>()?);
        content.parse::<Token![;]>()?;
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


fn parse_expr_until_delim(input: ParseStream<'_>) -> Result<Expr> {
    let mut ts = TokenStream2::new();
    while !input.is_empty()
        && !input.peek(Token![->])
        && !input.peek(Token![,])
        && !input.peek(Token![;])
    {
        let tt: TokenTree = input.parse()?;
        ts.extend(std::iter::once(tt));
    }
    if ts.is_empty() {
        return Err(syn::Error::new(input.span(), "expected an expression"));
    }
    syn::parse2::<Expr>(ts)
}


fn parse_policy_value(input: syn::parse::ParseStream<'_>) -> Result<PolicyValue> {
    if input.peek(kw::fmt) {
        input.parse::<kw::fmt>()?;
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

        return Ok(PolicyValue::Fmt(FmtSpec { require_all, pieces }));
    }

    Ok(PolicyValue::Expr(input.parse::<syn::Expr>()?))
}