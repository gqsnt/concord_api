use proc_macro2::{TokenStream as TokenStream2, TokenTree};
use syn::parse::{Parse, ParseStream};
use syn::{Expr, Ident, LitStr, Path, Result, Token, Type, braced, bracketed};

syn::custom_keyword!(client);
syn::custom_keyword!(scheme);
syn::custom_keyword!(host);
syn::custom_keyword!(params);
syn::custom_keyword!(headers);
syn::custom_keyword!(prefix);
syn::custom_keyword!(path);
syn::custom_keyword!(query);
syn::custom_keyword!(body);
syn::custom_keyword!(paginate);
syn::custom_keyword!(timeout);
syn::custom_keyword!(partial);

#[derive(Debug)]
pub struct ApiFile {
    pub client: ClientDecl,
    pub tree: Vec<Node>,
}
impl Parse for ApiFile {
    fn parse(input: ParseStream) -> Result<Self> {
        let client: ClientDecl = input.parse()?;
        let mut tree = Vec::new();
        while !input.is_empty() {
            tree.push(input.parse()?);
        }
        Ok(Self { client, tree })
    }
}

#[derive(Debug)]
pub struct ClientDecl {
    pub name: Ident,
    pub scheme: Option<Ident>,
    pub host: LitStr,
    pub params: Vec<ParamDecl>,
    pub headers: Vec<HeaderRule>,
    pub timeout: Option<Expr>,
}
impl Parse for ClientDecl {
    fn parse(input: ParseStream) -> Result<Self> {
        input.parse::<client>()?;
        let name: Ident = input.parse()?;
        let content;
        braced!(content in input);

        let mut scheme_v: Option<Ident> = None;
        let mut host_v: Option<LitStr> = None;
        let mut timeout_v: Option<Expr> = None;
        let mut params_v: Vec<ParamDecl> = Vec::new();
        let mut headers_v: Vec<HeaderRule> = Vec::new();

        while !content.is_empty() {
            if content.peek(scheme) {
                content.parse::<scheme>()?;
                content.parse::<Token![:]>()?;
                scheme_v = Some(content.parse()?);
                let _ = content.parse::<Token![,]>();
                continue;
            }
            if content.peek(host) {
                content.parse::<host>()?;
                content.parse::<Token![:]>()?;
                host_v = Some(content.parse()?);
                let _ = content.parse::<Token![,]>();
                continue;
            }
            if content.peek(timeout) {
                content.parse::<timeout>()?;
                content.parse::<Token![:]>()?;
                if timeout_v.is_some() {
                    return Err(content.error("v1.7: duplicate client.timeout"));
                }
                timeout_v = Some(content.parse::<Expr>()?);
                let _ = content.parse::<Token![,]>();
                continue;
            }
            if content.peek(params) {
                let pb: ParamsBlock = content.parse()?;
                params_v = pb.params;
                continue;
            }
            if content.peek(headers) {
                let hb: HeadersBlock = content.parse()?;
                headers_v = hb.rules;
                continue;
            }
            return Err(content.error("v1.5: unexpected token in client block"));
        }

        let host =
            host_v.ok_or_else(|| syn::Error::new_spanned(&name, "client.host is required"))?;
        Ok(Self {
            name,
            scheme: scheme_v,
            host,
            params: params_v,
            headers: headers_v,
            timeout: timeout_v,
        })
    }
}

// -------------------- Placeholder --------------------

#[derive(Debug, Clone)]
pub struct PlaceholderDecl {
    pub name: Ident,
    pub alias: Option<Ident>,
    pub optional: bool,        // '?'
    pub ty: Type,              // default String if omitted
    pub ty_explicit: bool,     // ':' present
    pub default: Option<Expr>, // '= expr'
}
impl PlaceholderDecl {
    #[inline]
    pub fn is_decl(&self) -> bool {
        self.ty_explicit || self.optional || self.default.is_some() || self.alias.is_some()
    }
}
impl Parse for PlaceholderDecl {
    fn parse(input: ParseStream) -> Result<Self> {
        let name: Ident = input.parse()?;
        let alias = if input.peek(Token![as]) {
            input.parse::<Token![as]>()?;
            Some(input.parse::<Ident>()?)
        } else {
            None
        };
        let optional = if input.peek(Token![?]) {
            input.parse::<Token![?]>()?;
            true
        } else {
            false
        };

        let (ty, ty_explicit) = if input.peek(Token![:]) {
            input.parse::<Token![:]>()?;
            (input.parse::<Type>()?, true)
        } else {
            (syn::parse_str::<Type>("String")?, false)
        };

        let default = if input.peek(Token![=]) {
            input.parse::<Token![=]>()?;
            Some(input.parse::<Expr>()?)
        } else {
            None
        };

        let is_decl = ty_explicit || optional || default.is_some();
        if is_decl && !ty_explicit {
            return Err(syn::Error::new(
                name.span(),
                "v1.5: declared placeholders must specify an explicit type (use `{name: Type}`)",
            ));
        }
        if alias.is_some() && !ty_explicit {
            return Err(syn::Error::new(
                name.span(),
                "v2: placeholder alias requires an explicit type (use `{name as alias: Type}`)",
            ));
        }

        Ok(Self {
            name,
            alias,
            optional,
            ty,
            ty_explicit,
            default,
        })
    }
}

