//! Parsing of the `api!` declaration into a syntax tree. No meaning is assigned here beyond
//! structure; name resolution and composition happen in `model`.

use proc_macro2::Span;
use syn::ext::IdentExt;
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
    syn::custom_keyword!(via);
    syn::custom_keyword!(inherit);
    syn::custom_keyword!(request);
    syn::custom_keyword!(response);
    syn::custom_keyword!(format);
    syn::custom_keyword!(json);
    syn::custom_keyword!(text);
    syn::custom_keyword!(bytes);
    syn::custom_keyword!(form);
}

/// HTTP methods accepted as endpoint keywords.
pub const METHODS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

pub struct Api {
    pub client: Owner,
}

#[allow(clippy::large_enum_variant)]
pub enum OwnerKind {
    Client {
        vis: Visibility,
        base: Option<LitStr>,
    },
    Scope,
    Endpoint {
        method: Ident,
        output: Option<Output>,
        body: Option<BodyParam>,
    },
}

/// A body or output format as written: a built-in word or a type path. It is chosen only by its
/// spelling in format position, never by the payload type.
#[derive(Clone)]
pub enum Format {
    Word(Word, Span),
    Path(syn::Path),
}

impl Format {
    /// Where the format is written.
    pub fn span(&self) -> Span {
        match self {
            Format::Word(_, span) => *span,
            Format::Path(path) => path.span(),
        }
    }

    /// The format as written (`json`, `codecs::Csv`), for diagnostics and documentation.
    pub fn text(&self) -> String {
        match self {
            Format::Word(word, _) => word.as_str().to_owned(),
            Format::Path(path) => type_text(path),
        }
    }
}

/// The built-in format words. They resolve to `$crate::{Json, Text, Binary, Form}`, so they need
/// no import and work under a renamed dependency.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Word {
    Json,
    Text,
    Bytes,
    Form,
}

impl Word {
    const ALL: [Word; 4] = [Word::Json, Word::Text, Word::Bytes, Word::Form];

    pub fn as_str(self) -> &'static str {
        match self {
            Word::Json => "json",
            Word::Text => "text",
            Word::Bytes => "bytes",
            Word::Form => "form",
        }
    }

    /// The runtime marker type in the `concord` crate.
    pub fn marker(self) -> &'static str {
        match self {
            Word::Json => "Json",
            Word::Text => "Text",
            Word::Bytes => "Binary",
            Word::Form => "Form",
        }
    }

    /// The marker's `Encode::CONTENT_TYPE`, for generated documentation only (kept in sync with
    /// `concord/src/repr.rs`; the request itself uses the marker's constant).
    pub fn media_type(self) -> &'static str {
        match self {
            Word::Json => "application/json",
            Word::Text => "text/plain; charset=utf-8",
            Word::Bytes => "application/octet-stream",
            Word::Form => "application/x-www-form-urlencoded",
        }
    }

    /// A raw identifier (`r#json`) is never a word.
    fn of(ident: &Ident) -> Option<Self> {
        let text = ident.to_string();
        Self::ALL.into_iter().find(|w| text == w.as_str())
    }
}

const WORDS: &str = "`json`, `text`, `bytes` or `form`";

/// A typed output: `-> T`. Legacy format tokens are retained only for migration errors.
pub struct Output {
    pub ty: Type,
    /// `None`: the nearest `response` default.
    pub format: Option<Format>,
}

/// A uniform representation statement on any owner.
#[derive(Clone)]
pub struct FormatDefault {
    pub format: Format,
    pub media: Option<LitStr>,
    /// Written as `format F;` (both directions).
    pub both: bool,
}

/// A type or path as written, without `quote`'s token spacing (`Vec<Item>`, not `Vec < Item >`).
pub fn type_text(t: &dyn quote::ToTokens) -> String {
    t.to_token_stream()
        .to_string()
        .replace(" < ", "<")
        .replace("< ", "<")
        .replace(" >", ">")
        .replace(" :: ", "::")
        .replace(":: ", "::")
        .replace(" ,", ",")
        .replace("& ", "&")
}

