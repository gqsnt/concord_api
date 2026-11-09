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
    syn::custom_keyword!(stream);
    syn::custom_keyword!(send);
    syn::custom_keyword!(params);
    syn::custom_keyword!(limit);
    syn::custom_keyword!(page);
    syn::custom_keyword!(next);
    syn::custom_keyword!(link);
    syn::custom_keyword!(url);
}

/// HTTP methods accepted as endpoint keywords.
pub const METHODS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

pub struct Api {
    pub client: Owner,
    /// `params Name(..);` on the client: parameter groups, spliced into the endpoints that
    /// declare `..Name` ([`expand_groups`]).
    pub groups: Vec<Group>,
}

/// `/// docs` `params Name(optional or initialized values);` declared on the client: values that
/// several endpoints share, written once. An endpoint includes them with `..Name` in its value
/// list; every including endpoint also implements the generated trait `Name` (the group's
/// setters), so code written once can configure any of them.
pub struct Group {
    pub docs: Vec<Attribute>,
    pub name: Ident,
    pub params: Vec<Param>,
}

/// `..Name` in an endpoint's value list.
pub struct Spread {
    /// The number of values declared before it: where the group's values are spliced.
    pub at: usize,
    pub name: Ident,
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

/// A body or output format: an ordinary Rust type path (`Json`, `codecs::Csv`,
/// `IgnoreContentType<Json>`), resolved by Rust where it is written. The macro attaches no meaning
/// to its name: a format is chosen only by its statement, never by the payload type, and what it
/// does is its `Encode`/`Decode` implementation.
#[derive(Clone)]
pub struct Format(pub syn::Path);

impl Format {
    /// Where the format is written.
    pub fn span(&self) -> Span {
        self.0.span()
    }

