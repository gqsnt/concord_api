//! `"literal {name} literal {owner.name}"` templates used by `path`, `subdomain` and quoted
//! query/header values. `{{` and `}}` are literal braces.

use syn::LitStr;

use crate::syntax::Ref;

pub enum RawPiece {
    Lit(String),
    Hole(Ref),
}

pub fn parse(lit: &LitStr) -> syn::Result<Vec<RawPiece>> {
    let text = lit.value();
    let span = lit.span();
    let err = |msg: String| syn::Error::new(span, msg);
    let mut pieces = Vec::new();
    let mut lit_buf = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                lit_buf.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                lit_buf.push('}');
            }
            '}' => {
                return Err(err(format!(
                    "unmatched `}}` in {text:?}; write `}}}}` for a literal brace"
                )));
            }
            '{' => {
                let mut inner = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(c) => inner.push(c),
                        None => return Err(err(format!("unclosed `{{` in {text:?}"))),
                    }
                }
                if !lit_buf.is_empty() {
                    pieces.push(RawPiece::Lit(std::mem::take(&mut lit_buf)));
                }
                pieces.push(RawPiece::Hole(parse_hole(&inner, span, &text)?));
            }
            c => lit_buf.push(c),
        }
    }
    if !lit_buf.is_empty() {
        pieces.push(RawPiece::Lit(lit_buf));
    }
    Ok(pieces)
}

fn parse_hole(inner: &str, span: proc_macro2::Span, text: &str) -> syn::Result<Ref> {
    let inner = inner.trim();
    let ident = |s: &str| -> syn::Result<syn::Ident> {
        let raw = s.strip_prefix("r#").unwrap_or(s);
        let valid = !raw.is_empty()
            && raw.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !raw.starts_with(|c: char| c.is_ascii_digit());
        if !valid {
            return Err(syn::Error::new(
                span,
                format!(
                    "`{{{inner}}}` in {text:?} is not a value reference; holes are `{{name}}` or `{{owner.name}}`"
                ),
            ));
        }
        Ok(if s.starts_with("r#") {
            syn::Ident::new_raw(raw, span)
        } else {
            syn::Ident::new(raw, span)
        })
    };
    match inner.split_once('.') {
        Some((owner, name)) => Ok(Ref {
            owner: Some(ident(owner.trim())?),
            name: ident(name.trim())?,
        }),
        None => Ok(Ref {
            owner: None,
            name: ident(inner)?,
        }),
    }
}

/// Splits literal pieces on `sep`, producing one piece list per component. A hole never
/// contains a separator.
pub fn split(pieces: Vec<RawPiece>, sep: char) -> Vec<Vec<RawPiece>> {
    let mut parts = vec![Vec::new()];
    for piece in pieces {
        match piece {
            RawPiece::Hole(r) => parts.last_mut().unwrap().push(RawPiece::Hole(r)),
            RawPiece::Lit(s) => {
                let mut first = true;
                for chunk in s.split(sep) {
                    if !first {
                        parts.push(Vec::new());
                    }
                    first = false;
                    if !chunk.is_empty() {
                        parts
                            .last_mut()
                            .unwrap()
                            .push(RawPiece::Lit(chunk.to_owned()));
                    }
                }
            }
        }
    }
    parts
}

/// Re-renders pieces as template text for messages.
pub fn render(pieces: &[RawPiece]) -> String {
    let mut out = String::new();
    for piece in pieces {
        match piece {
            RawPiece::Lit(s) => out.push_str(&s.replace('{', "{{").replace('}', "}}")),
            RawPiece::Hole(r) => {
                out.push('{');
                if let Some(owner) = &r.owner {
                    out.push_str(&owner.to_string());
                    out.push('.');
                }
                out.push_str(&r.name.to_string());
                out.push('}');
            }
        }
    }
    out
}