/// A `via` representation written as a format path naming the same type: the path as written,
/// except a bare lowercase identifier (a type named `text`, say), which becomes `self::text`
/// because a bare word in format position is a built-in format. Which type a path names is
/// decided by name resolution, so no path is turned into a word here (`codec::Text` and a local
/// module named `concord` keep their meaning). Only used to word a diagnostic reported anyway.
fn replacement(repr: &Type) -> String {
    if let Type::Path(p) = repr {
        if p.qself.is_none() && p.path.leading_colon.is_none() && p.path.segments.len() == 1 {
            let segment = &p.path.segments[0];
            let name = segment.ident.to_string();
            let bare = name.trim_start_matches("r#");
            if segment.arguments.is_none() && bare.starts_with(|c: char| c.is_ascii_lowercase()) {
                // As written: a raw identifier keeps its `r#` (`self::r#type`).
                return format!("self::{name}");
            }
        }
    }
    type_text(repr)
}

/// `via R` after a payload type: the removed spelling, reported with its exact replacement.
fn reject_via(input: ParseStream<'_>, ty: &Type, site: ViaSite<'_>) -> syn::Result<()> {
    if !input.peek(kw::via) {
        return Ok(());
    }
    let via: kw::via = input.parse()?;
    let repr: Type = input.parse()?;
    let format = replacement(&repr);
    let ty = type_text(ty);
    let message = match site {
        ViaSite::Output => {
            format!("`via` was removed: write `-> {ty}` and `response {format};` on the endpoint")
        }
        ViaSite::Body { name, optional } => format!(
            "`via` was removed: write `body {name}{optional}: {ty}` and `request {format};` on the endpoint"
        ),
    };
    Err(syn::Error::new(via.span, message))
}

#[derive(Clone, Copy)]
enum ViaSite<'a> {
    Output,
    Body { name: &'a Ident, optional: &'a str },
}

/// A built-in word in format position: the word not followed by `::` (`json::Codec` is a path).
fn peek_word(input: ParseStream<'_>) -> bool {
    (input.peek(kw::json) || input.peek(kw::text) || input.peek(kw::bytes) || input.peek(kw::form))
        && !input.peek2(Token![::])
}

/// A word or a type path in format position. A bare lowercase identifier must be one of the
/// words: a lowercase format type is written as a path (`self::csv`). In a parameter list the
/// identifier may also be a misspelled placement (`qurey page: u32`), and the error says so.
fn parse_format(input: ParseStream<'_>, in_params: bool) -> syn::Result<Format> {
    if peek_word(input) {
        let ident: Ident = input.parse()?;
        let word = Word::of(&ident).expect("peeked a format word");
        return Ok(Format::Word(word, ident.span()));
    }
    let path: syn::Path = input.parse()?;
    if path.leading_colon.is_none() && path.segments.len() == 1 {
        let segment = &path.segments[0];
        let text = segment.ident.to_string();
        if segment.arguments.is_none()
            && !text.starts_with("r#")
            && text
                .trim_start_matches("r#")
                .starts_with(|c: char| c.is_ascii_lowercase())
        {
            let what = if in_params {
                format!(
                    "`{text}` is neither a placement (`query`, `header(\"..\")`) nor a body format: the built-in formats are {WORDS}; a format type is written as a path (`self::{text}`)"
                )
            } else {
                format!(
                    "`{text}` is not a format: the built-in formats are {WORDS}; a format type is written as a path (`self::{text}`)"
                )
            };
            return Err(syn::Error::new(segment.ident.span(), what));
        }
    }
    Ok(Format::Path(path))
}