// -------------------- Template atoms --------------------

#[derive(Debug, Clone)]
pub enum Atom {
    Lit(LitStr),
    Param(Box<PlaceholderDecl>), // {decl} ou {ref}
    Ref(RefExpr),                // ref sans accolades (ex: user_agent)
}

#[derive(Debug, Clone)]
pub struct SegmentExpr {
    pub atoms: Vec<Atom>, // concatenation by adjacency
}

#[derive(Copy, Clone, Debug)]
pub enum RouteMode {
    Host, // separator: .
    Path, // separator: /
}

#[derive(Debug, Clone)]
pub enum RouteExpr {
    Static(LitStr),
    Segments(Vec<SegmentExpr>), // segment list
}

fn ensure_static_route_lit(mode: RouteMode, lit: &LitStr) -> Result<()> {
    let s = lit.value();

    if s.contains('{') || s.contains('}') {
        return Err(syn::Error::new(
            lit.span(),
            "v1.5: string literals must be static; use `{placeholder}` tokens instead of braces inside strings",
        ));
    }

    if matches!(mode, RouteMode::Host) && (s.contains('.') || s.contains('/')) {
        return Err(syn::Error::new(
            lit.span(),
            "v1.5: host labels must not contain '.' or '/'; use `.` as the prefix separator",
        ));
    }

    Ok(())
}

impl RouteExpr {
    pub fn parse_host_expr(input: ParseStream) -> Result<Self> {
        parse_route_expr_mode(input, RouteMode::Host)
    }
    pub fn parse_path_expr(input: ParseStream) -> Result<Self> {
        parse_route_expr_mode(input, RouteMode::Path)
    }
}

fn parse_route_expr_mode(input: ParseStream, mode: RouteMode) -> Result<RouteExpr> {
    if input.peek(syn::token::Bracket) {
        return parse_route_segments_bracket(input, mode);
    }

    // inline form: atoms separated by '.' (host) or '/' (path)
    let segs = parse_route_chain(input, mode)?;

    // small normalization: single literal atom => Static
    if segs.len() == 1
        && let [Atom::Lit(lit)] = segs[0].atoms.as_slice()
    {
        return Ok(RouteExpr::Static(lit.clone()));
    }

    Ok(RouteExpr::Segments(segs))
}

fn parse_route_segments_bracket(input: ParseStream, mode: RouteMode) -> Result<RouteExpr> {
    let content;
    bracketed!(content in input);

    if content.is_empty() {
        return Ok(RouteExpr::Segments(Vec::new()));
    }

    let mut segments: Vec<SegmentExpr> = Vec::new();
    while !content.is_empty() {
        let mut atoms: Vec<Atom> = Vec::new();
        while !content.is_empty() && !content.peek(Token![,]) {
            if content.peek(LitStr) {
                let lit: LitStr = content.parse()?;
                ensure_static_route_lit(mode, &lit)?;
                // Bracket route form is the "strict segments" API:
                // each item is a single segment, so literals must not smuggle '/'.
                if matches!(mode, RouteMode::Path) && lit.value().contains('/') {
                    return Err(syn::Error::new(
                        lit.span(),
                        "v1.6: bracket path literals are strict segments and must not contain '/'; use the inline path form for raw injection or split the segment",
                    ));
                }
                atoms.push(Atom::Lit(lit));
            } else if content.peek(syn::token::Brace) {
                let inner;
                braced!(inner in content);
                let ph: PlaceholderDecl = inner.parse()?;
                if ph.optional {
                    return Err(syn::Error::new(
                        ph.name.span(),
                        "v1.5: route placeholders cannot be optional (`?`)",
                    ));
                }
                if !inner.is_empty() {
                    return Err(inner.error("v1.5: unexpected tokens in `{placeholder}`"));
                }
                atoms.push(Atom::Param(Box::new(ph)));
            } else {
                return Err(content
                    .error("v1.5: expected string literal or `{placeholder}` inside `[...]`"));
            }
        }

        if atoms.is_empty() {
            return Err(content.error("v1.5: empty segment in `[...]`"));
        }

        segments.push(SegmentExpr { atoms });

        if content.peek(Token![,]) {
            content.parse::<Token![,]>()?;
        }
    }

    Ok(RouteExpr::Segments(segments))
}