    /// The format as written (`Json`, `codecs::Csv`), for diagnostics and documentation.
    pub fn text(&self) -> String {
        type_text(&self.0)
    }
}

/// An output: `-> T`, `-> stream T` or `-> stream`. Legacy format tokens are retained only for
/// migration errors.
pub struct Output {
    /// `None` only for `-> stream` without an item.
    pub ty: Option<Type>,
    /// `None`: the nearest `response` default.
    pub format: Option<Format>,
    /// The `stream` keyword: the output is opened incrementally, not decoded.
    pub stream: Option<Span>,
}

/// A uniform representation statement on any owner.
#[derive(Clone)]
pub struct FormatDefault {
    pub format: Format,
    pub media: Option<LitStr>,
    /// Written as `format F;` (both directions).
    pub both: bool,
    /// The statement's keyword (`request`, `response`, `format`), for diagnostics.
    pub span: Span,
}

/// A type or path as written, without `quote`'s token spacing (`Vec<Item>`, not `Vec < Item >`).
/// Only the spacing between tokens is chosen: every identifier (`r#type`) and literal
/// (`"a :: b"`) is its own token's text, so a suggestion made of it means what was written.
pub fn type_text(t: &dyn quote::ToTokens) -> String {
    token_text(t.to_token_stream(), true)
}

/// Whether `t`'s tokens hold a fragment forwarded by `macro_rules!` (a group without delimiters).
pub fn forwarded(t: &dyn quote::ToTokens) -> bool {
    fn any(tokens: proc_macro2::TokenStream) -> bool {
        tokens.into_iter().any(|tt| match tt {
            proc_macro2::TokenTree::Group(g) => {
                g.delimiter() == proc_macro2::Delimiter::None || any(g.stream())
            }
            _ => false,
        })
    }
    any(t.to_token_stream())
}

/// A payload type and, when given, its format, for code in a diagnostic, with the note to append.
/// A part holding a fragment forwarded by `macro_rules!` is named `T` or `F` instead of spelled:
/// no written text is that fragment (another macro's matcher sees it as one opaque token, so
/// `choose!($x)` with `$x = u8` does not match an arm `(u8)` that `choose!(u8)` matches).
pub fn advice(
    ty: &dyn quote::ToTokens,
    format: Option<&dyn quote::ToTokens>,
) -> (String, String, &'static str) {
    let t = forwarded(ty);
    let f = format.is_some_and(forwarded);
    let note = match (t, f) {
        (false, false) => "",
        (true, false) => {
            " (`T` is the payload type as declared: it holds a fragment forwarded by `macro_rules!`, which no written type reproduces exactly)"
        }
        (false, true) => {
            " (`F` is the format as declared: it holds a fragment forwarded by `macro_rules!`, which no written type reproduces exactly)"
        }
        (true, true) => {
            " (`T` and `F` are the payload type and format as declared: they hold fragments forwarded by `macro_rules!`, which no written type reproduces exactly)"
        }
    };
    let ty = if t { "T".to_owned() } else { type_text(ty) };
    let format = match format {
        Some(_) if f => "F".to_owned(),
        Some(format) => type_text(format),
        None => String::new(),
    };
    (ty, format, note)
}

/// An expression's text for documentation, spaced as Rust is usually written (`&ACCEPT`,
/// `HeaderValue::from_static("x")`, `vec![1, 2]`) rather than as a token stream
/// (`& ACCEPT`, `HeaderValue :: from_static ("x")`). Only the spacing differs from the tokens.
pub fn expr_text(tokens: proc_macro2::TokenStream) -> String {
    token_text(tokens, false)
}

/// The tokens' own texts, spaced by their neighbours. `types`: angle brackets are generics (in an
/// expression only a turbofish `::<` opens them); a braced const argument is an expression.
fn token_text(tokens: proc_macro2::TokenStream, types: bool) -> String {
    use proc_macro2::{Delimiter, Spacing, TokenTree};

    #[derive(Clone, Copy, PartialEq)]
    enum Prev {
        Start,
        Word,
        /// `mut`, `const`, `dyn` or `impl` in a type: what follows is spaced (`&mut [u8]`).
        Keyword,
        Group,
        /// A punctuation character; `true` when it is unary or joined to the next one.
        Punct(char, bool),
    }

    /// Whether `a` written right before `b` would lex differently than the two apart (one token,
    /// `..`, `&&`, `<-`, or a comment opener `//`, `/*`).
    fn lexes_with(a: char, b: char) -> bool {
        matches!(
            (a, b),
            (':', ':')
                | ('-' | '=', '>')
                | (
                    '=' | '!' | '<' | '>' | '+' | '-' | '*' | '/' | '%' | '^' | '&' | '|',
                    '='
                )
                | ('&', '&')
                | ('|', '|')
                | ('<', '<' | '-')
                | ('>', '>')
                | ('.', '.')
                | ('/', '/' | '*')
        )
    }

    /// The delimiters written for a group. A fragment forwarded by `macro_rules!` (`$e:expr`,
    /// `$t:ty`) groups its tokens without delimiters: bare when it is one token, or a type where a
    /// type is expected (`types`), parenthesized otherwise (`(1 + 2) * 2`, `(&DATA).len()`,
    /// `&(dyn A + B)`), so the text groups as the tokens did (as [`regroup`] does in the code).
    /// Executable advice does not spell such a fragment at all ([`advice`]).
    fn delimiters(g: &proc_macro2::Group, types: bool) -> (&'static str, &'static str) {
        match g.delimiter() {
            Delimiter::Parenthesis => ("(", ")"),
            Delimiter::Bracket => ("[", "]"),
            Delimiter::Brace => ("{ ", " }"),
            Delimiter::None if types && bare_fragment(g) => ("", ""),
            Delimiter::None if !types && !expression_fragment(g) => ("", ""),
            Delimiter::None => ("(", ")"),
        }
    }

    /// A macro's input, which may match any tokens: every token apart, as written, except after
    /// joined punctuation, before `,`/`;` and in a nested invocation's `name!(`.
    fn verbatim(tokens: proc_macro2::TokenStream, out: &mut String) {
        let (mut first, mut joint, mut word, mut bang) = (true, false, false, false);
        for tt in tokens {
            let space = !first
                && !joint
                && match &tt {
                    TokenTree::Punct(p) if matches!(p.as_char(), ',' | ';') => false,
                    TokenTree::Punct(p) if p.as_char() == '!' && word => false,
                    TokenTree::Group(g) if (word || bang) && g.delimiter() != Delimiter::Brace => {
                        false
                    }
                    _ => true,
                };
            if space {
                out.push(' ');
            }
            first = false;
            bang = matches!(&tt, TokenTree::Punct(p) if p.as_char() == '!') && word;
            joint = matches!(&tt, TokenTree::Punct(p) if p.spacing() == Spacing::Joint);
            word = matches!(&tt, TokenTree::Ident(_));
            match tt {
                TokenTree::Group(g) => {
                    let (open, close) = delimiters(&g, true);
                    out.push_str(open);
                    verbatim(g.stream(), out);
                    out.push_str(close);
                }
                other => out.push_str(&other.to_string()),
            }
        }
    }

    fn walk(tokens: proc_macro2::TokenStream, types: bool, out: &mut String) {
        let tokens: Vec<TokenTree> = tokens.into_iter().collect();
        let mut prev = Prev::Start;
        // The previous token is a macro's `!` (`name!`): the group after it is the macro's input.
        let mut bang = false;
        // Open generics (`Vec<`, or a turbofish `::<` in an expression): their `>` closes
        // without a space.
        let mut generics = 0usize;
        // The previous token, when it was a punctuation written apart from the next token.
        let mut alone: Option<char> = None;
        // A number keeps its space before a lone `.` when it is an unsuffixed integer (`1 . 2` is
        // not the float `1.`) and before any `.` when it ends with one (`1. ..` is not `1...`);
        // a literal after that `.` stays apart too.
        let (mut integer, mut trailing_dot, mut dot_after_number) = (false, false, false);
        for (i, tt) in tokens.iter().cloned().enumerate() {
            let space = match (&tt, prev) {
                (_, Prev::Start) => false,
                // Punctuation written apart stays apart when joining it would change the tokens
                // (closing generics aside: `>>` and `> >` close the same brackets).
                (TokenTree::Punct(p), _)
                    if alone.is_some_and(|a| lexes_with(a, p.as_char()))
                        && !(p.as_char() == '>' && alone == Some('>') && generics > 0) =>
                {
                    true
                }
                (TokenTree::Punct(p), _)
                    if p.as_char() == '.'
                        && (trailing_dot || integer && p.spacing() == Spacing::Alone) =>
                {
                    true
                }
                (TokenTree::Literal(_), _) if dot_after_number => true,
                // `< -1>` stays apart: `<-` is one token.
                (TokenTree::Punct(p), Prev::Punct('<', _)) if types && p.as_char() == '-' => true,
                (TokenTree::Punct(p), _) if p.as_char() == '>' && generics > 0 => false,
                (TokenTree::Punct(p), Prev::Word | Prev::Group) if types && p.as_char() == '<' => {
                    false
                }
                (_, Prev::Punct(_, true)) => false,
                (_, Prev::Punct('.', _)) => false,
                (TokenTree::Punct(p), _) if matches!(p.as_char(), '.' | ',' | ';' | '?' | ':') => {
                    false
                }
                (TokenTree::Punct(p), _) if p.as_char() == '!' && prev == Prev::Word => false,
                (TokenTree::Group(g), Prev::Word | Prev::Group | Prev::Punct('!' | '>', _))
                    if g.delimiter() != Delimiter::Brace =>
                {
                    false
                }
                _ => true,
            };
            if space {
                out.push(' ');
            }
            dot_after_number = (integer || trailing_dot)
                && matches!(&tt, TokenTree::Punct(p) if p.as_char() == '.');
            let number = match &tt {
                TokenTree::Literal(l) => l.to_string(),
                _ => String::new(),
            };
            integer = !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit() || b == b'_');
            trailing_dot =
                number.starts_with(|c: char| c.is_ascii_digit()) && number.ends_with('.');
            alone = match &tt {
                TokenTree::Punct(p) if p.spacing() == Spacing::Alone => Some(p.as_char()),
                _ => None,
            };
            let macro_input = bang;
            bang = matches!(&tt, TokenTree::Punct(p) if p.as_char() == '!') && prev == Prev::Word;
            prev = match tt {
                TokenTree::Group(g) => {
                    // A negative literal fragment is grouped only before a postfix operator.
                    let (open, close) = if !types && literal_fragment(&g) {
                        if postfix_follows(tokens.get(i + 1), tokens.get(i + 2)) {
                            ("(", ")")
                        } else {
                            ("", "")
                        }
                    } else {
                        delimiters(&g, types)
                    };
                    out.push_str(open);
                    if macro_input {
                        verbatim(g.stream(), out);
                    } else {
                        // A braced const argument (`Payload<{ N + 1 }>`) is an expression.
                        walk(g.stream(), types && g.delimiter() != Delimiter::Brace, out);
                    }
                    out.push_str(close);
                    Prev::Group
                }
                TokenTree::Punct(p) => {
                    let c = p.as_char();
                    out.push(c);
                    // `&x`, `-1`, `*p`, `!b` at the start or after an operator or `,` are unary.
                    let unary = matches!(c, '&' | '-' | '*' | '!')
                        && matches!(prev, Prev::Start | Prev::Punct(..));
                    // Both colons of `::` count as joined (no space after the path separator).
                    let path = c == ':' && prev == Prev::Punct(':', true);
                    // The end of a range operator (`..=`) binds like `.`.
                    let range = matches!(prev, Prev::Punct('.', _));
                    let opens = c == '<' && (types || prev == Prev::Punct(':', true));
                    // The `>` of `->` closes nothing.
                    let arrow = c == '>' && prev == Prev::Punct('-', true);
                    if opens {
                        generics += 1;
                    } else if c == '>' && generics > 0 && !arrow {
                        generics -= 1;
                    }
                    Prev::Punct(
                        c,
                        p.spacing() == Spacing::Joint || unary || opens || path || range,
                    )
                }
                other => {
                    let text = other.to_string();
                    out.push_str(&text);
                    if types && matches!(text.as_str(), "mut" | "const" | "dyn" | "impl") {
                        Prev::Keyword
                    } else {
                        Prev::Word
                    }
                }
            };
        }
    }

    let mut out = String::new();
    walk(tokens, types, &mut out);
    out
}