/// `("media/type")` after a format or `body`: the body's media type, checked here.
fn parse_media(input: ParseStream<'_>) -> syn::Result<LitStr> {
    let content;
    parenthesized!(content in input);
    let lit: LitStr = content.parse().map_err(|e| {
        syn::Error::new(
            e.span(),
            "expected the body's media type as a string, e.g. `json(\"application/merge-patch+json\") patch: Patch`",
        )
    })?;
    if !content.is_empty() {
        return Err(content.error("expected a single quoted media type"));
    }
    check_media_type(&lit)?;
    Ok(lit)
}

/// Whether a parenthesized group holds exactly one string literal (a media type) rather than a
/// tuple type (`-> json ()`, `-> codecs::Csv (A, B)`).
fn peek_media(input: ParseStream<'_>) -> bool {
    let media = |input: ParseStream<'_>| -> syn::Result<()> {
        let content;
        parenthesized!(content in input);
        content.parse::<LitStr>()?;
        if content.is_empty() {
            Ok(())
        } else {
            Err(content.error("not a media type"))
        }
    };
    media(&input.fork()).is_ok()
}

/// A media type written on an output (the removed `-> json("…") T`, or `response json("…");`),
/// reported at the literal when there is one.
fn output_media(input: ParseStream<'_>) -> syn::Error {
    let literal = |input: ParseStream<'_>| -> syn::Result<LitStr> {
        let content;
        parenthesized!(content in input);
        content.parse()
    };
    let span = literal(&input.fork()).map_or_else(|_| input.span(), |lit| lit.span());
    syn::Error::new(
        span,
        "an output takes no media type: Concord sends no `Accept` and does not check the response `Content-Type`; to send an `Accept` header, declare it (`headers { \"accept\" = \"...\"; }`)",
    )
}

/// A declared body media type: parsed by the `mime` crate (Concord keeps no media-type grammar of
/// its own) and checked by `http` as a static header value, since it becomes
/// `HeaderValue::from_static` (visible ASCII).
fn check_media_type(lit: &LitStr) -> syn::Result<()> {
    let text = lit.value();
    let fail = |why: &str| {
        Err(syn::Error::new(
            lit.span(),
            format!(
                "{text:?} is not a media type: {why} (expected `type/subtype` with optional `; name=value` parameters, e.g. \"application/merge-patch+json\")"
            ),
        ))
    };
    if text.parse::<mime::Mime>().is_err() {
        return fail("rejected by the `mime` parser");
    }
    match http::HeaderValue::from_str(&text) {
        Ok(value) if value.to_str().is_ok() => Ok(()),
        _ => fail("not a static header value (visible ASCII, checked by `http::HeaderValue`)"),
    }
}

/// The payload of a typed output, after `->`: `T`, `FORMAT T`.
fn parse_output(input: ParseStream<'_>) -> syn::Result<Output> {
    let at_end = |input: ParseStream<'_>| {
        input.peek(syn::token::Brace) || input.peek(Token![;]) || input.is_empty()
    };
    let explicit = if peek_word(input) {
        let ident: Ident = input.parse()?;
        let word = Word::of(&ident).expect("peeked a format word");
        if at_end(input) {
            // A single path is always a Rust payload type, even if spelled like a format word.
            return Ok(Output {
                ty: syn::parse_quote!(#ident),
                format: None,
            });
        }
        Some(Format::Word(word, ident.span()))
    } else {
        // A path followed by more type tokens is a format; a path that ends the output is the
        // payload (inherited format); anything else (`()`, `[u8; 4]`, ...) is a payload.
        let fork = input.fork();
        match fork.parse::<syn::Path>() {
            Ok(path) if !at_end(&fork) && !fork.peek(kw::via) => {
                if let Ok(ident) = fork.fork().parse::<Ident>() {
                    if let Some(word) = Word::of(&ident).filter(|_| !fork.peek2(Token![::])) {
                        return Err(syn::Error::new(
                            ident.span(),
                            format!(
                                "removed output format: write `-> {1}` and `response {0};` on this endpoint",
                                word.as_str(),
                                type_text(&path)
                            ),
                        ));
                    }
                }
                Some(parse_format(input, false)?)
            }
            _ => None,
        }
    };
    if explicit.is_some() && input.peek(syn::token::Paren) && peek_media(input) {
        return Err(output_media(input));
    }
    let ty: Type = input.parse()?;
    reject_via(input, &ty, ViaSite::Output)?;
    // `-> () json`: a word after a payload that is not a path (paths are checked above).
    if let Ok(ident) = input.fork().parse::<Ident>() {
        if let Some(word) = Word::of(&ident).filter(|_| explicit.is_none()) {
            return Err(syn::Error::new(
                ident.span(),
                format!(
                    "removed output format: write `-> {1}` and `response {0};` on this endpoint",
                    word.as_str(),
                    type_text(&ty)
                ),
            ));
        }
    }
    Ok(Output {
        ty,
        format: explicit,
    })
}