fn parse_ref_expr(input: syn::parse::ParseStream<'_>) -> syn::Result<RefExpr> {
    let head: syn::Ident = input.parse()?;
    let head_s = head.to_string();
    let is_scoped =
        (head_s == "vars" || head_s == "ep") && (input.peek(Token![.]) || input.peek(Token![::]));
    if is_scoped {
        let scope = if head_s == "vars" {
            RefScope::Vars
        } else {
            RefScope::Ep
        };
        if input.peek(Token![.]) {
            let _ = input.parse::<Token![.]>()?;
        } else {
            let _ = input.parse::<Token![::]>()?;
        }
        let name: syn::Ident = input.parse()?;
        Ok(RefExpr {
            scope: Some(scope),
            name,
        })
    } else {
        Ok(RefExpr {
            scope: None,
            name: head,
        })
    }
}

fn parse_route_chain(input: ParseStream, mode: RouteMode) -> Result<Vec<SegmentExpr>> {
    let mut segments: Vec<SegmentExpr> = Vec::new();

    // first segment
    segments.push(parse_route_chain_segment(input, mode)?);

    loop {
        let has_sep = match mode {
            RouteMode::Host => input.peek(Token![.]),
            RouteMode::Path => input.peek(Token![/]),
        };
        if !has_sep {
            break;
        }

        match mode {
            RouteMode::Host => {
                input.parse::<Token![.]>()?;
            }
            RouteMode::Path => {
                input.parse::<Token![/]>()?;
            }
        }

        segments.push(parse_route_chain_segment(input, mode)?);
    }

    Ok(segments)
}

fn parse_route_chain_segment(input: ParseStream, mode: RouteMode) -> Result<SegmentExpr> {
    let mut atoms: Vec<Atom> = Vec::new();

    loop {
        if input.peek(LitStr) {
            let lit: LitStr = input.parse()?;
            ensure_static_route_lit(mode, &lit)?;
            atoms.push(Atom::Lit(lit));
            continue;
        }

        if let Some(ph) = try_parse_placeholder_decl(input)? {
            if ph.optional {
                return Err(syn::Error::new(
                    ph.name.span(),
                    "v1.5: route placeholders cannot be optional (`?`)",
                ));
            }
            atoms.push(Atom::Param(Box::new(ph)));
            continue;
        }

        break;
    }

    if atoms.is_empty() {
        Err(input.error("v1.5: expected a route segment (string literal or `{placeholder}`)"))
    } else {
        Ok(SegmentExpr { atoms })
    }
}

// Only consume `{ ... }` if the content parses *exactly* as a PlaceholderDecl.
// Otherwise, it is treated as the start of a `{ ... }` block by the caller.
fn try_parse_placeholder_decl(input: ParseStream) -> Result<Option<PlaceholderDecl>> {
    if !input.peek(syn::token::Brace) {
        return Ok(None);
    }

    let ahead = input.fork();
    let inner_ahead;
    braced!(inner_ahead in ahead);

    let ph: PlaceholderDecl = match inner_ahead.parse() {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };

    if !inner_ahead.is_empty() {
        return Ok(None);
    }

    // commit
    let inner;
    braced!(inner in input);
    let ph2: PlaceholderDecl = inner.parse()?;
    if !inner.is_empty() {
        return Err(inner.error("v1.5: unexpected tokens in `{placeholder}`"));
    }
    // keep the parsed value from the real stream (ph2)
    let _ = ph; // silence unused on ahead
    Ok(Some(ph2))
}

// -------------------- v1.5 header/query RHS --------------------

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RefScope {
    Vars,
    Ep,
}

#[derive(Clone, Debug)]
pub struct RefExpr {
    pub scope: Option<RefScope>,
    pub name: syn::Ident,
}

impl From<Ident> for RefExpr {
    fn from(name: Ident) -> Self {
        Self { scope: None, name }
    }
}

