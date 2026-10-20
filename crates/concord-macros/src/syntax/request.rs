//! Route, DNS, query/header, timeout and atom syntax (§5, §6).
//!
//! `owner` parameters are the owning client/scope/endpoint label named by
//! every diagnostic (DIAG-01).

use std::time::Duration;

use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{Ident, LitBool, LitChar, LitInt, LitStr, Token, Type, braced, bracketed};

use super::ast::{
    Atom, DurationLiteral, Emission, KeyBlock, KeyStatement, PathOperation, PlacementFamily,
    Reference, StatementKind, SubdomainOperation, Template, TemplatePart, TemplateSegment,
};
use super::diagnostics::{
    Code, coded, validate_integer, validate_public_header_name, validate_query_key,
    validate_static_host_label,
};
use super::keyword;

const MAX_CONCAT_DEPTH: usize = 64;

// Core owns this limit; xtask's LIMIT-01 policy checks the copy.
const MAX_FORMATTED_PUBLIC_VALUE_BYTES: usize = 65_536;

pub(crate) fn parse_subdomain(
    input: ParseStream<'_>,
    owner: &str,
) -> syn::Result<SubdomainOperation> {
    input.parse::<keyword::subdomain>()?;
    if input.peek(keyword::off) {
        input.parse::<keyword::off>()?;
        input.parse::<Token![;]>()?;
        return Ok(SubdomainOperation::Off);
    }
    let replace = input.peek(keyword::replace);
    if replace {
        input.parse::<keyword::replace>()?;
    }
    if input.peek(LitStr) {
        let literal: LitStr = input.parse()?;
        return Err(coded(
            literal.span(),
            Code::Atom,
            format!(
                "DNS labels of {owner} are a typed atom list, not a host template: write `subdomain [\"api\"];` or `subdomain [region, \"api\"];`"
            ),
        ));
    }
    let content;
    bracketed!(content in input);
    let mut atoms = Vec::new();
    while !content.is_empty() {
        let atom = parse_atom(&content, 0, owner)?;
        if let Atom::Str(literal) = &atom {
            validate_static_host_label(literal, owner)?;
        }
        atoms.push(atom);
        if content.is_empty() {
            break;
        }
        content.parse::<Token![,]>()?;
    }
    if atoms.is_empty() {
        return Err(coded(
            content.span(),
            Code::Atom,
            format!(
                "a subdomain list of {owner} requires at least one label; write `subdomain off;` to remove generated labels"
            ),
        ));
    }
    input.parse::<Token![;]>()?;
    Ok(if replace {
        SubdomainOperation::Replace(atoms)
    } else {
        SubdomainOperation::Prepend(atoms)
    })
}

pub(crate) fn parse_path(input: ParseStream<'_>, owner: &str) -> syn::Result<PathOperation> {
    input.parse::<keyword::path>()?;
    if input.peek(keyword::off) {
        input.parse::<keyword::off>()?;
        input.parse::<Token![;]>()?;
        return Ok(PathOperation::Off);
    }
    let replace = input.peek(keyword::replace);
    if replace {
        input.parse::<keyword::replace>()?;
    }
    if input.peek(syn::token::Bracket) {
        return Err(coded(
            input.span(),
            Code::Migrate,
            format!(
                "v2 path atom lists were replaced by a relative template string: `path [\"items\", endpoint.id]` becomes `path \"items/{{id}}\";` (in {owner})"
            ),
        ));
    }
    let literal: LitStr = input.parse().map_err(|_| {
        input.error(format!(
            "[C3-PATH] `path` in {owner} requires one relative template string such as `path \"items/{{id}}\";`, `path replace \"...\";` or `path off;`"
        ))
    })?;
    let template = parse_template(&literal, owner)?;
    input.parse::<Token![;]>()?;
    Ok(if replace {
        PathOperation::Replace(template)
    } else {
        PathOperation::Append(template)
    })
}

