//! Parsing of the `api!` declaration into a syntax tree. No meaning is assigned here beyond
//! structure; name resolution and composition happen in `model`.

use proc_macro2::Span;
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{Attribute, Expr, Ident, LitStr, Token, Type, Visibility, braced, parenthesized};

mod kw {
    syn::custom_keyword!(client);
    syn::custom_keyword!(scope);
    syn::custom_keyword!(base);
    syn::custom_keyword!(path);
    syn::custom_keyword!(subdomain);
    syn::custom_keyword!(query);
    syn::custom_keyword!(header);
    syn::custom_keyword!(headers);
    syn::custom_keyword!(each);
    syn::custom_keyword!(body);
    syn::custom_keyword!(append);
    syn::custom_keyword!(remove);
}

/// HTTP methods accepted as endpoint keywords.
pub const METHODS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

pub struct Api {
    pub client: Owner,
}

pub enum OwnerKind {
    Client {
        vis: Visibility,
        base: Option<LitStr>,
    },
    Scope,
    Endpoint {
        method: Ident,
        output: Option<Type>,
        body: Option<BodyParam>,
    },
}

pub struct Owner {
    pub kind: OwnerKind,
    pub docs: Vec<Attribute>,
    pub name: Ident,
    pub params: Vec<Param>,
    pub stmts: Stmts,
    pub children: Vec<Owner>,
}

#[derive(Default)]
pub struct Stmts {
    pub path: Option<LitStr>,
    pub subdomain: Option<LitStr>,
    pub query: Option<Vec<Entry>>,
    pub headers: Option<Vec<Entry>>,
}

