//! Pagination Init/Cursor syntax (§14.1).

use syn::parse::ParseStream;
use syn::spanned::Spanned;
use syn::{
    Ident, LitBool, LitChar, LitFloat, LitInt, LitStr, Path, Token, Type, braced, parenthesized,
};

use super::ast::{InitValue, PaginationDecl};
use super::diagnostics::{Code, coded, keyword_suggestion, related};
use super::keyword;
use super::request::parse_reference;
use super::values::parse_stable_value;
use crate::semantic::surface::canonical_ident;

pub(crate) fn parse_pagination(input: ParseStream<'_>, owner: &str) -> syn::Result<PaginationDecl> {
    input.parse::<keyword::paginate>()?;
    input.parse::<keyword::via>().map_err(|_| {
        input.error(format!(
            "[C3-PAGE] pagination of {owner} is `paginate via Controller {{ init Init {{ ... }}; cursor -> slot; }}`{}",
            keyword_suggestion(input, &["via"])
        ))
    })?;
    let controller: Type = input.parse()?;
    let body;
    braced!(body in input);
    if !body.peek(keyword::init) {
        if body.fork().parse::<Ident>().is_ok() && (body.peek2(Token![->]) || body.peek2(Token![=]))
        {
            return Err(coded(
                body.span(),
                Code::Migrate,
                format!(
                    "v2 public controller-field bindings were replaced: write `init InitType {{ field: value, ... }}; cursor -> slot;` in {owner}"
                ),
            ));
        }
        return Err(coded(
            body.span(),
            Code::Page,
            format!(
                "the pagination block of {owner} begins with `init InitType {{ field: value, ... }};`{}",
                keyword_suggestion(&body, &["init"])
            ),
        ));
    }
    body.parse::<keyword::init>()?;
    let init_type: Path = body.parse()?;
    let fields_content;
    braced!(fields_content in body);
    let mut init_fields: Vec<(Ident, InitValue)> = Vec::new();
    while !fields_content.is_empty() {
        if fields_content.peek(Token![..]) {
            return Err(coded(
                fields_content.span(),
                Code::Page,
                format!(
                    "`..Default::default()` is not DSL syntax (Init of {owner}); supply every Init field or name a Rust constant"
                ),
            ));
        }
        let field: Ident = fields_content.parse()?;
        fields_content.parse::<Token![:]>()?;
        let value = if fields_content.peek(LitStr)
            || fields_content.peek(LitChar)
            || fields_content.peek(LitBool)
            || fields_content.peek(LitInt)
            || fields_content.peek(LitFloat)
            || fields_content.peek(Token![-])
            || is_constant_path(&fields_content)
        {
            InitValue::Stable(parse_stable_value(&fields_content, owner)?)
        } else {
            InitValue::Reference(parse_reference(&fields_content, owner)?)
        };
        if let Some((existing, _)) = init_fields
            .iter()
            .find(|(existing, _)| canonical_ident(existing) == canonical_ident(&field))
        {
            let error = coded(
                field.span(),
                Code::Page,
                format!(
                    "duplicate Init field `{}` in the pagination of {owner}; assign each field once",
                    canonical_ident(&field)
                ),
            );
            return Err(related(error, existing.span(), "first assignment here"));
        }
        init_fields.push((field, value));
        if fields_content.is_empty() {
            break;
        }
        fields_content.parse::<Token![,]>()?;
    }
    body.parse::<Token![;]>()?;
    if !body.peek(keyword::cursor) {
        return Err(coded(
            body.span(),
            Code::Page,
            format!(
                "the pagination block of {owner} ends with `cursor -> slot;` or `cursor -> (a, b);`{}",
                keyword_suggestion(&body, &["cursor"])
            ),
        ));
    }
    let cursor_span = body.span();
    body.parse::<keyword::cursor>()?;
    body.parse::<Token![->]>()?;
    let (cursor, tuple) = if body.peek(syn::token::Paren) {
        let content;
        parenthesized!(content in body);
        let mut names: Vec<Ident> = Vec::new();
        while !content.is_empty() {
            names.push(content.parse()?);
            if content.is_empty() {
                break;
            }
            content.parse::<Token![,]>()?;
        }
        if names.len() < 2 {
            return Err(coded(
                cursor_span,
                Code::Page,
                format!(
                    "a tuple cursor destination of {owner} names at least two endpoint slots; write `cursor -> slot;` for one slot"
                ),
            ));
        }
        (names, true)
    } else {
        let name: Ident = body.parse()?;
        if body.peek(Token![.]) {
            let fork = body.fork();
            let slot = fork
                .parse::<Token![.]>()
                .and_then(|_| fork.parse::<Ident>())
                .map_or_else(|_| "slot".to_owned(), |slot| canonical_ident(&slot));
            return Err(coded(
                name.span(),
                Code::Page,
                format!(
                    "cursor destinations of {owner} are bare endpoint-owned slots, never `endpoint.x`, client or scope values; declare `{slot}` as an endpoint parameter and write `cursor -> {slot};`"
                ),
            ));
        }
        (vec![name], false)
    };
    body.parse::<Token![;]>()?;
    if !body.is_empty() {
        return Err(coded(
            body.span(),
            Code::Page,
            format!(
                "unexpected tokens after `cursor -> ...;` in the pagination of {owner}; the cursor mapping is the last statement"
            ),
        ));
    }
    Ok(PaginationDecl {
        controller,
        init_type,
        init_fields,
        cursor,
        tuple,
        cursor_span: cursor_span.span(),
    })
}

/// A path whose last segment starts with an uppercase letter or contains `::`
/// is treated as a Rust constant; a bare lowercase identifier or `owner.name`
/// is a value reference.
fn is_constant_path(input: ParseStream<'_>) -> bool {
    let fork = input.fork();
    let Ok(path) = fork.parse::<Path>() else {
        return false;
    };
    if fork.peek(Token![.]) {
        return false;
    }
    path.segments.len() > 1
        || path.segments.last().is_some_and(|segment| {
            segment
                .ident
                .to_string()
                .chars()
                .next()
                .is_some_and(char::is_uppercase)
        })
}