fn template_error(
    literal: &LitStr,
    range: (usize, usize),
    owner: &str,
    message: impl std::fmt::Display,
) -> syn::Error {
    coded(
        literal.span(),
        Code::Path,
        format!(
            "{message} (path template of {owner}, template bytes {}..{})",
            range.0, range.1
        ),
    )
}

/// Scans a decoded template value: holes are balanced `{...}`; `{{`/`}}`
/// escape literal braces outside holes; `/` separates segments only outside
/// holes (PATH-01).
pub(crate) fn parse_template(literal: &LitStr, owner: &str) -> syn::Result<Template> {
    let text = literal.value();
    if text.is_empty() {
        return Err(template_error(
            literal,
            (0, 0),
            owner,
            "an empty path template appends nothing; omit the clause instead",
        ));
    }
    if text.starts_with('/') {
        return Err(template_error(
            literal,
            (0, 1),
            owner,
            "a generated path template is relative: remove the leading `/`",
        ));
    }
    if text.ends_with('/') {
        return Err(template_error(
            literal,
            (text.len() - 1, text.len()),
            owner,
            "trailing slashes are not supported in generated path templates; remove the trailing `/`",
        ));
    }
    let bytes = text.as_bytes();
    let mut segments = Vec::new();
    let mut parts: Vec<TemplatePart> = Vec::new();
    let mut literal_buffer = String::new();
    let mut segment_start = 0;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b'{' if bytes.get(index + 1) == Some(&b'{') => {
                literal_buffer.push('{');
                index += 2;
            }
            b'}' if bytes.get(index + 1) == Some(&b'}') => {
                literal_buffer.push('}');
                index += 2;
            }
            b'}' => {
                return Err(template_error(
                    literal,
                    (index, index + 1),
                    owner,
                    "unmatched `}`; write `}}` for a literal brace",
                ));
            }
            b'{' => {
                let start = index;
                let Some(offset) = text[index + 1..].find('}') else {
                    return Err(template_error(
                        literal,
                        (index, text.len()),
                        owner,
                        "unterminated `{` hole; write `{{` for a literal brace",
                    ));
                };
                let end = index + 1 + offset;
                let hole = &text[index + 1..end];
                if hole.contains('{') {
                    return Err(template_error(
                        literal,
                        (start, end + 1),
                        owner,
                        "a hole cannot contain braces; name a Rust type alias for a brace-delimited const-generic adapter",
                    ));
                }
                if !literal_buffer.is_empty() {
                    parts.push(TemplatePart::Literal(std::mem::take(&mut literal_buffer)));
                }
                let (reference, via) = parse_hole(literal, hole, (start, end + 1), owner)?;
                parts.push(TemplatePart::Hole {
                    reference,
                    via,
                    range: (start, end + 1),
                });
                index = end + 1;
            }
            b'/' => {
                if !literal_buffer.is_empty() {
                    parts.push(TemplatePart::Literal(std::mem::take(&mut literal_buffer)));
                }
                finish_segment(
                    literal,
                    &mut segments,
                    std::mem::take(&mut parts),
                    (segment_start, index),
                    owner,
                )?;
                segment_start = index + 1;
                index += 1;
            }
            b'?' | b'#' => {
                return Err(template_error(
                    literal,
                    (index, index + 1),
                    owner,
                    "a literal `?` or `#` in a path template looks like query/fragment syntax; use a scalar value/adapter when the character is intentional segment data",
                ));
            }
            b'\\' => {
                return Err(template_error(
                    literal,
                    (index, index + 1),
                    owner,
                    "a path segment cannot contain a backslash; use a scalar value/adapter when it is intentional segment data",
                ));
            }
            _ => {
                let character = text[index..].chars().next().expect("valid UTF-8 boundary");
                literal_buffer.push(character);
                index += character.len_utf8();
            }
        }
    }
    if !literal_buffer.is_empty() {
        parts.push(TemplatePart::Literal(literal_buffer));
    }
    finish_segment(
        literal,
        &mut segments,
        parts,
        (segment_start, bytes.len()),
        owner,
    )?;
    Ok(Template {
        literal: literal.clone(),
        segments,
    })
}