/// `via R` after a payload type: the removed spelling, reported with its exact replacement.
fn reject_via(input: ParseStream<'_>, ty: &Type, site: ViaSite<'_>) -> syn::Result<()> {
    if !input.peek(kw::via) {
        return Ok(());
    }
    let via: kw::via = input.parse()?;
    let repr: Type = input.parse()?;
    let (ty, format, note) = advice(ty, Some(&repr));
    let message = match site {
        ViaSite::Output => format!(
            "`via` was removed: write `-> {ty}` and `response {format};` on the endpoint{note}"
        ),
        ViaSite::Body { name, optional } => format!(
            "`via` was removed: write `body {name}{optional}: {ty}` and `request {format};` on the endpoint{note}"
        ),
    };
    Err(syn::Error::new(via.span, message))
}

#[derive(Clone, Copy)]
enum ViaSite<'a> {
    Output,
    Body { name: &'a Ident, optional: &'a str },
}

/// A format in statement position: a type path, resolved by Rust where it is written.
fn parse_format(input: ParseStream<'_>) -> syn::Result<Format> {
    input.parse().map(Format)
}

/// `("media/type")` after a format or `body`: the body's media type, checked here.
fn parse_media(input: ParseStream<'_>) -> syn::Result<LitStr> {
    let content;
    parenthesized!(content in input);
    let lit: LitStr = content.parse().map_err(|e| {
        syn::Error::new(
            e.span(),
            "expected the body's media type as a string, e.g. `body(\"application/merge-patch+json\") patch: Patch`",
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
        "an output takes no media type: the response format checks the response `Content-Type` itself and may suggest an `Accept`; to send a specific `Accept`, declare the header (`headers { \"accept\" = \"...\"; }`), and for other response media types use a format that accepts them",
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
    if let Err(e) = text.parse::<mime::Mime>() {
        return fail(&e.to_string());
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
    // `-> stream T` / `-> stream`: the keyword, unless `stream` begins a path, a generic type or
    // a type macro (`-> stream::Event`, `-> stream<T>`, `-> stream!()`; `r#stream` is never the
    // keyword).
    if input.peek(kw::stream)
        && !input.peek2(Token![::])
        && !input.peek2(Token![<])
        && !input.peek2(Token![!])
    {
        let keyword: kw::stream = input.parse()?;
        if at_end(input) {
            return Ok(Output {
                ty: None,
                format: None,
                stream: Some(keyword.span),
            });
        }
        let ty = parse_type(input)?;
        reject_via(input, &ty, ViaSite::Output)?;
        if !at_end(input) {
            return Err(syn::Error::new(
                input.span(),
                "expected the endpoint block after `-> stream T`: the stream's format is a statement (`response F;`, `F: Incremental<T>`), not a second type",
            ));
        }
        return Ok(Output {
            ty: Some(ty),
            format: None,
            stream: Some(keyword.span),
        });
    }
    // A path followed by more type tokens is the removed in-place format (`-> Csv Vec<Row>`,
    // reported by the model with its replacement); a path that ends the output is the payload;
    // a path followed by `!` is a type macro (`-> payload!()`, `-> m::payload![]`), a payload;
    // anything else (`()`, `[u8; 4]`, ...) is a payload.
    let fork = input.fork();
    let explicit = match fork.parse::<syn::Path>() {
        Ok(_) if !at_end(&fork) && !fork.peek(kw::via) && !fork.peek(Token![!]) => {
            Some(parse_format(input)?)
        }
        _ => None,
    };
    if explicit.is_some() && input.peek(syn::token::Paren) && peek_media(input) {
        return Err(output_media(input));
    }
    let ty = parse_type(input)?;
    reject_via(input, &ty, ViaSite::Output)?;
    // `-> () Csv`: a format after a payload that is not a path (paths are handled above).
    if explicit.is_none() && !at_end(input) && input.fork().parse::<syn::Path>().is_ok() {
        let format: syn::Path = input.parse()?;
        let (shown, format_text, note) = advice(&ty, Some(&format));
        return Err(syn::Error::new(
            format.span(),
            format!(
                "removed output format: write `-> {shown}` and `response {format_text};` on this endpoint{note}"
            ),
        ));
    }
    Ok(Output {
        ty: Some(ty),
        format: explicit,
        stream: None,
    })
}

pub struct Owner {
    pub kind: OwnerKind,
    pub docs: Vec<Attribute>,
    /// `#[cfg(..)]` and `#[deprecated..]` on scopes and endpoints.
    pub cfgs: Vec<Attribute>,
    pub deprecated: Option<Attribute>,
    /// `#[read]` on a scope or endpoint: it (every endpoint below it) only reads.
    pub read: Option<Attribute>,
    pub name: Ident,
    pub params: Vec<Param>,
    pub stmts: Stmts,
    pub children: Vec<Owner>,
    /// `..Name` entries (endpoints only), replaced by [`expand_groups`].
    pub spreads: Vec<Spread>,
    /// The groups spliced into this endpoint, in declaration order (after expansion).
    pub groups: Vec<Ident>,
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
    /// `send POLICY;` on the client: the type whose `SendWith<T>` every generated `send()` uses.
    /// `send P;` or `send P -> Template;` (`_` in the template stands for each endpoint's output).
    pub send: Option<(Type, Span, Option<Type>)>,
    /// `limit EXPR;` on any owner: the bound, in bytes, of a finite typed output's body.
    pub limit: Option<(Expr, Span)>,
    /// `page next link;` or `page next url a?.b;` on an endpoint: where the provider issues the
    /// next page's URL.
    pub next: Option<NextStmt>,
}

/// `page next ...;` on an endpoint.
pub struct NextStmt {
    pub span: Span,
    pub source: NextSource,
}

/// Where the provider issues the next page's URL.
pub enum NextSource {
    /// `page next link;`: the `Link` header's `rel="next"` target.
    Link,
    /// `page next url a?.b;`: a field of the decoded output, `?` stepping into an `Option`.
    Url(Vec<NextStep>),
}

/// One field of a `page next url` path.
pub struct NextStep {
    pub field: Ident,
    pub optional: bool,
}

#[derive(Clone)]
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

#[derive(Clone)]
pub struct Placement {
    pub family: Family,
    pub key: Option<Key>,
    pub each: bool,
    pub span: Span,
    /// `query(..) x: T`: the value writes its own pairs (`T: concord::QueryPairs`).
    pub pairs: bool,
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
            let (params, _, spreads) = parse_params(input, false)?;
            reject_spreads(&spreads)?;
            params
        } else {
            Vec::new()
        };
        let content;
        braced!(content in input);
        let mut base = None;
        let mut groups = Vec::new();
        let (stmts, children) = parse_owner_body(&content, Some(&mut base), Some(&mut groups))?;
        if !input.is_empty() {
            return Err(input.error("unexpected tokens after the client declaration; `api!` declares exactly one client"));
        }
        Ok(Api {
            client: Owner {
                kind: OwnerKind::Client { vis, base },
                docs,
                cfgs: Vec::new(),
                deprecated: None,
                read: None,
                name,
                params,
                stmts,
                children,
                spreads: Vec::new(),
                groups: Vec::new(),
            },
            groups,
        })
    }
}

fn reject_spreads(spreads: &[Spread]) -> syn::Result<()> {
    match spreads.first() {
        Some(spread) => Err(syn::Error::new(
            spread.name.span(),
            "a parameter group (`..Group`) is included by endpoints only: a scope's or the client's value reaches every endpoint below it",
        )),
        None => Ok(()),
    }
}

/// `params Name(..);`
fn parse_group(input: ParseStream<'_>) -> syn::Result<Group> {
    let attrs = input.call(Attribute::parse_outer)?;
    let docs = docs_only(attrs)?;
    input.parse::<kw::params>()?;
    let name: Ident = input.parse()?;
    if !input.peek(syn::token::Paren) {
        return Err(
            input.error("expected the group's values: `params Name(query limit?: u32, ..);`")
        );
    }
    let (params, _, spreads) = parse_params(input, false)?;
    if let Some(spread) = spreads.first() {
        return Err(syn::Error::new(
            spread.name.span(),
            "a parameter group cannot include another group; list its values",
        ));
    }
    input.parse::<Token![;]>()?;
    if params.is_empty() {
        return Err(syn::Error::new(
            name.span(),
            "a parameter group declares at least one value",
        ));
    }
    for param in &params {
        if !param.optional && param.default.is_none() {
            return Err(syn::Error::new(
                param.name.span(),
                format!(
                    "a parameter group holds optional (`{0}?: T`) or initialized (`{0}: T = v`) values: a required value is a positional argument, declared on each endpoint",
                    param.name
                ),
            ));
        }
    }
    Ok(Group { docs, name, params })
}

/// Splices every `..Name` with the values of the client's group `Name`, at the position written.
/// The spliced values keep the spans of the group's declaration, so a conflict (a value of the
/// same name on the endpoint) points at both.
pub fn expand_groups(api: &mut Api) -> syn::Result<()> {
    let mut seen: Vec<&Ident> = Vec::new();
    for group in &api.groups {
        if let Some(first) = seen.iter().find(|n| same(n, &group.name)) {
            let mut e = syn::Error::new(
                group.name.span(),
                format!("duplicate parameter group `{}`", group.name),
            );
            e.combine(syn::Error::new(first.span(), "first declared here"));
            return Err(e);
        }
        seen.push(&group.name);
    }
    let mut used = vec![false; api.groups.len()];
    expand_owner(&mut api.client, &api.groups, &mut used)?;
    if let Some((group, _)) = api.groups.iter().zip(&used).find(|(_, used)| !**used) {
        return Err(syn::Error::new(
            group.name.span(),
            format!(
                "parameter group `{}` is not included by any endpoint; include it with `..{0}` in an endpoint's values",
                group.name
            ),
        ));
    }
    Ok(())
}

/// The same identifier, whatever its spelling (`Paging` and `r#Paging` name one group).
fn same(a: &Ident, b: &Ident) -> bool {
    a.unraw() == b.unraw()
}

/// Levenshtein distance, for a "did you mean" suggestion between short identifiers.
fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let substitution = previous + usize::from(ca != *cb);
            previous = row[j + 1];
            row[j + 1] = substitution.min(row[j] + 1).min(row[j + 1] + 1);
        }
    }
    row[b.len()]
}