pub struct Param {
    pub docs: Vec<Attribute>,
    pub sensitive: bool,
    pub placement: Option<Placement>,
    pub name: Ident,
    pub optional: bool,
    pub ty: Type,
    pub default: Option<Expr>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Family {
    Query,
    Header,
}

pub struct Placement {
    pub family: Family,
    pub key: Option<LitStr>,
    pub each: bool,
    pub span: Span,
}

pub struct BodyParam {
    pub docs: Vec<Attribute>,
    pub name: Ident,
    pub optional: bool,
    pub ty: Type,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Op {
    Introduce,
    Append,
    Override,
    Remove,
}

pub struct Entry {
    pub op: Op,
    pub op_span: Span,
    pub key: LitStr,
    pub value: Option<Rhs>,
}

pub enum Rhs {
    Template(LitStr),
    Ref { each: bool, reference: Ref },
}

#[derive(Clone)]
pub struct Ref {
    pub owner: Option<Ident>,
    pub name: Ident,
}

impl Ref {
    pub fn span(&self) -> Span {
        self.owner
            .as_ref()
            .map_or_else(|| self.name.span(), Ident::span)
    }
}

impl Parse for Api {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let docs = docs_only(attrs)?;
        let vis: Visibility = input.parse()?;
        input.parse::<kw::client>().map_err(|e| {
            syn::Error::new(
                e.span(),
                "expected `client Name { ... }` as the root of `api!`",
            )
        })?;
        let name: Ident = input.parse()?;
        let params = if input.peek(syn::token::Paren) {
            parse_params(input, false)?.0
        } else {
            Vec::new()
        };
        let content;
        braced!(content in input);
        let mut base = None;
        let (stmts, children) = parse_owner_body(&content, Some(&mut base))?;
        if !input.is_empty() {
            return Err(input.error("unexpected tokens after the client declaration; `api!` declares exactly one client"));
        }
        Ok(Api {
            client: Owner {
                kind: OwnerKind::Client { vis, base },
                docs,
                name,
                params,
                stmts,
                children,
            },
        })
    }
}

/// Keeps `///` documentation; rejects every other attribute at owner level.
fn docs_only(attrs: Vec<Attribute>) -> syn::Result<Vec<Attribute>> {
    let mut docs = Vec::new();
    for attr in attrs {
        if attr.path().is_ident("doc") {
            docs.push(attr);
        } else {
            return Err(syn::Error::new(
                attr.span(),
                "unsupported attribute here; only documentation comments are allowed",
            ));
        }
    }
    Ok(docs)
}

fn parse_owner_body(
    input: ParseStream<'_>,
    mut base: Option<&mut Option<LitStr>>,
) -> syn::Result<(Stmts, Vec<Owner>)> {
    let mut stmts = Stmts::default();
    let mut children = Vec::new();
    while !input.is_empty() {
        if input.peek(kw::base) {
            let kw: kw::base = input.parse()?;
            let Some(slot) = base.as_deref_mut() else {
                return Err(syn::Error::new(
                    kw.span,
                    "`base` is only allowed on the client",
                ));
            };
            let lit: LitStr = input.parse()?;
            input.parse::<Token![;]>()?;
            set_once(slot, lit, kw.span, "base")?;
            continue;
        }
        if input.peek(kw::path)
            || input.peek(kw::subdomain)
            || input.peek(kw::query)
            || input.peek(kw::headers)
        {
            if let Some(first) = children.first() {
                let first: &Owner = first;
                return Err(syn::Error::new(
                    input.span(),
                    format!(
                        "owner statements (`path`, `subdomain`, `query`, `headers`) must precede child declarations (first child: `{}`)",
                        first.name
                    ),
                ));
            }
            parse_stmt(input, &mut stmts)?;
            continue;
        }
        children.push(parse_child(input)?);
    }
    Ok((stmts, children))
}

fn set_once<T>(slot: &mut Option<T>, value: T, span: Span, what: &str) -> syn::Result<()> {
    if slot.is_some() {
        return Err(syn::Error::new(
            span,
            format!("duplicate `{what}` statement; each owner declares it at most once"),
        ));
    }
    *slot = Some(value);
    Ok(())
}

fn parse_stmt(input: ParseStream<'_>, stmts: &mut Stmts) -> syn::Result<()> {
    if input.peek(kw::path) {
        let kw: kw::path = input.parse()?;
        let lit: LitStr = input.parse()?;
        input.parse::<Token![;]>()?;
        return set_once(&mut stmts.path, lit, kw.span, "path");
    }
    if input.peek(kw::subdomain) {
        let kw: kw::subdomain = input.parse()?;
        let lit: LitStr = input.parse()?;
        input.parse::<Token![;]>()?;
        return set_once(&mut stmts.subdomain, lit, kw.span, "subdomain");
    }
    if input.peek(kw::query) {
        let kw: kw::query = input.parse()?;
        let entries = parse_entries(input)?;
        return set_once(&mut stmts.query, entries, kw.span, "query");
    }
    let kw: kw::headers = input.parse()?;
    let entries = parse_entries(input)?;
    set_once(&mut stmts.headers, entries, kw.span, "headers")
}

fn parse_entries(input: ParseStream<'_>) -> syn::Result<Vec<Entry>> {
    let content;
    braced!(content in input);
    let mut entries = Vec::new();
    while !content.is_empty() {
        let (op, op_span) = if content.peek(kw::append) {
            (Op::Append, content.parse::<kw::append>()?.span)
        } else if content.peek(Token![override]) {
            (Op::Override, content.parse::<Token![override]>()?.span)
        } else if content.peek(kw::remove) {
            (Op::Remove, content.parse::<kw::remove>()?.span)
        } else {
            (Op::Introduce, content.span())
        };
        let key: LitStr = content.parse().map_err(|e| {
            syn::Error::new(
                e.span(),
                "expected a quoted key, e.g. `\"per_page\" = per_page;`",
            )
        })?;
        let value = if op == Op::Remove {
            None
        } else {
            content.parse::<Token![=]>()?;
            Some(parse_rhs(&content)?)
        };
        content.parse::<Token![;]>()?;
        entries.push(Entry {
            op,
            op_span,
            key,
            value,
        });
    }
    Ok(entries)
}

fn parse_rhs(input: ParseStream<'_>) -> syn::Result<Rhs> {
    if input.peek(LitStr) {
        return Ok(Rhs::Template(input.parse()?));
    }
    let each = if input.peek(kw::each) {
        input.parse::<kw::each>()?;
        true
    } else {
        false
    };
    Ok(Rhs::Ref {
        each,
        reference: parse_ref(input)?,
    })
}

fn parse_ref(input: ParseStream<'_>) -> syn::Result<Ref> {
    let first: Ident = input.call(Ident::parse_any_ident)?;
    if input.peek(Token![.]) {
        input.parse::<Token![.]>()?;
        let name: Ident = input.call(Ident::parse_any_ident)?;
        Ok(Ref {
            owner: Some(first),
            name,
        })
    } else {
        Ok(Ref {
            owner: None,
            name: first,
        })
    }
}

trait ParseAnyIdent {
    fn parse_any_ident(input: ParseStream<'_>) -> syn::Result<Ident>;
}

impl ParseAnyIdent for Ident {
    fn parse_any_ident(input: ParseStream<'_>) -> syn::Result<Ident> {
        use syn::ext::IdentExt;
        Ident::parse_any(input)
    }
}

fn parse_child(input: ParseStream<'_>) -> syn::Result<Owner> {
    let attrs = input.call(Attribute::parse_outer)?;
    let docs = docs_only(attrs)?;
    if input.peek(kw::scope) {
        input.parse::<kw::scope>()?;
        let name: Ident = input.parse()?;
        let params = if input.peek(syn::token::Paren) {
            parse_params(input, false)?.0
        } else {
            Vec::new()
        };
        let content;
        braced!(content in input);
        let (stmts, children) = parse_owner_body(&content, None)?;
        return Ok(Owner {
            kind: OwnerKind::Scope,
            docs,
            name,
            params,
            stmts,
            children,
        });
    }
    let method: Ident = input.parse().map_err(|e| {
        syn::Error::new(
            e.span(),
            "expected `scope`, an HTTP method (`GET`, `POST`, ...) or an owner statement",
        )
    })?;
    if !METHODS.contains(&method.to_string().as_str()) {
        return Err(syn::Error::new(
            method.span(),
            format!(
                "expected `scope`, an HTTP method ({}) or an owner statement (`path`, `subdomain`, `query`, `headers`), found `{method}`",
                METHODS.join(", ")
            ),
        ));
    }
    let name: Ident = input.parse()?;
    let (params, body) = parse_params(input, true)?;
    let output = if input.peek(Token![->]) {
        input.parse::<Token![->]>()?;
        Some(input.parse::<Type>()?)
    } else {
        None
    };
    let stmts = if input.peek(Token![;]) {
        input.parse::<Token![;]>()?;
        Stmts::default()
    } else {
        let content;
        braced!(content in input);
        let (stmts, children) = parse_owner_body(&content, None)?;
        if let Some(child) = children.first() {
            return Err(syn::Error::new(
                child.name.span(),
                "endpoints cannot contain scopes or endpoints",
            ));
        }
        stmts
    };
    Ok(Owner {
        kind: OwnerKind::Endpoint {
            method,
            output,
            body,
        },
        docs,
        name,
        params,
        stmts,
        children: Vec::new(),
    })
}

fn parse_params(
    input: ParseStream<'_>,
    allow_body: bool,
) -> syn::Result<(Vec<Param>, Option<BodyParam>)> {
    let content;
    parenthesized!(content in input);
    let mut params = Vec::new();
    let mut body: Option<BodyParam> = None;
    while !content.is_empty() {
        let attrs = content.call(Attribute::parse_outer)?;
        let mut docs = Vec::new();
        let mut sensitive = false;
        for attr in attrs {
            if attr.path().is_ident("doc") {
                docs.push(attr);
            } else if attr.path().is_ident("sensitive") && matches!(attr.meta, syn::Meta::Path(_)) {
                if sensitive {
                    return Err(syn::Error::new(attr.span(), "duplicate `#[sensitive]`"));
                }
                sensitive = true;
            } else {
                return Err(syn::Error::new(
                    attr.span(),
                    "unsupported attribute; values accept documentation comments and `#[sensitive]`",
                ));
            }
        }
        if content.peek(kw::body) && !content.peek2(Token![:]) && !content.peek2(Token![?]) {
            let kw: kw::body = content.parse()?;
            if !allow_body {
                return Err(syn::Error::new(
                    kw.span,
                    "only endpoints can declare a `body`",
                ));
            }
            if sensitive {
                return Err(syn::Error::new(
                    kw.span,
                    "`#[sensitive]` applies to placed values, not bodies (Concord never renders bodies)",
                ));
            }
            let name: Ident = content.parse()?;
            let optional = parse_question(&content)?;
            content.parse::<Token![:]>()?;
            let ty: Type = content.parse()?;
            if content.peek(Token![=]) {
                return Err(content.error("a body cannot have a default"));
            }
            if body.is_some() {
                return Err(syn::Error::new(
                    kw.span,
                    "an endpoint declares at most one `body`",
                ));
            }
            body = Some(BodyParam {
                docs,
                name,
                optional,
                ty,
            });
        } else {
            let placement = parse_placement(&content)?;
            let name: Ident = content.parse()?;
            let optional = parse_question(&content)?;
            content.parse::<Token![:]>()?;
            let ty: Type = content.parse()?;
            let default = if content.peek(Token![=]) {
                content.parse::<Token![=]>()?;
                Some(content.parse::<Expr>()?)
            } else {
                None
            };
            params.push(Param {
                docs,
                sensitive,
                placement,
                name,
                optional,
                ty,
                default,
            });
        }
        if content.is_empty() {
            break;
        }
        content.parse::<Token![,]>()?;
    }
    Ok((params, body))
}

fn parse_question(input: ParseStream<'_>) -> syn::Result<bool> {
    if input.peek(Token![?]) {
        input.parse::<Token![?]>()?;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn parse_placement(input: ParseStream<'_>) -> syn::Result<Option<Placement>> {
    // `query x: T` / `query("k") x: T` / `header("k") x: T`, each optionally followed by `each`.
    let family = if input.peek(kw::query) && !input.peek2(Token![:]) && !input.peek2(Token![?]) {
        let kw: kw::query = input.parse()?;
        (Family::Query, kw.span)
    } else if input.peek(kw::header) && !input.peek2(Token![:]) && !input.peek2(Token![?]) {
        let kw: kw::header = input.parse()?;
        (Family::Header, kw.span)
    } else {
        if input.peek(kw::each) && !input.peek2(Token![:]) && !input.peek2(Token![?]) {
            return Err(
                input.error("`each` follows a placement: `query(\"tag\") each tags: Vec<String>`")
            );
        }
        return Ok(None);
    };
    let key = if input.peek(syn::token::Paren) {
        let content;
        parenthesized!(content in input);
        let key: LitStr = content.parse()?;
        if !content.is_empty() {
            return Err(content.error("expected a single quoted key"));
        }
        Some(key)
    } else {
        None
    };
    if family.0 == Family::Header && key.is_none() {
        return Err(syn::Error::new(
            family.1,
            "a header placement names its field: `header(\"x-api-key\") key: String`",
        ));
    }
    let each = if input.peek(kw::each) && !input.peek2(Token![:]) && !input.peek2(Token![?]) {
        input.parse::<kw::each>()?;
        true
    } else {
        false
    };
    Ok(Some(Placement {
        family: family.0,
        key,
        each,
        span: family.1,
    }))
}