fn finish_segment(
    literal: &LitStr,
    segments: &mut Vec<TemplateSegment>,
    parts: Vec<TemplatePart>,
    range: (usize, usize),
    owner: &str,
) -> syn::Result<()> {
    if parts.is_empty() {
        return Err(template_error(
            literal,
            range,
            owner,
            "empty path segment (`//`) is not accepted; remove the doubled `/`",
        ));
    }
    if let [TemplatePart::Literal(text)] = parts.as_slice() {
        if text == "." || text == ".." {
            return Err(template_error(
                literal,
                range,
                owner,
                "a path segment cannot be `.` or `..`; write the resolved relative path",
            ));
        }
    }
    segments.push(TemplateSegment { parts, range });
    Ok(())
}

struct HoleSyntax {
    reference: Reference,
    via: Option<Type>,
}

impl Parse for HoleSyntax {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let reference = parse_reference(input, "a path template")?;
        let via = if input.peek(keyword::via) {
            input.parse::<keyword::via>()?;
            Some(input.parse::<Type>()?)
        } else {
            None
        };
        if !input.is_empty() {
            return Err(input.error("unexpected tokens"));
        }
        Ok(Self { reference, via })
    }
}

fn parse_hole(
    literal: &LitStr,
    text: &str,
    range: (usize, usize),
    owner: &str,
) -> syn::Result<(Reference, Option<Type>)> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(template_error(
            literal,
            range,
            owner,
            "an empty `{}` hole must name a declared value; write `{name}`",
        ));
    }
    let tokens: proc_macro2::TokenStream = trimmed.parse().map_err(|_| {
        template_error(
            literal,
            range,
            owner,
            "a hole is `{name}`, `{owner.name}` or `{name via Adapter}`",
        )
    })?;
    let tokens = respan(tokens, literal.span());
    let parsed: HoleSyntax = syn::parse2(tokens).map_err(|_| {
        template_error(
            literal,
            range,
            owner,
            "a hole names one declared total value: `{name}`, `{owner.name}` or `{name via Adapter}`; it cannot evaluate expressions or select optional structure",
        )
    })?;
    Ok((parsed.reference, parsed.via))
}

fn respan(tokens: proc_macro2::TokenStream, span: proc_macro2::Span) -> proc_macro2::TokenStream {
    tokens
        .into_iter()
        .map(|mut tree| {
            if let proc_macro2::TokenTree::Group(group) = &tree {
                let mut replacement =
                    proc_macro2::Group::new(group.delimiter(), respan(group.stream(), span));
                replacement.set_span(span);
                tree = proc_macro2::TokenTree::Group(replacement);
            } else {
                tree.set_span(span);
            }
            tree
        })
        .collect()
}

pub(crate) fn parse_key_block(
    input: ParseStream<'_>,
    family: PlacementFamily,
    owner: &str,
) -> syn::Result<KeyBlock> {
    let family_name = match family {
        PlacementFamily::Query => {
            input.parse::<keyword::query>()?;
            "query"
        }
        PlacementFamily::Header => {
            input.parse::<keyword::headers>()?;
            "headers"
        }
    };
    if input.peek(keyword::off) {
        input.parse::<keyword::off>()?;
        input.parse::<Token![;]>()?;
        return Ok(KeyBlock::Off);
    }
    let replace = input.peek(keyword::replace);
    if replace {
        input.parse::<keyword::replace>()?;
    }
    if !input.peek(syn::token::Brace) {
        return Err(coded(
            input.span(),
            Code::Place,
            format!(
                "`{family_name}` in {owner} requires `{{ ... }}`, `{family_name} replace {{ ... }}` or `{family_name} off;`"
            ),
        ));
    }
    let body;
    braced!(body in input);
    let mut statements = Vec::new();
    while !body.is_empty() {
        statements.push(parse_key_statement(&body, family, owner)?);
    }
    Ok(if replace {
        KeyBlock::Replace(statements)
    } else {
        KeyBlock::Modify(statements)
    })
}