impl syn::parse::Parse for RefExpr {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        parse_ref_expr(input)
    }
}

#[derive(Debug, Clone)]
pub enum ValueExpr {
    Lit(LitStr),                                   // "testHeader"
    Ref(RefExpr),                                  // user_agent
    Decl(Box<PlaceholderDecl>),                    // debug?: bool = true
    Format { atoms: Vec<Atom>, mode: FormatMode }, // ["key-", ...] or partial([...])
}

#[derive(Copy, Clone, Debug)]
pub enum FormatMode {
    GateEntry,
    Partial,
}

fn parse_format_bracket(input: ParseStream) -> Result<Vec<Atom>> {
    let content;
    bracketed!(content in input);

    // `[]` => empty string
    if content.is_empty() {
        return Ok(Vec::new());
    }

    let mut out: Vec<Atom> = Vec::new();
    while !content.is_empty() {
        while !content.is_empty() && !content.peek(Token![,]) {
            if content.peek(LitStr) {
                out.push(Atom::Lit(content.parse()?));
            } else if content.peek(Ident) {
                out.push(Atom::Ref(content.parse()?));
            } else if content.peek(syn::token::Brace) {
                let inner;
                braced!(inner in content);
                let ph: PlaceholderDecl = inner.parse()?;
                if !inner.is_empty() {
                    return Err(inner.error("v1.5: unexpected tokens in `{placeholder}`"));
                }
                out.push(Atom::Param(Box::new(ph)));
            } else {
                return Err(content
                    .error("v1.5: expected string literal or `{placeholder}` inside `[...]`"));
            }
        }
        if content.peek(Token![,]) {
            content.parse::<Token![,]>()?;
        }
    }

    Ok(out)
}

fn parse_value_expr(input: ParseStream) -> Result<ValueExpr> {
    if input.peek(partial) {
        input.parse::<partial>()?;
        let atoms = parse_format_bracket(input)?;
        return Ok(ValueExpr::Format {
            atoms,
            mode: FormatMode::Partial,
        });
    }
    if input.peek(syn::token::Bracket) {
        return Ok(ValueExpr::Format {
            atoms: parse_format_bracket(input)?,
            mode: FormatMode::GateEntry,
        });
    }
    if input.peek(LitStr) {
        return Ok(ValueExpr::Lit(input.parse()?));
    }
    if input.peek(syn::token::Brace) {
        let inner;
        braced!(inner in input);
        let ph: PlaceholderDecl = inner.parse()?;
        if !inner.is_empty() {
            return Err(inner.error("unexpected tokens in `{...}`"));
        }
        return Ok(if ph.is_decl() {
            ValueExpr::Decl(Box::new(ph))
        } else {
            ValueExpr::Ref(RefExpr {
                scope: None,
                name: ph.name,
            })
        });
    }
    // ident: decl si suivi de '?' ou ':' (sinon ref)
    if input.peek(Ident)
        && (input.peek2(Token![?]) || input.peek2(Token![:]) || input.peek2(Token![as]))
    {
        let ph: PlaceholderDecl = input.parse()?;
        return Ok(ValueExpr::Decl(Box::new(ph)));
    }
    Ok(ValueExpr::Ref(input.parse()?))
}

// -------------------- Params --------------------

#[derive(Debug)]
pub struct ParamsBlock {
    pub params: Vec<ParamDecl>,
}
impl Parse for ParamsBlock {
    fn parse(input: ParseStream) -> Result<Self> {
        input.parse::<params>()?;
        let content;
        braced!(content in input);

        let mut out = Vec::new();
        while !content.is_empty() {
            out.push(content.parse()?);
            if content.peek(Token![;]) {
                content.parse::<Token![;]>()?;
            } else {
                let _ = content.parse::<Token![,]>();
            }
        }

        Ok(Self { params: out })
    }
}

#[derive(Debug, Clone)]
pub struct ParamDecl {
    pub name: Ident,
    pub optional: bool, // name?: T
    pub ty: Type,
    pub default: Option<Expr>, // name: T = expr
}
impl Parse for ParamDecl {
    fn parse(input: ParseStream) -> Result<Self> {
        let name: Ident = input.parse()?;
        let optional = if input.peek(Token![?]) {
            input.parse::<Token![?]>()?;
            true
        } else {
            false
        };
        input.parse::<Token![:]>()?;
        let ty: Type = input.parse()?;
        let default = if input.peek(Token![=]) {
            input.parse::<Token![=]>()?;
            Some(input.parse::<Expr>()?)
        } else {
            None
        };
        Ok(Self {
            name,
            optional,
            ty,
            default,
        })
    }
}