fn expand_owner(owner: &mut Owner, groups: &[Group], used: &mut [bool]) -> syn::Result<()> {
    let spreads = std::mem::take(&mut owner.spreads);
    if !spreads.is_empty() && !matches!(owner.kind, OwnerKind::Endpoint { .. }) {
        reject_spreads(&spreads)?;
    }
    // A value in two included groups, reported as such (not as an endpoint value).
    for (i, a) in spreads.iter().enumerate() {
        for b in &spreads[i + 1..] {
            let (Some(ga), Some(gb)) = (
                groups.iter().find(|g| same(&g.name, &a.name)),
                groups.iter().find(|g| same(&g.name, &b.name)),
            ) else {
                continue;
            };
            if same(&ga.name, &gb.name) {
                continue;
            }
            if let Some(value) = ga
                .params
                .iter()
                .find(|v| gb.params.iter().any(|w| same(&w.name, &v.name)))
            {
                return Err(syn::Error::new(
                    b.name.span(),
                    format!(
                        "value `{}` is in both parameter groups `{}` and `{}` included here; keep it in one",
                        value.name, a.name, b.name
                    ),
                ));
            }
        }
    }
    let declared: Vec<Ident> = owner.params.iter().map(|p| p.name.clone()).collect();
    // From the last spread to the first, so earlier insertion points stay valid.
    for spread in spreads.iter().rev() {
        let Some(index) = groups.iter().position(|g| same(&g.name, &spread.name)) else {
            let written = spread.name.to_string();
            let near = groups
                .iter()
                .map(|g| g.name.to_string())
                .find(|name| edit_distance(name, &written) <= 2);
            return Err(syn::Error::new(
                spread.name.span(),
                match near {
                    Some(near) => format!("no parameter group `{written}`; did you mean `{near}`?"),
                    None => format!(
                        "no parameter group `{written}`; declare it on the client: `params {written}(query x?: T, ..);`"
                    ),
                },
            ));
        };
        for value in &groups[index].params {
            if let Some(own) = owner
                .params
                .iter()
                .find(|p| same(&p.name, &value.name) && declared.iter().any(|d| same(d, &p.name)))
            {
                let mut e = syn::Error::new(
                    own.name.span(),
                    format!(
                        "value `{}` is declared on this endpoint and in its parameter group `{}`; keep one",
                        value.name, spread.name
                    ),
                );
                e.combine(syn::Error::new(
                    value.name.span(),
                    "declared in the group here",
                ));
                return Err(e);
            }
        }
        if owner.groups.iter().any(|g| same(g, &spread.name)) {
            return Err(syn::Error::new(
                spread.name.span(),
                format!("parameter group `{}` is included twice", spread.name),
            ));
        }
        used[index] = true;
        let values = groups[index].params.clone();
        let count = values.len();
        owner.params.splice(spread.at..spread.at, values);
        if let OwnerKind::Endpoint {
            body: Some(body), ..
        } = &mut owner.kind
            && spread.at <= body.position
        {
            body.position += count;
        }
        owner.groups.insert(0, spread.name.clone());
    }
    for child in &mut owner.children {
        expand_owner(child, groups, used)?;
    }
    Ok(())
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
    mut groups: Option<&mut Vec<Group>>,
) -> syn::Result<(Stmts, Vec<Owner>)> {
    let mut stmts = Stmts::default();
    let mut children = Vec::new();
    while !input.is_empty() {
        // `/// docs` `params Name(..);`: a parameter group (the client's only).
        let ahead = input.fork();
        let _ = ahead.call(Attribute::parse_outer);
        if ahead.peek(kw::params) && !ahead.peek2(Token![::]) {
            let at = ahead.span();
            let group = parse_group(input)?;
            match groups.as_deref_mut() {
                Some(groups) => groups.push(group),
                None => {
                    return Err(syn::Error::new(
                        at,
                        "parameter groups (`params Name(..);`) are declared on the client",
                    ));
                }
            }
            continue;
        }
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
        if input.peek(kw::send) {
            let kw: kw::send = input.parse()?;
            if base.is_none() {
                return Err(syn::Error::new(
                    kw.span,
                    "`send` is only allowed on the client: one send policy serves every endpoint",
                ));
            }
            if let Some(first) = children.first() {
                let first: &Owner = first;
                return Err(syn::Error::new(
                    kw.span,
                    format!(
                        "owner statements (`send`, `path`, `query`, ...) must precede child declarations (first child: `{}`)",
                        first.name
                    ),
                ));
            }
            if input.peek(Token![;]) {
                return Err(input.error(
                    "expected a send policy: a path to a type implementing `concord::SendWith<T>` (`send crate::Policy;`)",
                ));
            }
            let ty: Type = input.parse()?;
            // `send P -> Result<_, Error>;`: the result type every generated `send()` declares.
            let output = if input.peek(Token![->]) {
                input.parse::<Token![->]>()?;
                // Like other types: a fragment forwarded by `macro_rules!` keeps its grouping.
                Some(parse_type(input)?)
            } else {
                None
            };
            input.parse::<Token![;]>()?;
            if stmts.send.is_some() {
                return Err(syn::Error::new(
                    kw.span,
                    "`send` is set twice on the client",
                ));
            }
            stmts.send = Some((ty, kw.span, output));
            continue;
        }
        if input.peek(kw::page) && !input.peek2(Token![::]) {
            let kw: kw::page = input.parse()?;
            if let Some(first) = children.first() {
                let first: &Owner = first;
                return Err(syn::Error::new(
                    kw.span,
                    format!(
                        "owner statements (`page`, `path`, `query`, ...) must precede child declarations (first child: `{}`)",
                        first.name
                    ),
                ));
            }
            let expected = "expected `page next link;` or `page next url <field path>;`";
            if !input.peek(kw::next) {
                return Err(input.error(expected));
            }
            input.parse::<kw::next>()?;
            let source = if input.peek(kw::link) {
                input.parse::<kw::link>()?;
                NextSource::Link
            } else if input.peek(kw::url) {
                input.parse::<kw::url>()?;
                let mut steps = Vec::new();
                loop {
                    if !input.peek(Ident::peek_any) {
                        return Err(input.error(
                            "expected a field of the output: `page next url links?.pages?.next;`",
                        ));
                    }
                    let field = Ident::parse_any(input)?;
                    let optional = input.peek(Token![?]);
                    if optional {
                        input.parse::<Token![?]>()?;
                    }
                    steps.push(NextStep { field, optional });
                    if !input.peek(Token![.]) {
                        break;
                    }
                    input.parse::<Token![.]>()?;
                }
                NextSource::Url(steps)
            } else {
                return Err(input.error(expected));
            };
            input.parse::<Token![;]>()?;
            let stmt = NextStmt {
                span: kw.span,
                source,
            };
            set_once(&mut stmts.next, stmt, kw.span, "page next")?;
            continue;
        }
        if input.peek(kw::limit) && !input.peek2(Token![::]) {
            let kw: kw::limit = input.parse()?;
            if let Some(first) = children.first() {
                let first: &Owner = first;
                return Err(syn::Error::new(
                    kw.span,
                    format!(
                        "owner statements (`limit`, `path`, `query`, ...) must precede child declarations (first child: `{}`)",
                        first.name
                    ),
                ));
            }
            if input.peek(Token![;]) {
                return Err(input.error(
                    "expected the bound in bytes, a `usize` constant expression (`limit 4 * 1024 * 1024;`)",
                ));
            }
            // Parsed like an initializer: a fragment forwarded by `macro_rules!` keeps its
            // grouping (`16 / $d` with `$d = 1 + 3` is `16 / (1 + 3)`).
            let expr = parse_initializer(input)?;
            input.parse::<Token![;]>()?;
            if stmts.limit.is_some() {
                return Err(syn::Error::new(
                    kw.span,
                    "duplicate `limit` statement; each owner declares it at most once",
                ));
            }
            stmts.limit = Some((expr, kw.span));
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
                        "owner statements (`path`, `subdomain`, `query`, `headers`, `format`, `request`, `response`) must precede child declarations (first child: `{}`)",
                        first.name
                    ),
                ));
            }
            if input.peek(kw::request) || input.peek(kw::response) || input.peek(kw::format) {
                parse_default(input, &mut stmts)?;
            } else {
                parse_stmt(input, &mut stmts)?;
            }
            continue;
        }
        children.push(parse_child(input)?);
    }
    Ok((stmts, children))
}