fn parse_key_statement(
    input: ParseStream<'_>,
    family: PlacementFamily,
    owner: &str,
) -> syn::Result<KeyStatement> {
    let span = input.span();
    let kind = if input.peek(keyword::remove) {
        input.parse::<keyword::remove>()?;
        StatementKind::Remove
    } else if input.peek(keyword::replace) {
        input.parse::<keyword::replace>()?;
        StatementKind::Replace
    } else if input.peek(keyword::when_present) {
        input.parse::<keyword::when_present>()?;
        StatementKind::WhenPresent
    } else if input.peek(keyword::append) {
        input.parse::<keyword::append>()?;
        StatementKind::Append
    } else {
        StatementKind::Introduce
    };
    let key: LitStr = input.parse().map_err(|_| {
        input.error(format!(
            "[C3-PLACE] a key statement in {owner} starts with a static string key; keys are not interpolated (write `\"key\" = value;`)"
        ))
    })?;
    match family {
        PlacementFamily::Query => validate_query_key(&key, owner)?,
        PlacementFamily::Header => {
            validate_public_header_name(&key, owner)?;
        }
    }
    let emission = if kind == StatementKind::Remove {
        None
    } else {
        if input.peek(Token![+=]) {
            return Err(coded(
                input.span(),
                Code::Migrate,
                format!("`+=` was removed; write `append \"K\" = value;` in {owner}"),
            ));
        }
        input.parse::<Token![=]>()?;
        let emission = parse_emission(input, owner)?;
        if let Emission::Atom(atom) = &emission {
            validate_head_literals(atom, family, owner)?;
        }
        Some(emission)
    };
    input.parse::<Token![;]>()?;
    Ok(KeyStatement {
        kind,
        key,
        emission,
        span,
    })
}

/// Check only literal output: references (including initialized values) can
/// change at run time. Literal bytes in a concatenation cannot be removed by
/// another component, so their sum is a lower bound on the formatted size.
fn validate_head_literals(atom: &Atom, family: PlacementFamily, owner: &str) -> syn::Result<usize> {
    let bytes = match atom {
        Atom::Reference { .. } => 0,
        Atom::Concat { parts, .. } => {
            let mut bytes = 0_usize;
            for part in parts {
                bytes = bytes.saturating_add(validate_head_literals(part, family, owner)?);
            }
            bytes
        }
        _ => {
            // Match the literal spelling emitted by semantic::head::resolve_atom.
            let text = match atom {
                Atom::Str(literal) => literal.value(),
                Atom::Char(literal) => literal.value().to_string(),
                Atom::Bool(literal) => literal.value.to_string(),
                Atom::Int { negative, literal } => format!(
                    "{}{}",
                    if *negative { "-" } else { "" },
                    literal.base10_digits()
                ),
                Atom::Reference { .. } | Atom::Concat { .. } => unreachable!(),
            };
            if family == PlacementFamily::Header && http::HeaderValue::from_str(&text).is_err() {
                return Err(coded(
                    atom.span(),
                    Code::Atom,
                    format!(
                        "header literal in {owner} cannot form an HTTP header value; remove control characters other than horizontal tab"
                    ),
                ));
            }
            text.len()
        }
    };
    if bytes > MAX_FORMATTED_PUBLIC_VALUE_BYTES {
        let family = match family {
            PlacementFamily::Query => "query",
            PlacementFamily::Header => "header",
        };
        return Err(coded(
            atom.span(),
            Code::Atom,
            format!(
                "{family} literal output in {owner} exceeds the {MAX_FORMATTED_PUBLIC_VALUE_BYTES}-byte formatted public-value limit; shorten the literal components"
            ),
        ));
    }
    Ok(bytes)
}

fn parse_emission(input: ParseStream<'_>, owner: &str) -> syn::Result<Emission> {
    if input.peek(keyword::each) {
        let marker: keyword::each = input.parse()?;
        let reference = parse_reference(input, owner)?;
        let via = if input.peek(keyword::via) {
            input.parse::<keyword::via>()?;
            Some(input.parse::<Type>()?)
        } else {
            None
        };
        return Ok(Emission::Each {
            reference,
            via,
            span: marker.span(),
        });
    }
    Ok(Emission::Atom(parse_atom(input, 0, owner)?))
}