// -------------------- Headers --------------------

#[derive(Debug)]
pub struct HeadersBlock {
    pub rules: Vec<HeaderRule>,
}
impl Parse for HeadersBlock {
    fn parse(input: ParseStream) -> Result<Self> {
        input.parse::<headers>()?;
        let content;
        braced!(content in input);

        let mut rules = Vec::new();
        while !content.is_empty() {
            rules.push(content.parse()?);
            if content.peek(Token![;]) {
                content.parse::<Token![;]>()?;
            } else {
                let _ = content.parse::<Token![,]>();
            }
        }

        Ok(Self { rules })
    }
}

#[derive(Debug, Clone)]
pub enum HeaderRule {
    Remove { name: LitStr },
    Set { name: LitStr, value: Box<ValueExpr> }, // "k" => v | "k": v | "k"=v | ident = v | keyless decl/ref
}

impl Parse for HeaderRule {
    fn parse(input: ParseStream) -> Result<Self> {
        fn key_from_header_ident(id: &Ident) -> LitStr {
            // snake_case -> kebab-case + lowercase
            let s = id.to_string().replace('_', "-").to_ascii_lowercase();
            LitStr::new(&s, id.span())
        }
        fn parse_after_key_sep(input: ParseStream) -> Result<ValueExpr> {
            if input.peek(Token![=>]) {
                input.parse::<Token![=>]>()?;
            } else if input.peek(Token![:]) {
                input.parse::<Token![:]>()?;
            } else if input.peek(Token![=]) {
                input.parse::<Token![=]>()?;
            } else {
                return Err(input.error("expected `=>`, `:` or `=`"));
            }
            parse_value_expr(input)
        }
        fn parse_braced_placeholder_as_value(input: ParseStream) -> Result<ValueExpr> {
            let inner;
            braced!(inner in input);
            let ph: PlaceholderDecl = inner.parse()?;
            if !inner.is_empty() {
                return Err(inner.error("unexpected tokens in `{...}`"));
            }
            Ok(if ph.is_decl() {
                ValueExpr::Decl(Box::new(ph))
            } else {
                ValueExpr::Ref(RefExpr {
                    scope: None,
                    name: ph.name,
                })
            })
        }
        if input.peek(Token![-]) {
            input.parse::<Token![-]>()?;
            let n: LitStr = if input.peek(LitStr) {
                input.parse()?
            } else if input.peek(Ident) {
                let id: Ident = input.parse()?;
                key_from_header_ident(&id)
            } else {
                return Err(input.error("expected header name after `-`"));
            };
            return Ok(HeaderRule::Remove { name: n });
        }
        if input.peek(syn::token::Brace) {
            let v = parse_braced_placeholder_as_value(input)?;
            let key = match &v {
                ValueExpr::Decl(d) => key_from_header_ident(&d.name),
                ValueExpr::Ref(r) => key_from_header_ident(&r.name),
                _ => return Err(input.error("invalid keyless header entry")),
            };
            return Ok(HeaderRule::Set {
                name: key,
                value: Box::new(v),
            });
        }
        if input.peek(Ident) && (input.peek2(Token![?]) || input.peek2(Token![:])) {
            // inline decl: debug?: bool = true
            let decl: PlaceholderDecl = input.parse()?;
            let key = key_from_header_ident(&decl.name);
            return Ok(HeaderRule::Set {
                name: key,
                value: Box::new(ValueExpr::Decl(Box::new(decl))),
            });
        }

        // explicit key: "x-debug" => <value>
        if input.peek(LitStr) {
            let name: LitStr = input.parse()?;
            let value = if input.peek(syn::token::Brace) {
                // sugar: "x" {debug?: bool=true}
                parse_braced_placeholder_as_value(input)?
            } else {
                parse_after_key_sep(input)?
            };
            return Ok(HeaderRule::Set {
                name,
                value: Box::new(value),
            });
        }

        // ident form: attr = <value> OR keyless ref: attr
        if input.peek(Ident) {
            let id: Ident = input.parse()?;
            let name = key_from_header_ident(&id);
            if input.peek(Token![=]) {
                input.parse::<Token![=]>()?;
                let value = parse_value_expr(input)?;
                return Ok(HeaderRule::Set {
                    name,
                    value: Box::new(value),
                });
            }
            // keyless ref: debug_flag
            return Ok(HeaderRule::Set {
                name,
                value: Box::new(ValueExpr::Ref(RefExpr {
                    scope: None,
                    name: id,
                })),
            });
        }

        Err(input.error("invalid header entry"))
    }
}