/// `format F;`, `request F;`, `request F "media/type";`, `response F;`: the formats of the bodies
/// and typed outputs of this owner and below, the nearest statement winning.
fn parse_default(input: ParseStream<'_>, stmts: &mut Stmts) -> syn::Result<()> {
    let both = input.peek(kw::format);
    let request = both || input.peek(kw::request);
    let span = if both {
        input.parse::<kw::format>()?.span
    } else if request {
        input.parse::<kw::request>()?.span
    } else {
        input.parse::<kw::response>()?.span
    };
    if input.peek(Token![;]) {
        return Err(input.error(
            "expected a format type: a path to a type implementing `Encode` and/or `Decode` (`format F;`, `request F;`, `response F;`)",
        ));
    }
    if input.peek(LitStr) {
        return Err(input.error(
            "a media type follows its format, which the statement restates: `request F \"application/merge-patch+json\";` (a statement replaces the inherited format and media type together)",
        ));
    }
    let format = parse_format(input)?;
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
                "an output takes no media type: the response format checks the response `Content-Type` itself and may suggest an `Accept`; to send a specific `Accept`, declare the header (`headers { \"accept\" = \"...\"; }`), and for other response media types use a format that accepts them",
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
        span,
    };
    // One message for a direction set twice, whichever statement came first.
    let taken = |slot: &Option<FormatDefault>, direction: &str| -> syn::Result<()> {
        match slot {
            Some(first) if first.both || both => Err(syn::Error::new(
                span,
                format!(
                    "duplicate `{direction}` direction: `format` counts as both `request` and `response`, at most once each per owner; for a different media type write `request F \"type/subtype\";` and `response F;` instead of `format`"
                ),
            )),
            _ => Ok(()),
        }
    };
    if request {
        taken(&stmts.request, "request")?;
    }
    if both || !request {
        taken(&stmts.response, "response")?;
    }
    if both {
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

/// Children accept docs, `#[cfg(..)]` (whole subtree), `#[deprecated..]` (navigation method
/// only) and `#[read]` (whole subtree); every other attribute is still rejected.
type ChildAttrs = (
    Vec<Attribute>,
    Vec<Attribute>,
    Option<Attribute>,
    Option<Attribute>,
);

fn child_attrs(attrs: Vec<Attribute>) -> syn::Result<ChildAttrs> {
    let (mut docs, mut cfgs, mut deprecated, mut read) = (Vec::new(), Vec::new(), None, None);
    for attr in attrs {
        if attr.path().is_ident("cfg") {
            cfgs.push(attr);
        } else if attr.path().is_ident("read") {
            if !matches!(attr.meta, syn::Meta::Path(_)) {
                return Err(syn::Error::new(
                    attr.span(),
                    "`#[read]` takes no arguments: it states that every endpoint it covers only reads",
                ));
            }
            if read.is_some() {
                return Err(syn::Error::new(attr.span(), "duplicate `#[read]`"));
            }
            read = Some(attr);
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
                "unsupported attribute on a scope or endpoint; documentation comments, `#[cfg(..)]`, `#[deprecated]` and `#[read]` are accepted",
            ));
        }
    }
    Ok((docs, cfgs, deprecated, read))
}