/// `reference := ident | "client" "." ident | scope_ident "." ident`.
pub(crate) fn parse_reference(input: ParseStream<'_>, owner: &str) -> syn::Result<Reference> {
    let first: Ident = input.parse().map_err(|_| {
        input.error(format!(
            "[C3-NAME] expected a value reference in {owner}, such as `id`, `client.tenant` or `regional.region`"
        ))
    })?;
    if input.peek(Token![.]) {
        input.parse::<Token![.]>()?;
        let name: Ident = input.parse()?;
        if input.peek(Token![.])
            || input.peek(syn::token::Paren)
            || input.peek(syn::token::Bracket)
            || input.peek(Token![!])
        {
            return Err(coded(
                name.span(),
                Code::Atom,
                format!(
                    "arbitrary Rust expressions are not supported after a value reference in {owner}; reference a declared value and adapt it with `via Encoder`"
                ),
            ));
        }
        let span = first
            .span()
            .join(name.span())
            .unwrap_or_else(|| first.span());
        match first.to_string().as_str() {
            "vars" => {
                return Err(coded(
                    first.span(),
                    Code::Migrate,
                    format!("`vars.NAME` was removed; use `client.NAME` in {owner}"),
                ));
            }
            "ep" => {
                return Err(coded(
                    first.span(),
                    Code::Migrate,
                    format!("`ep.NAME` was removed; use the bare local name in {owner}"),
                ));
            }
            "secret" | "credential" | "credentials" => {
                return Err(coded(
                    first.span(),
                    Code::Auth,
                    format!(
                        "secrets and credentials are private sources and cannot appear in public expressions of {owner}; apply a credential with `auth {{ ... }}`"
                    ),
                ));
            }
            _ => {}
        }
        return Ok(Reference {
            owner: Some(first),
            name,
            span,
        });
    }
    if input.peek(syn::token::Paren) || input.peek(syn::token::Bracket) || input.peek(Token![!]) {
        return Err(coded(
            first.span(),
            Code::Atom,
            format!(
                "arbitrary Rust expressions are not supported in {owner}; reference a declared value and adapt it with `via Encoder`"
            ),
        ));
    }
    Ok(Reference {
        owner: None,
        span: first.span(),
        name: first,
    })
}

pub(crate) fn parse_atom(input: ParseStream<'_>, depth: usize, owner: &str) -> syn::Result<Atom> {
    if input.peek(syn::token::Bracket) {
        return Err(coded(
            input.span(),
            Code::Atom,
            format!(
                "bracketed interpolation is not DSL syntax (in {owner}); concatenate atoms with `concat [...]`"
            ),
        ));
    }
    if input.peek(LitStr) {
        return input.parse().map(Atom::Str);
    }
    if input.peek(LitChar) {
        return input.parse().map(Atom::Char);
    }
    if input.peek(LitBool) {
        return input.parse().map(Atom::Bool);
    }
    if input.peek(Token![-]) {
        input.parse::<Token![-]>()?;
        let literal: LitInt = input.parse().map_err(|_| {
            input.error(format!(
                "[C3-ATOM] a negative atom in {owner} must be an integer literal; write `-1`"
            ))
        })?;
        validate_integer(&literal, "atom", owner)?;
        return Ok(Atom::Int {
            negative: true,
            literal,
        });
    }
    if input.peek(LitInt) {
        let literal: LitInt = input.parse()?;
        validate_integer(&literal, "atom", owner)?;
        return Ok(Atom::Int {
            negative: false,
            literal,
        });
    }
    if input.peek(keyword::fmt) {
        return Err(coded(
            input.span(),
            Code::Migrate,
            format!("`fmt[...]` was removed; write `concat [...]` in {owner}"),
        ));
    }
    if input.peek(keyword::concat) && input.peek2(syn::token::Bracket) {
        let marker: keyword::concat = input.parse()?;
        if depth >= MAX_CONCAT_DEPTH {
            return Err(coded(
                marker.span(),
                Code::Atom,
                format!(
                    "nested `concat [...]` in {owner} exceeds the maximum depth of 64; flatten the concatenation (nested parts normalize to one list)"
                ),
            ));
        }
        let content;
        bracketed!(content in input);
        let mut parts = Vec::new();
        while !content.is_empty() {
            parts.push(parse_atom(&content, depth + 1, owner)?);
            if content.is_empty() {
                break;
            }
            content.parse::<Token![,]>()?;
        }
        if parts.is_empty() {
            return Err(coded(
                marker.span(),
                Code::Atom,
                format!(
                    "empty `concat []` is invalid in {owner}; write at least one part, such as `concat [\"v\", version]`, or remove the statement"
                ),
            ));
        }
        return Ok(Atom::Concat {
            parts,
            span: marker.span(),
        });
    }
    if input.peek(keyword::each) {
        return Err(coded(
            input.span(),
            Code::Atom,
            format!(
                "`each` is valid only as the whole emission of a query/header statement, never inside a path, DNS label, concatenation or rate dimension (in {owner}); write `\"key\" = each values;`"
            ),
        ));
    }
    let reference = parse_reference(input, owner)?;
    let via = if input.peek(keyword::via) {
        input.parse::<keyword::via>()?;
        Some(Box::new(input.parse::<Type>()?))
    } else {
        None
    };
    Ok(Atom::Reference { reference, via })
}