// -------------------- Query --------------------

#[derive(Debug)]
pub struct QueryBlock {
    pub items: Vec<QueryEntry>,
}
impl Parse for QueryBlock {
    fn parse(input: ParseStream) -> Result<Self> {
        input.parse::<query>()?;
        let content;
        braced!(content in input);
        let mut items = Vec::new();
        while !content.is_empty() {
            items.push(content.parse()?);
            if content.peek(Token![,]) {
                content.parse::<Token![,]>()?;
            }
        }
        Ok(Self { items })
    }
}

#[derive(Debug, Clone)]
pub enum QueryEntry {
    Remove { key: LitStr },
    Set { key: LitStr, value: ValueExpr }, // override-by-key (default)
    Push { key: LitStr, value: ValueExpr }, // allow duplicates
}

impl Parse for QueryEntry {
    fn parse(input: ParseStream) -> Result<Self> {
        fn key_from_query_ident(id: &Ident) -> LitStr {
            LitStr::new(&id.to_string(), id.span())
        }

        fn parse_after_key_sep(input: ParseStream) -> Result<ValueExpr> {
            if input.peek(Token![=>]) {
                input.parse::<Token![=>]>()?;
            } else if input.peek(Token![:]) {
                input.parse::<Token![:]>()?;
            } else if input.peek(Token![=]) {
                input.parse::<Token![=]>()?;
            } else {
                return Err(input.error("expected `=>`, `:` or `=`"));
            }
            parse_value_expr(input)
        }

        fn parse_braced_placeholder_as_value(input: ParseStream) -> Result<ValueExpr> {
            let inner;
            braced!(inner in input);
            let ph: PlaceholderDecl = inner.parse()?;
            if !inner.is_empty() {
                return Err(inner.error("unexpected tokens in `{...}`"));
            }
            Ok(if ph.is_decl() {
                ValueExpr::Decl(Box::new(ph))
            } else {
                ValueExpr::Ref(RefExpr {
                    scope: None,
                    name: ph.name,
                })
            })
        }

        // removal: - "k" | - ident
        if input.peek(Token![-]) {
            input.parse::<Token![-]>()?;
            let key: LitStr = if input.peek(LitStr) {
                input.parse()?
            } else if input.peek(Ident) {
                let id: Ident = input.parse()?;
                key_from_query_ident(&id)
            } else {
                return Err(input.error("expected query key after `-`"));
            };
            return Ok(QueryEntry::Remove { key });
        }

        // keyless entry: {decl/ref} or decl without braces => default Set
        if input.peek(syn::token::Brace) {
            let v = parse_braced_placeholder_as_value(input)?;
            let key = match &v {
                ValueExpr::Decl(d) => key_from_query_ident(&d.name),
                ValueExpr::Ref(r) => key_from_query_ident(&r.name),
                _ => return Err(input.error("invalid keyless query entry")),
            };
            return Ok(QueryEntry::Set { key, value: v });
        }

        if input.peek(Ident) && (input.peek2(Token![?]) || input.peek2(Token![:])) {
            let decl: PlaceholderDecl = input.parse()?;
            let key = key_from_query_ident(&decl.name);
            return Ok(QueryEntry::Set {
                key,
                value: ValueExpr::Decl(Box::new(decl)),
            });
        }

        // explicit key: "k" ... or ident ...
        if input.peek(LitStr) {
            let key: LitStr = input.parse()?;

            if input.peek(Token![+=]) {
                input.parse::<Token![+=]>()?;
                let value = parse_value_expr(input)?;
                return Ok(QueryEntry::Push { key, value });
            }

            let value = if input.peek(syn::token::Brace) {
                parse_braced_placeholder_as_value(input)?
            } else {
                parse_after_key_sep(input)?
            };
            return Ok(QueryEntry::Set { key, value });
        }

        if input.peek(Ident) {
            let id: Ident = input.parse()?;
            let key = key_from_query_ident(&id);

            if input.peek(Token![+=]) {
                input.parse::<Token![+=]>()?;
                let value = parse_value_expr(input)?;
                return Ok(QueryEntry::Push { key, value });
            }

            if input.peek(Token![=]) {
                input.parse::<Token![=]>()?;
                let value = parse_value_expr(input)?;
                return Ok(QueryEntry::Set { key, value });
            }

            // keyless ref => default Set
            return Ok(QueryEntry::Set {
                key,
                value: ValueExpr::Ref(id.into()),
            });
        }

        Err(input.error("invalid query entry"))
    }
}