fn parse_child(input: ParseStream<'_>) -> syn::Result<Owner> {
    let attrs = input.call(Attribute::parse_outer)?;
    let (docs, cfgs, deprecated, read) = child_attrs(attrs)?;
    if input.peek(kw::scope) {
        input.parse::<kw::scope>()?;
        let name: Ident = input.parse()?;
        let params = if input.peek(syn::token::Paren) {
            let (params, _, spreads) = parse_params(input, false)?;
            reject_spreads(&spreads)?;
            params
        } else {
            Vec::new()
        };
        let content;
        braced!(content in input);
        let (stmts, children) = parse_owner_body(&content, None, None)?;
        return Ok(Owner {
            kind: OwnerKind::Scope,
            docs,
            cfgs,
            deprecated,
            read,
            name,
            params,
            stmts,
            children,
            spreads: Vec::new(),
            groups: Vec::new(),
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
    let (params, body, spreads) = parse_params(input, true)?;
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
        // `-> item! { response F; }`: the braces were the type macro's own.
        if let Some(Output {
            ty: Some(Type::Macro(m)),
            ..
        }) = &output
            && matches!(m.mac.delimiter, syn::MacroDelimiter::Brace(_))
            && !input.peek(syn::token::Brace)
        {
            return Err(syn::Error::new(
                m.span(),
                format!(
                    "expected `;` or this endpoint's `{{ .. }}` after its output type: the braces after `{}!` belong to the type macro; write `-> {0}!()` or `-> {0}! {{}} {{ .. }}`",
                    type_text(&m.mac.path)
                ),
            ));
        }
        let content;
        braced!(content in input);
        let (stmts, children) = parse_owner_body(&content, None, None)?;
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
        read,
        name,
        params,
        stmts,
        children: Vec::new(),
        spreads,
        groups: Vec::new(),
    })
}

fn parse_params(
    input: ParseStream<'_>,
    allow_body: bool,
) -> syn::Result<(Vec<Param>, Option<BodyParam>, Vec<Spread>)> {
    let content;
    parenthesized!(content in input);
    let mut params = Vec::new();
    let mut body: Option<BodyParam> = None;
    let mut spreads = Vec::new();
    while !content.is_empty() {
        // `..Group`: the values of a parameter group, spliced here.
        if content.peek(Token![..]) {
            content.parse::<Token![..]>()?;
            let name: Ident = content.parse().map_err(|e| {
                syn::Error::new(e.span(), "expected a parameter group's name after `..`")
            })?;
            spreads.push(Spread {
                at: params.len(),
                name,
            });
            if content.is_empty() {
                break;
            }
            content.parse::<Token![,]>()?;
            continue;
        }
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
            // A body: `body name: T`; `body("media") name: T`, `F name: T` and
            // `F("media") name: T` are removed spellings, reported with their replacement.
            let (format, at) = if content.peek(kw::body) && !content.peek2(Token![::]) {
                (None, content.parse::<kw::body>()?.span)
            } else if allow_body {
                // A bare lowercase identifier is more likely a misspelled placement
                // (`qurey page: u32`) than a format, which is a statement anyway.
                let path: syn::Path = content.fork().parse()?;
                if let Some(ident) = path.get_ident()
                    && ident
                        .to_string()
                        .starts_with(|c: char| c.is_ascii_lowercase())
                {
                    return Err(syn::Error::new(
                        ident.span(),
                        format!(
                            "`{ident}` is not a placement: a parameter is `name: T`, `query name: T`, `query(\"key\") name: T`, `header(KEY) name: T` or `body name: T` (a body's format is a statement: `request F;`)"
                        ),
                    ));
                }
                let format = parse_format(&content)?;
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
            let ty = parse_type(&content)?;
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
            let ty = parse_type(&content)?;
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
                if !content.is_empty()
                    && !content.peek(Token![=])
                    && !content.peek(Token![,])
                    && content.fork().parse::<syn::Path>().is_ok()
                {
                    let format: syn::Path = content.parse()?;
                    let (shown, format_text, note) = advice(&ty, Some(&format));
                    return Err(syn::Error::new(
                        format.span(),
                        format!(
                            "removed body format: write `body {}{}: {shown}` and `request {format_text};` on this endpoint (a format is a type path: `{format_text}` must name a format type in scope){note}",
                            // As written: the suggestion is code, so `r#type` stays raw.
                            name,
                            if optional { "?" } else { "" },
                        ),
                    ));
                }
            }
            let default = if content.peek(Token![=]) {
                content.parse::<Token![=]>()?;
                Some(parse_initializer(&content)?)
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
    Ok((params, body, spreads))
}

/// What `parse` reads from `input`, and the tokens it consumed as written (`None` if it ended
/// inside a group).
fn consumed<T>(
    input: ParseStream<'_>,
    parse: impl FnOnce(ParseStream<'_>) -> syn::Result<T>,
) -> syn::Result<(T, Option<proc_macro2::TokenStream>)> {
    let start = input.cursor();
    let parsed = parse(input)?;
    let end = input.cursor();
    let mut written = proc_macro2::TokenStream::new();
    let mut at = start;
    while at != end {
        match at.token_tree() {
            Some((tt, next)) => {
                written.extend([tt]);
                at = next;
            }
            None => return Ok((parsed, None)),
        }
    }
    Ok((parsed, Some(written)))
}

/// A value's initializer: checked as an expression, kept as the tokens written with forwarded
/// fragments regrouped ([`regroup`]); the parsed expression printed back has lost their grouping.
fn parse_initializer(input: ParseStream<'_>) -> syn::Result<Expr> {
    Ok(match consumed(input, |input| input.parse::<Expr>())? {
        (_, Some(written)) => Expr::Verbatim(regroup(written, true)),
        (expr, None) => expr,
    })
}

/// A payload or value type. When its tokens hold forwarded fragments it is parsed again from
/// them, regrouped ([`regroup`]): `syn` reads such a fragment in a const argument as ungrouped.
fn parse_type(input: ParseStream<'_>) -> syn::Result<Type> {
    Ok(match consumed(input, |input| input.parse::<Type>())? {
        (ty, Some(written)) if forwarded(&written) => {
            syn::parse2(regroup(written, false)).unwrap_or(ty)
        }
        (ty, _) => ty,
    })
}

/// Whether a group is a fragment forwarded by `macro_rules!` (no delimiters) holding a negative
/// literal (`-1isize`); written brackets, braces or parentheses (`[-1]`, `{ -1 }`, `m!(-1)`) are not.
fn literal_fragment(g: &proc_macro2::Group) -> bool {
    use proc_macro2::{Delimiter, TokenTree};
    let tokens: Vec<TokenTree> = g.stream().into_iter().collect();
    g.delimiter() == Delimiter::None
        && matches!(tokens.as_slice(), [TokenTree::Punct(minus), TokenTree::Literal(_)] if minus.as_char() == '-')
}

/// Whether the tokens after a negative literal fragment bind tighter than its `-`: a method call
/// or field (`.`, not the `..` of a range), an index or `?` (`$n.abs()` is `(-1).abs()`). A range
/// pattern's bound (`$n..=1`), where parentheses are not allowed, is followed by `..`.
fn postfix_follows(
    next: Option<&proc_macro2::TokenTree>,
    after: Option<&proc_macro2::TokenTree>,
) -> bool {
    use proc_macro2::{Delimiter, Spacing, TokenTree};
    match next {
        Some(TokenTree::Punct(p)) if p.as_char() == '?' => true,
        Some(TokenTree::Punct(p)) if p.as_char() == '.' => {
            !(p.spacing() == Spacing::Joint
                && matches!(after, Some(TokenTree::Punct(q)) if q.as_char() == '.'))
        }
        Some(TokenTree::Group(g)) => g.delimiter() == Delimiter::Bracket,
        _ => false,
    }
}

/// Whether a fragment forwarded by `macro_rules!`, where an expression is expected, needs its
/// grouping as parentheses: an expression of several tokens, or a type with `+`
/// (`size_of::<&(dyn Send + Sync)>()`). Left as it came: a path, one primary expression that may
/// head a struct literal (`$p { n: 7 }`); a negative literal, which may bound a range pattern,
/// where parentheses are not allowed (grouped only before a [postfix operator](postfix_follows));
/// a lifetime, a pattern or another type that is no expression (`'static`, `&'static str`,
/// `Vec<u8>`). A type that also reads as an expression (`[u8; 3]`, `&T`) means the same in
/// parentheses.
fn expression_fragment(g: &proc_macro2::Group) -> bool {
    let stream = g.stream();
    stream.clone().into_iter().nth(1).is_some()
        && !literal_fragment(g)
        && syn::parse::Parser::parse2(syn::Path::parse, stream.clone()).is_err()
        && (syn::parse::Parser::parse2(Expr::parse, stream.clone()).is_ok()
            || syn::parse::Parser::parse2(Type::parse, stream.clone()).is_ok()
                && syn::parse::Parser::parse2(Type::without_plus, stream).is_err())
}

/// Whether a fragment forwarded by `macro_rules!` (a group without delimiters) means the same
/// without its group: one token, or a type without `+`.
fn bare_fragment(g: &proc_macro2::Group) -> bool {
    let stream = g.stream();
    stream.clone().into_iter().nth(1).is_none()
        || syn::parse::Parser::parse2(Type::without_plus, stream).is_ok()
}

/// `tokens` with forwarded fragments grouped as rustc grouped them. A proc macro cannot hand such
/// a fragment back as rustc made it, and rustc reads a group without delimiters from a proc macro
/// as ungrouped: `$e * 2` with `$e = 1 + 2` would be 5, not 6, and `$e.len()` with `$e = &DATA`
/// would be `&DATA.len()`. Where an expression is expected (`expr`: an initializer, a braced const
/// argument, an array length) a fragment that is an [expression or a type with `+`](expression_fragment)
/// is put in parentheses; a literal, a lifetime, a path or another type is left as it came. Where a
/// type is expected only a fragment that is not a [type](bare_fragment) is. The added parentheses
/// carry the macro's call-site span, so rustc does not report them as unneeded in the user's code
/// (`size_of::<([u8; 3])>()`). A macro's input (`name!(..)`) is kept as it came: there the
/// fragment is still one, opaque to that macro's matcher.
fn regroup(tokens: proc_macro2::TokenStream, expr: bool) -> proc_macro2::TokenStream {
    regroup_in(tokens, expr, false)
}

/// [`regroup`]; `array`: the contents of `[T; N]` in a type, an expression after its `;`.
fn regroup_in(
    tokens: proc_macro2::TokenStream,
    mut expr: bool,
    array: bool,
) -> proc_macro2::TokenStream {
    use proc_macro2::{Delimiter, Group, Span, TokenTree};
    let (mut word, mut bang) = (false, false);
    let mut out = proc_macro2::TokenStream::new();
    let tokens: Vec<TokenTree> = tokens.into_iter().collect();
    for (i, tt) in tokens.iter().cloned().enumerate() {
        let (next, after) = (tokens.get(i + 1), tokens.get(i + 2));
        let macro_input = bang;
        bang = matches!(&tt, TokenTree::Punct(p) if p.as_char() == '!') && word;
        word = matches!(&tt, TokenTree::Ident(_));
        if array && matches!(&tt, TokenTree::Punct(p) if p.as_char() == ';') {
            expr = true;
        }
        let rebuilt = |g: &Group, delimiter: Delimiter, span: Span, expr: bool, array: bool| {
            let mut group = Group::new(delimiter, regroup_in(g.stream(), expr, array));
            group.set_span(span);
            TokenTree::Group(group)
        };
        out.extend([match tt {
            TokenTree::Group(g) if macro_input => TokenTree::Group(g),
            TokenTree::Group(g) if g.delimiter() == Delimiter::None => {
                let bare = if !expr {
                    bare_fragment(&g)
                } else if literal_fragment(&g) {
                    !postfix_follows(next, after)
                } else {
                    !expression_fragment(&g)
                };
                match (bare, forwarded(&g.stream())) {
                    (true, false) => TokenTree::Group(g),
                    // A type that itself holds fragments (`Payload<{ $e * 2 }>` as `$t`).
                    (true, true) => rebuilt(&g, Delimiter::None, g.span(), expr, false),
                    (false, _) => {
                        rebuilt(&g, Delimiter::Parenthesis, Span::call_site(), true, false)
                    }
                }
            }
            TokenTree::Group(g) if !forwarded(&g.stream()) => TokenTree::Group(g),
            TokenTree::Group(g) => {
                let (delimiter, span) = (g.delimiter(), g.span());
                match delimiter {
                    Delimiter::Brace => rebuilt(&g, delimiter, span, true, false),
                    Delimiter::Bracket if !expr => rebuilt(&g, delimiter, span, false, true),
                    _ => rebuilt(&g, delimiter, span, expr, false),
                }
            }
            other => other,
        }]);
    }
    out
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
    let mut pairs = false;
    let key = if input.peek(syn::token::Paren) {
        let content;
        parenthesized!(content in input);
        if content.peek(Token![..]) {
            let dots: Token![..] = content.parse()?;
            if !content.is_empty() {
                return Err(
                    content.error("`query(..)` takes no key: the value writes its own pairs")
                );
            }
            if family.0 == Family::Header {
                return Err(syn::Error::new(
                    dots.spans[0],
                    "`header(..)` is not supported: a header placement names its field",
                ));
            }
            pairs = true;
        }
        if pairs {
            None
        } else {
            let key = parse_key(
                &content,
                "expected a quoted name (`header(\"x-request-id\")`) or a native `HeaderName` constant (`header(AUTHORIZATION)`)",
            )?;
            if !content.is_empty() {
                return Err(content.error("expected a single key"));
            }
            Some(key)
        }
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
    if pairs && each {
        return Err(syn::Error::new(
            family.1,
            "`query(..)` writes its own pairs, so it takes no `each`: write every pair in the value's `QueryPairs` implementation",
        ));
    }
    Ok(Some(Placement {
        family: family.0,
        key,
        each,
        span: family.1,
        pairs,
    }))
}

#[cfg(test)]
mod tests {
    use super::type_text;

    /// A `via` path is suggested exactly as written: a format is an ordinary type path, so a
    /// lowercase or raw identifier keeps its meaning (`response r#type;` parses) — re-check F3e.
    #[test]
    fn via_replacement_names_the_same_path() {
        let text = |t: syn::Type| type_text(&t);
        assert_eq!(text(syn::parse_quote!(concord::Json)), "concord::Json");
        assert_eq!(text(syn::parse_quote!(Text)), "Text");
        assert_eq!(text(syn::parse_quote!(::c4::Text)), "::c4::Text");
        assert_eq!(text(syn::parse_quote!(text)), "text");
        assert_eq!(text(syn::parse_quote!(r#type)), "r#type");
        let path: syn::Path = syn::parse_str("r#type").unwrap();
        assert_eq!(type_text(&path), "r#type");
    }

    /// F03: literal contents are never respaced (`"a :: b"` has six bytes, `"a::b"` four), in
    /// any literal kind or nesting; only the spacing between tokens is chosen.
    #[test]
    fn literals_keep_their_contents() {
        let text = |s: &str| type_text(&syn::parse_str::<syn::Type>(s).unwrap());
        assert_eq!(
            text(r#"Payload<{ count("a :: b") }>"#),
            r#"Payload<{ count("a :: b") }>"#
        );
        for literal in [
            r#""a :: b < c > d , e & f""#,
            r#"r"x :: y""#,
            r##"r#"q :: " , & "#"##,
            r#"b"a :: b""#,
            r#"br"a , b""#,
            r#"c"a :: b""#,
            "'<'",
            "b' '",
        ] {
            let written = format!("Payload<{{ count({literal}) }}>");
            let shown = text(&written);
            assert!(shown.contains(literal), "{literal} in {shown}");
            // The suggestion parses back to the same tokens.
            let back: syn::Type = syn::parse_str(&shown).unwrap();
            let original: syn::Type = syn::parse_str(&written).unwrap();
            assert_eq!(
                quote::quote!(#back).to_string(),
                quote::quote!(#original).to_string()
            );
        }
        assert_eq!(
            text(r#"Outer<Inner<{ f("a :: b") }>, [u8; { g(" , ") }]>"#),
            r#"Outer<Inner<{ f("a :: b") }>, [u8; { g(" , ") }]>"#
        );
    }

    /// Ordinary types read as Rust is written; raw identifiers and type macros stay as written.
    #[test]
    fn types_are_spaced_as_written() {
        let text = |s: &str| type_text(&syn::parse_str::<syn::Type>(s).unwrap());
        for (written, shown) in [
            ("Vec < Option < Item > >", "Vec<Option<Item>>"),
            (
                "std :: collections :: HashMap < String , u32 >",
                "std::collections::HashMap<String, u32>",
            ),
            ("& 'static str", "&'static str"),
            ("& 'a mut [u8 ; 4]", "&'a mut [u8; 4]"),
            ("(u8 , String)", "(u8, String)"),
            ("()", "()"),
            ("* const u8", "*const u8"),
            (
                "Box < dyn Fn (u8) -> Vec < u8 > + Send >",
                "Box<dyn Fn(u8) -> Vec<u8> + Send>",
            ),
            ("< T as Trait > :: Assoc", "<T as Trait>::Assoc"),
            ("Option < r#type :: r#match >", "Option<r#type::r#match>"),
            ("payload ! ()", "payload!()"),
            ("m :: payload ! [u8]", "m::payload![u8]"),
            ("Foo < - 1 >", "Foo< -1>"),
        ] {
            assert_eq!(text(written), shown, "{written}");
        }
    }

    /// Review of F03 (4.2): punctuation written apart is never joined into another token, so a
    /// macro matching `(. .)` is not shown as `(..)`. A macro's input may match any tokens, so it
    /// is shown token for token (only spacing that cannot change the tokens is dropped); numbers
    /// keep the space a following `.` needs.
    #[test]
    fn separate_punctuation_stays_separate() {
        let text = |s: &str| type_text(&syn::parse_str::<syn::Type>(s).unwrap());
        for (written, shown) in [
            ("choose!(. .)", "choose!(. .)"),
            ("choose!(..)", "choose!(..)"),
            ("choose!(a < b < c > > d)", "choose!(a < b < c > > d)"),
            ("choose!(a < b < c >> d)", "choose!(a < b < c >> d)"),
            ("choose!(1. . 2)", "choose!(1. . 2)"),
            ("m!(& &x)", "m!(& & x)"),
            ("m!(&&x)", "m!(&& x)"),
            ("m!(a / / b)", "m!(a / / b)"),
            ("m!(a / *b)", "m!(a / * b)"),
            ("m!(a < - b)", "m!(a < - b)"),
            ("m!(a = > b)", "m!(a = > b)"),
            ("m!(a => b)", "m!(a => b)"),
            ("m!(a : : b)", "m!(a : : b)"),
            ("m!(1 . 2)", "m!(1 . 2)"),
            ("m!(x.0, y; inner!(z))", "m!(x . 0, y; inner!(z))"),
            // Outside a macro's input: generics, and numbers before `.` in a const argument.
            ("Vec < Vec < u8 > >", "Vec<Vec<u8>>"),
            (
                "Foo < { f(1 . max(2), 1. ..2, 1 ..= 3, 1.5 . max(2)) } >",
                "Foo<{ f(1 .max(2), 1. ..2, 1..=3, 1.5.max(2)) }>",
            ),
        ] {
            let shown_text = text(written);
            assert_eq!(shown_text, shown, "{written}");
            // The same tokens back, jointness included.
            let tokens = |t: &str| {
                fn flat(s: proc_macro2::TokenStream, out: &mut Vec<String>) {
                    for tt in s {
                        match tt {
                            proc_macro2::TokenTree::Group(g) => flat(g.stream(), out),
                            proc_macro2::TokenTree::Punct(p) => {
                                out.push(format!("{}{:?}", p.as_char(), p.spacing()))
                            }
                            tt => out.push(tt.to_string()),
                        }
                    }
                }
                let mut out = Vec::new();
                flat(t.parse().unwrap(), &mut out);
                out
            };
            if written.contains('!') {
                assert_eq!(tokens(&shown_text), tokens(written), "{written}");
            }
        }
    }

    /// Review of F03 (4.2): a fragment forwarded by `macro_rules!` is a group without delimiters;
    /// its text keeps the grouping (`(1 + 2) * 2`, not `1 + 2 * 2`), and a type or a single token
    /// stays bare.
    #[test]
    fn invisible_groups_keep_their_grouping() {
        use proc_macro2::{Delimiter, Group, TokenStream, TokenTree};
        let group = |tokens: TokenStream| TokenTree::Group(Group::new(Delimiter::None, tokens));
        let sum = group(quote::quote!(1 + 2));
        assert_eq!(
            type_text(&quote::quote!(Payload<{ pick!(#sum * 2) }>)),
            "Payload<{ pick!((1 + 2) * 2) }>"
        );
        assert_eq!(super::expr_text(quote::quote!(#sum * 2)), "(1 + 2) * 2");
        let ty = group(quote::quote!(Vec<u8>));
        assert_eq!(type_text(&quote::quote!(Option<#ty>)), "Option<Vec<u8>>");
        let path = group(quote::quote!(crate::Fmt));
        assert_eq!(type_text(&path), "crate::Fmt");
        let object = group(quote::quote!(dyn Send + Sync));
        assert_eq!(type_text(&quote::quote!(&#object)), "&(dyn Send + Sync)");
        let one = group(quote::quote!(3));
        assert_eq!(type_text(&quote::quote!([u8; #one])), "[u8; 3]");
    }
}