pub(crate) fn parse_timeout(input: ParseStream<'_>, owner: &str) -> syn::Result<DurationLiteral> {
    input.parse::<keyword::timeout>()?;
    if input.peek(keyword::off) {
        return Err(coded(
            input.span(),
            Code::Owner,
            format!(
                "`timeout off` is not supported in {owner}; declare one positive finite duration such as `timeout 30s;`"
            ),
        ));
    }
    let literal: LitInt = input.parse().map_err(|_| {
        input.error(format!(
            "[C3-OWNER] timeout in {owner} requires a duration such as `250ms`, `5s`, `10m` or `1h`"
        ))
    })?;
    let value = parse_duration(&literal, owner)?;
    input.parse::<Token![;]>()?;
    Ok(DurationLiteral {
        value,
        span: literal.span(),
    })
}

/// Positive integer duration with `ms`, `s`, `m` or `h` (§3.1).
pub(crate) fn parse_duration(literal: &LitInt, owner: &str) -> syn::Result<Duration> {
    let text = literal.to_string().replace('_', "");
    let digits = text.strip_suffix(literal.suffix()).unwrap_or(&text);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(coded(
            literal.span(),
            Code::Owner,
            format!("duration values in {owner} are base-10 integers; write e.g. `30s`"),
        ));
    }
    let value = literal.base10_parse::<u64>().map_err(|_| {
        coded(
            literal.span(),
            Code::Owner,
            format!("duration value in {owner} is not representable; write a smaller duration"),
        )
    })?;
    if value == 0 {
        return Err(coded(
            literal.span(),
            Code::Owner,
            format!(
                "durations must be greater than zero (in {owner}); write a positive duration such as `1s`"
            ),
        ));
    }
    let seconds = |factor: u64| {
        value
            .checked_mul(factor)
            .map(Duration::from_secs)
            .ok_or_else(|| {
                coded(
                    literal.span(),
                    Code::Owner,
                    format!(
                        "duration in {owner} overflows `std::time::Duration`; write a smaller duration"
                    ),
                )
            })
    };
    match literal.suffix() {
        "ms" => Ok(Duration::from_millis(value)),
        "s" => seconds(1),
        "m" => seconds(60),
        "h" => seconds(3_600),
        _ => Err(coded(
            literal.span(),
            Code::Owner,
            format!(
                "duration unit must be one of `ms`, `s`, `m`, or `h` (in {owner}); write e.g. `250ms` or `5s`"
            ),
        )),
    }
}