// -------------------- Node tree --------------------

#[derive(Debug)]
pub enum Node {
    Prefix(NodeBlock),
    Path(NodeBlock),
    Endpoint(Box<EndpointDecl>),
}
impl Parse for Node {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek(prefix) {
            return Ok(Node::Prefix(input.parse()?));
        }
        if input.peek(path) {
            return Ok(Node::Path(input.parse()?));
        }
        Ok(Node::Endpoint(input.parse()?))
    }
}

#[derive(Debug)]
pub struct NodeBlock {
    pub template: RouteExpr,
    pub children: Vec<Node>,
    pub headers: Vec<HeaderRule>,
    pub query: Vec<QueryEntry>,
    pub timeout: Option<Expr>,
}
impl Parse for NodeBlock {
    fn parse(input: ParseStream) -> Result<Self> {
        let mode = if input.peek(prefix) {
            input.parse::<prefix>()?;
            RouteMode::Host
        } else if input.peek(path) {
            input.parse::<path>()?;
            RouteMode::Path
        } else {
            return Err(input.error("v1.5: expected prefix or path"));
        };

        let template = match mode {
            RouteMode::Host => RouteExpr::parse_host_expr(input)?,
            RouteMode::Path => RouteExpr::parse_path_expr(input)?,
        };

        if input.peek(headers) || input.peek(query) || input.peek(timeout) {
            return Err(input.error(
                "v1.7: `headers { ... }` / `query { ... }` / `timeout: ...` must be inside the block braces",
            ));
        }

        let content;
        braced!(content in input);

        let mut headers_v: Vec<HeaderRule> = Vec::new();
        let mut query_v: Vec<QueryEntry> = Vec::new();
        let mut timeout_v: Option<Expr> = None;
        let mut children: Vec<Node> = Vec::new();

        while !content.is_empty() {
            if content.peek(headers) {
                let hb: HeadersBlock = content.parse()?;
                headers_v = hb.rules;
                continue;
            }
            if content.peek(query) {
                let qb: QueryBlock = content.parse()?;
                query_v = qb.items;
                continue;
            }
            if content.peek(timeout) {
                content.parse::<timeout>()?;
                content.parse::<Token![:]>()?;
                if timeout_v.is_some() {
                    return Err(content.error("v1.7: duplicate timeout in block"));
                }
                timeout_v = Some(content.parse::<Expr>()?);
                if content.peek(Token![;]) {
                    content.parse::<Token![;]>()?;
                } else {
                    let _ = content.parse::<Token![,]>();
                }
                continue;
            }
            children.push(content.parse()?);
        }

        Ok(Self {
            template,
            headers: headers_v,
            query: query_v,
            timeout: timeout_v,
            children,
        })
    }
}

// -------------------- Codec spec --------------------

#[derive(Debug, Clone)]
pub struct CodecSpec {
    pub codec: Path,
    pub ty: Type,
}
impl Parse for CodecSpec {
    fn parse(input: ParseStream) -> Result<Self> {
        let codec: Path = input.call(Path::parse_mod_style)?;
        input.parse::<Token![<]>()?;
        let ty: Type = input.parse()?;
        input.parse::<Token![>]>()?;
        Ok(Self { codec, ty })
    }
}

// -------------------- Pagination clause --------------------

#[derive(Debug, Clone)]
pub struct PaginateClause {
    pub paginator: Path,
    pub args: Vec<PaginateArg>,
}

#[derive(Debug, Clone)]
pub struct PaginateArg {
    pub key: Ident,
    pub value: Option<Expr>, // shorthand allowed
}

impl Parse for PaginateClause {
    fn parse(input: ParseStream) -> Result<Self> {
        input.parse::<paginate>()?;
        let paginator: Path = input.call(Path::parse_mod_style)?;
        let content;
        braced!(content in input);
        let mut args: Vec<PaginateArg> = Vec::new();
        while !content.is_empty() {
            let key: Ident = content.parse()?;
            let value = if content.peek(Token![:]) {
                content.parse::<Token![:]>()?;
                Some(content.parse::<Expr>()?)
            } else {
                None
            };
            args.push(PaginateArg { key, value });
            if content.peek(Token![,]) {
                content.parse::<Token![,]>()?;
            }
        }
        Ok(Self { paginator, args })
    }
}