pub struct Owner {
    pub kind: OwnerKind,
    pub docs: Vec<Attribute>,
    /// `#[cfg(..)]` and `#[deprecated..]` on scopes and endpoints.
    pub cfgs: Vec<Attribute>,
    pub deprecated: Option<Attribute>,
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
    /// `request FORMAT ["media"];` on any owner.
    pub request: Option<FormatDefault>,
    /// `response FORMAT;` on any owner.
    pub response: Option<FormatDefault>,
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

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Family {
    Query,
    Header,
}

/// A query or header key: a string literal, or (headers only) a Rust path to a native
/// `HeaderName` constant or static, resolved by rustc.
#[derive(Clone)]
pub enum Key {
    Lit(LitStr),
    Path(syn::Path),
}

impl Key {
    pub fn span(&self) -> Span {
        match self {
            Key::Lit(lit) => lit.span(),
            Key::Path(path) => path.span(),
        }
    }
}

fn parse_key(input: ParseStream<'_>, expected: &str) -> syn::Result<Key> {
    if input.peek(LitStr) {
        return Ok(Key::Lit(input.parse()?));
    }
    if input.peek(Token![::]) || input.peek(Ident::peek_any) {
        return input.call(syn::Path::parse_mod_style).map(Key::Path);
    }
    Err(input.error(expected))
}

pub struct Placement {
    pub family: Family,
    pub key: Option<Key>,
    pub each: bool,
    pub span: Span,
}

pub struct BodyParam {
    /// Number of ordinary values declared before the body (its positional order).
    pub position: usize,
    pub docs: Vec<Attribute>,
    pub name: Ident,
    pub optional: bool,
    pub ty: Type,
    /// A format written in place (`json item: T`): a removed form, parsed only so the model can
    /// report its replacement. `None` for `body item: T`.
    pub format: Option<Format>,
    /// A media type written on the body (`FORMAT("…")`, `body("…")`): removed, likewise.
    pub media: Option<LitStr>,
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
    pub key: Key,
    pub value: Option<Rhs>,
    /// `override "k" = v else inherit;` (required when `v` can be absent).
    pub else_inherit: bool,
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
        let (stmts, children) = parse_owner_body(&content, Some(&mut base), Site::Owner)?;
        if !input.is_empty() {
            return Err(input.error("unexpected tokens after the client declaration; `api!` declares exactly one client"));
        }
        Ok(Api {
            client: Owner {
                kind: OwnerKind::Client { vis, base },
                docs,
                cfgs: Vec::new(),
                deprecated: None,
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

/// Where an owner body is parsed: format defaults belong to clients and scopes only.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Site {
    Owner,
    Endpoint,
}

fn parse_owner_body(
    input: ParseStream<'_>,
    mut base: Option<&mut Option<LitStr>>,
    site: Site,
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
            || input.peek(kw::request)
            || input.peek(kw::response)
            || input.peek(kw::format)
        {
            if let Some(first) = children.first() {
                let first: &Owner = first;
                return Err(syn::Error::new(
                    input.span(),
                    format!(
                        "owner statements (`path`, `subdomain`, `query`, `headers`, `request`, `response`) must precede child declarations (first child: `{}`)",
                        first.name
                    ),
                ));
            }
            if input.peek(kw::request) || input.peek(kw::response) || input.peek(kw::format) {
                parse_default(input, &mut stmts, site)?;
            } else {
                parse_stmt(input, &mut stmts)?;
            }
            continue;
        }
        children.push(parse_child(input)?);
    }
    Ok((stmts, children))
}

/// `request FORMAT;`, `request FORMAT("media/type");`, `response FORMAT;`: the formats of the
/// bodies and typed outputs below that do not write their own.
fn parse_default(input: ParseStream<'_>, stmts: &mut Stmts, _site: Site) -> syn::Result<()> {
    let both = input.peek(kw::format);
    let request = both || input.peek(kw::request);
    let span = if both {
        input.parse::<kw::format>()?.span
    } else if request {
        input.parse::<kw::request>()?.span
    } else {
        input.parse::<kw::response>()?.span
    };
    if input.peek(LitStr) {
        return Err(input.error(
            "a media type follows its format, which the statement restates: `request json \"application/merge-patch+json\";` (a statement replaces the inherited format and media type together)",
        ));
    }
    let format = parse_format(input, false)?;
    if input.peek(syn::token::Paren) {
        let media = parse_media(input)?;
        return Err(syn::Error::new(
            media.span(),
            format!(
                "removed parenthesized media type: write `request {} {:?};`",
                format.text(),
                media.value()
            ),
        ));
    }
    let media = if input.peek(LitStr) {
        if both {
            return Err(input.error(
                "`format` sets both directions and takes no media type: write `request FORMAT \"media/type\";` (and `response FORMAT;`)",
            ));
        }
        if !request {
            return Err(input.error(
                "an output takes no media type: Concord sends no `Accept` and does not check the response `Content-Type`; to send an `Accept` header, declare it (`headers { ACCEPT = \"...\"; }`)",
            ));
        }
        let lit: LitStr = input.parse()?;
        check_media_type(&lit)?;
        Some(lit)
    } else {
        None
    };
    input.parse::<Token![;]>()?;
    let default = FormatDefault {
        format,
        media,
        both,
    };
    if both {
        if stmts.request.is_some() || stmts.response.is_some() {
            return Err(syn::Error::new(
                span,
                "duplicate direction: `format` counts as both `request` and `response`",
            ));
        }
        stmts.request = Some(default.clone());
        stmts.response = Some(default);
        Ok(())
    } else if request {
        set_once(&mut stmts.request, default, span, "request")
    } else {
        set_once(&mut stmts.response, default, span, "response")
    }
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
        // `append`/`remove` directly followed by `=` are a key named so; any other key path that
        // starts with them is written `self::append::...` (`append ::x` is append + absolute path).
        let keyword = |c: &syn::parse::ParseBuffer<'_>| !c.peek2(Token![=]);
        let (op, op_span) = if content.peek(kw::append) && keyword(&content) {
            (Op::Append, content.parse::<kw::append>()?.span)
        } else if content.peek(Token![override]) {
            (Op::Override, content.parse::<Token![override]>()?.span)
        } else if content.peek(kw::remove) && keyword(&content) {
            (Op::Remove, content.parse::<kw::remove>()?.span)
        } else {
            (Op::Introduce, content.span())
        };
        let key = parse_key(
            &content,
            "expected a key: a quoted name (`\"per_page\" = per_page;`) or, for a header, a native `HeaderName` constant (`ACCEPT = \"application/json\";`)",
        )?;
        let value = if op == Op::Remove {
            None
        } else {
            content.parse::<Token![=]>()?;
            Some(parse_rhs(&content)?)
        };
        let else_inherit = if content.peek(Token![else]) {
            let e: Token![else] = content.parse()?;
            content.parse::<kw::inherit>()?;
            if op != Op::Override {
                return Err(syn::Error::new(
                    e.span,
                    "`else inherit` only follows `override`",
                ));
            }
            true
        } else {
            false
        };
        content.parse::<Token![;]>()?;
        entries.push(Entry {
            op,
            op_span,
            key,
            value,
            else_inherit,
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
        Ident::parse_any(input)
    }
}

/// Children accept docs, `#[cfg(..)]` (whole subtree) and `#[deprecated..]`
/// (navigation method only); every other attribute is still rejected.
fn child_attrs(
    attrs: Vec<Attribute>,
) -> syn::Result<(Vec<Attribute>, Vec<Attribute>, Option<Attribute>)> {
    let (mut docs, mut cfgs, mut deprecated) = (Vec::new(), Vec::new(), None);
    for attr in attrs {
        if attr.path().is_ident("cfg") {
            cfgs.push(attr);
        } else if attr.path().is_ident("deprecated") {
            if deprecated.is_some() {
                return Err(syn::Error::new(attr.span(), "duplicate `#[deprecated]`"));
            }
            deprecated = Some(attr);
        } else if attr.path().is_ident("doc") {
            docs.push(attr);
        } else {
            return Err(syn::Error::new(
                attr.span(),
                "unsupported attribute on a scope or endpoint; documentation comments, `#[cfg(..)]` and `#[deprecated]` are accepted",
            ));
        }
    }
    Ok((docs, cfgs, deprecated))
}

fn parse_child(input: ParseStream<'_>) -> syn::Result<Owner> {
    let attrs = input.call(Attribute::parse_outer)?;
    let (docs, cfgs, deprecated) = child_attrs(attrs)?;
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
        let (stmts, children) = parse_owner_body(&content, None, Site::Owner)?;
        return Ok(Owner {
            kind: OwnerKind::Scope,
            docs,
            cfgs,
            deprecated,
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
                "expected `scope`, an HTTP method ({}) or an owner statement (`path`, `subdomain`, `query`, `headers`, `request`, `response`), found `{method}`",
                METHODS.join(", ")
            ),
        ));
    }
    let name: Ident = input.parse()?;
    let (params, body) = parse_params(input, true)?;
    let output = if input.peek(Token![->]) {
        input.parse::<Token![->]>()?;
        Some(parse_output(input)?)
    } else {
        None
    };
    let stmts = if input.peek(Token![;]) {
        input.parse::<Token![;]>()?;
        Stmts::default()
    } else {
        let content;
        braced!(content in input);
        let (stmts, children) = parse_owner_body(&content, None, Site::Endpoint)?;
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
        cfgs,
        deprecated,
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
        // `name: T` / `name?: T` is an ordinary value whatever its name (`json: T`, `body?: T`);
        // `Token![:]` also peeks the first half of `::`, which starts a format path instead.
        // A leading `::` (an absolute format path, `::c4::Text b: T`) is never a value name.
        let named = content.peek(Ident::peek_any)
            && ((content.peek2(Token![:]) && !content.peek2(Token![::]))
                || content.peek2(Token![?]));
        let placement =
            (content.peek(kw::query) || content.peek(kw::header) || content.peek(kw::each))
                && !content.peek2(Token![::]);
        let starts_path = content.peek(Ident::peek_any) || content.peek(Token![::]);
        if !named && !placement && starts_path {
            // A body: `body name: T`, `body("media") name: T`, `FORMAT name: T`,
            // `FORMAT("media") name: T`.
            let (format, at) = if content.peek(kw::body) && !content.peek2(Token![::]) {
                (None, content.parse::<kw::body>()?.span)
            } else if peek_word(&content) || allow_body {
                let format = parse_format(&content, true)?;
                let at = format.span();
                (Some(format), at)
            } else {
                let what: Ident = content.call(Ident::parse_any)?;
                return Err(syn::Error::new(
                    what.span(),
                    format!(
                        "expected a value (`name: T`) or a placement (`query`, `header(\"..\")`), found `{what}`"
                    ),
                ));
            };
            if !allow_body {
                return Err(syn::Error::new(at, "only endpoints can declare a body"));
            }
            if sensitive {
                return Err(syn::Error::new(
                    at,
                    "`#[sensitive]` applies to placed values, not bodies (Concord never renders bodies)",
                ));
            }
            let media = if content.peek(syn::token::Paren) {
                Some(parse_media(&content)?)
            } else {
                None
            };
            let name: Ident = content.parse().map_err(|e| {
                let written = format.as_ref().map_or("body".to_owned(), Format::text);
                syn::Error::new(
                    e.span(),
                    format!(
                        "expected the body's name: `{written} item: NewItem` (a value is written `name: T`; a placement `query x: T` or `header(\"..\") x: T`)"
                    ),
                )
            })?;
            let optional = parse_question(&content)?;
            content.parse::<Token![:]>()?;
            let ty: Type = content.parse()?;
            reject_via(
                &content,
                &ty,
                ViaSite::Body {
                    name: &name,
                    optional: if optional { "?" } else { "" },
                },
            )?;
            if content.peek(Token![=]) {
                return Err(content.error("a body cannot have a default"));
            }
            if body.is_some() {
                return Err(syn::Error::new(at, "an endpoint declares at most one body"));
            }
            body = Some(BodyParam {
                position: params.len(),
                docs,
                name,
                optional,
                ty,
                format,
                media,
            });
        } else {
            let placement = parse_placement(&content)?;
            let name: Ident = content.parse()?;
            let optional = parse_question(&content)?;
            content.parse::<Token![:]>()?;
            let ty: Type = content.parse()?;
            if allow_body && placement.is_none() {
                // `item: NewItem via Json` (the removed spelling without `body`) and
                // `item: NewItem json` (a format after the type).
                reject_via(
                    &content,
                    &ty,
                    ViaSite::Body {
                        name: &name,
                        optional: if optional { "?" } else { "" },
                    },
                )?;
                if let Ok(ident) = content.fork().parse::<Ident>() {
                    if let Some(word) = Word::of(&ident) {
                        return Err(syn::Error::new(
                            ident.span(),
                            format!(
                                "removed body format: write `body {1}{2}: {3}` and `request {0};` on this endpoint",
                                word.as_str(),
                                plain(&name),
                                if optional { "?" } else { "" },
                                type_text(&ty)
                            ),
                        ));
                    }
                }
            }
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

fn plain(ident: &Ident) -> String {
    let s = ident.to_string();
    s.strip_prefix("r#").map(str::to_owned).unwrap_or(s)
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
        let key = parse_key(
            &content,
            "expected a quoted name (`header(\"x-request-id\")`) or a native `HeaderName` constant (`header(AUTHORIZATION)`)",
        )?;
        if !content.is_empty() {
            return Err(content.error("expected a single key"));
        }
        Some(key)
    } else {
        None
    };
    if family.0 == Family::Header && key.is_none() {
        return Err(syn::Error::new(
            family.1,
            "a header placement names its field: `header(\"x-api-key\") key: String` or `header(AUTHORIZATION) token: String`",
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

#[cfg(test)]
mod tests {
    use super::replacement;

    /// A `via` path is suggested as written; a bare lowercase identifier gets `self::`, keeping a
    /// raw identifier's `r#` (`self::type` would not parse) — re-check F3e.
    #[test]
    fn via_replacement_names_the_same_path() {
        let text = |t: syn::Type| replacement(&t);
        assert_eq!(text(syn::parse_quote!(concord::Json)), "concord::Json");
        assert_eq!(text(syn::parse_quote!(Text)), "Text");
        assert_eq!(text(syn::parse_quote!(::c4::Text)), "::c4::Text");
        assert_eq!(text(syn::parse_quote!(text)), "self::text");
        assert_eq!(text(syn::parse_quote!(r#type)), "self::r#type");
        assert_eq!(text(syn::parse_quote!(r#text)), "self::r#text");
    }
}