// -------------------- Endpoint --------------------
/// Parse an expression for `timeout: <expr>` in endpoint clauses.
///
/// IMPORTANT: In the endpoint DSL, `timeout: <expr>` is commonly followed by `-> ...` without any
/// delimiter. `syn::Expr` parsing fails in that situation because `->` is not a valid Rust token
/// after an expression in normal Rust grammar.
///
/// We therefore parse token-by-token until we reach a DSL boundary at top-level:
/// - `->`
/// - `,` (optional delimiter supported)
/// - `headers { ... }` / `query { ... }` / `body ...` / `paginate ...` / `timeout ...`
fn parse_timeout_expr_endpoint(input: ParseStream) -> Result<Expr> {
    let mut ts = TokenStream2::new();
    while !input.is_empty() {
        // Hard stop: response separator.
        if input.peek(Token![->]) {
            break;
        }
        // Optional delimiter after timeout value.
        if input.peek(Token![,]) {
            break;
        }

        // Stop on the next DSL clause keyword (top-level).
        // For headers/query, only stop when followed by `{` to avoid cutting paths like `headers::X`.
        if input.peek(headers) && input.peek2(syn::token::Brace) {
            break;
        }
        if input.peek(query) && input.peek2(syn::token::Brace) {
            break;
        }
        if input.peek(body) || input.peek(paginate) || input.peek(timeout) {
            break;
        }

        let tt: TokenTree = input.parse()?;
        ts.extend(std::iter::once(tt));
    }

    if ts.is_empty() {
        return Err(input.error("v1.7: expected an expression after `timeout:`"));
    }
    syn::parse2::<Expr>(ts)
}

#[derive(Debug)]
pub struct EndpointDecl {
    pub name: Ident,
    pub method: Ident,
    pub path: RouteExpr,
    pub headers: Vec<HeaderRule>,
    pub query: Vec<QueryEntry>,
    pub body: Option<CodecSpec>,
    pub paginate: Option<PaginateClause>,
    pub timeout: Option<Expr>,
    pub resp: CodecSpec,
    pub map: Option<MapClause>,
}

#[derive(Debug, Clone)]
pub struct MapClause {
    pub out_ty: Type,
    pub expr: Expr,
}

impl Parse for EndpointDecl {
    fn parse(input: ParseStream) -> Result<Self> {
        let method: Ident = input.parse()?;
        let name: Ident = input.parse()?;

        let path: RouteExpr = RouteExpr::parse_path_expr(input)?;

        let mut headers_v: Vec<HeaderRule> = Vec::new();
        let mut query_v: Vec<QueryEntry> = Vec::new();
        let mut body_v: Option<CodecSpec> = None;
        let mut timeout_v: Option<Expr> = None;
        let mut paginate_v: Option<PaginateClause> = None;
        loop {
            if input.peek(headers) {
                let hb: HeadersBlock = input.parse()?;
                headers_v = hb.rules;
                continue;
            }
            if input.peek(query) {
                let qb: QueryBlock = input.parse()?;
                query_v = qb.items;
                continue;
            }
            if input.peek(body) {
                input.parse::<body>()?;
                body_v = Some(input.parse::<CodecSpec>()?);
                continue;
            }
            if input.peek(paginate) {
                paginate_v = Some(input.parse::<PaginateClause>()?);
                continue;
            }
            if input.peek(timeout) {
                input.parse::<timeout>()?;
                input.parse::<Token![:]>()?;
                if timeout_v.is_some() {
                    return Err(input.error("v1.7: duplicate timeout in endpoint"));
                }
                // Must support: `timeout:Duration::from_secs(5) -> ...`
                timeout_v = Some(parse_timeout_expr_endpoint(input)?);
                // Optional delimiter (allows `timeout: expr, headers { ... }` etc).
                if input.peek(Token![,]) {
                    input.parse::<Token![,]>()?;
                }
                continue;
            }
            break;
        }

        input.parse::<Token![->]>()?;
        let resp: CodecSpec = input.parse()?;

        let map = if input.peek(Token![|]) {
            input.parse::<Token![|]>()?;
            let out_ty: Type = input.parse()?;
            input.parse::<Token![=>]>()?;
            let expr: Expr = input.parse()?;
            Some(MapClause { out_ty, expr })
        } else {
            None
        };

        input.parse::<Token![;]>()?;

        Ok(Self {
            name,
            method,
            path,
            headers: headers_v,
            query: query_v,
            body: body_v,
            paginate: paginate_v,
            timeout: timeout_v,
            resp,
            map,
        })
    }
}
