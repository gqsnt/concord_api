use std::collections::{HashMap, HashSet};

use syn::parse::ParseStream;
use syn::spanned::Spanned;
use syn::{
    Attribute, Expr, ExprLit, Ident, Lit, LitBool, LitChar, LitFloat, LitInt, LitStr, Meta, Token,
    Type, braced,
};

use super::ast::{
    BodyDecl, DeclarationAttributes, DefaultExpr, Observation, ValueDecl, ValuePresence,
};
use super::diagnostics::{SpanError, insert_unique_value, validate_integer};
use super::keyword;
use crate::semantic::surface::canonical_ident;

pub(super) fn parse_declaration_attributes(
    input: ParseStream<'_>,
    owner: &str,
) -> syn::Result<DeclarationAttributes> {
    let attributes = input.call(Attribute::parse_outer)?;
    let mut result = DeclarationAttributes::default();
    for attribute in attributes {
        if let Some(doc) = doc_attribute(&attribute)? {
            result.docs.push(doc);
            continue;
        }
        if attribute.path().is_ident("deprecated") {
            if result.deprecated.is_some() {
                return Err(attribute
                    .span()
                    .error(format!("duplicate `#[deprecated]` attribute on {owner}")));
            }
            let mut note = None;
            attribute.parse_nested_meta(|meta| {
                if !meta.path.is_ident("note") {
                    return Err(meta.error("`#[deprecated]` accepts only `note = \"...\"`"));
                }
                if note.is_some() {
                    return Err(meta.error("duplicate deprecation note"));
                }
                note = Some(meta.value()?.parse::<LitStr>()?);
                Ok(())
            })?;
            result.deprecated = Some(note.ok_or_else(|| {
                attribute
                    .span()
                    .error("`#[deprecated]` requires `note = \"...\"`")
            })?);
            continue;
        }
        return Err(attribute.span().error(format!(
            "unsupported attribute on {owner}; supported attributes are Rust doc attributes and `#[deprecated(note = \"...\")]`"
        )));
    }
    Ok(result)
}

pub(super) fn parse_values(input: ParseStream<'_>) -> syn::Result<Vec<ValueDecl>> {
    input.parse::<keyword::values>()?;
    let body;
    braced!(body in input);
    let mut values = Vec::new();
    let mut names = HashSet::new();
    while !body.is_empty() {
        let value = parse_value(&body, values.len(), "client value")?;
        body.parse::<Token![;]>()?;
        insert_unique_value(&mut names, &value, "client value")?;
        values.push(value);
    }
    Ok(values)
}

pub(super) fn parse_parameters(
    input: ParseStream<'_>,
    owner: &str,
    body_is_special: bool,
) -> syn::Result<(Vec<ValueDecl>, Option<BodyDecl>)> {
    let mut parameters = Vec::new();
    let mut body = None;
    let mut names = HashMap::new();
    let mut position = 0;

    while !input.is_empty() {
        let attributes = input.call(Attribute::parse_outer)?;
        if attributes
            .iter()
            .any(|attribute| attribute.path().is_ident("body"))
        {
            return Err(attributes
                .iter()
                .find(|attribute| attribute.path().is_ident("body"))
                .expect("found body attribute")
                .span()
                .error("v1 `#[body] name: Spec` syntax was removed; write `body name: Input via Representation`"));
        }

        if input.peek(keyword::body) {
            if !body_is_special {
                return Err(input.error("request bodies are endpoint-only"));
            }
            input.parse::<keyword::body>()?;
            let name: Ident = input.parse()?;
            let canonical = insert_parameter_name(&mut names, &name, owner)?;
            let presence = if input.peek(Token![?]) {
                input.parse::<Token![?]>()?;
                ValuePresence::Optional
            } else {
                ValuePresence::Required
            };
            input.parse::<Token![:]>()?;
            let input_type: Type = input.parse()?;
            let spec_type = if input.peek(keyword::via) {
                input.parse::<keyword::via>()?;
                Some(input.parse()?)
            } else if input.peek(keyword::using) {
                return Err(input.error(
                    "v2.1 replaced body `using Representation` with inline `via Representation`",
                ));
            } else {
                None
            };
            let (docs, has_default, redacted) = parse_value_attributes(attributes, "request body")?;
            if has_default || redacted {
                return Err(name.span().error(
                    "request bodies accept documentation only; they cannot be defaulted or redacted public values",
                ));
            }
            if input.peek(Token![=]) {
                return Err(input.error("a request body parameter cannot have a default"));
            }
            if let Some(first) = &body {
                let first: &BodyDecl = first;
                let mut error = name.span().error(format!(
                    "endpoint declares two request body parameters: `{}` and `{canonical}`",
                    canonical_ident(&first.name)
                ));
                error.combine(first.name.span().error("the first request body is here"));
                return Err(error);
            }
            body = Some(BodyDecl {
                name,
                input_type,
                spec_type,
                presence,
                docs,
                position,
            });
        } else {
            let value = parse_ordinary_value(input, attributes, position, owner)?;
            insert_parameter_name(&mut names, &value.name, owner)?;
            parameters.push(value);
        }

        position += 1;
        if input.is_empty() {
            break;
        }
        input.parse::<Token![,]>()?;
    }
    Ok((parameters, body))
}

fn insert_parameter_name(
    names: &mut HashMap<String, proc_macro2::Span>,
    name: &Ident,
    owner: &str,
) -> syn::Result<String> {
    let canonical = canonical_ident(name);
    if let Some(first_span) = names.insert(canonical.clone(), name.span()) {
        let mut error = name
            .span()
            .error(format!("duplicate {owner} parameter `{canonical}`"));
        error.combine(first_span.error("the first parameter is here"));
        return Err(error);
    }
    Ok(canonical)
}

fn parse_value(input: ParseStream<'_>, position: usize, owner: &str) -> syn::Result<ValueDecl> {
    let attributes = input.call(Attribute::parse_outer)?;
    parse_ordinary_value(input, attributes, position, owner)
}

fn parse_ordinary_value(
    input: ParseStream<'_>,
    attributes: Vec<Attribute>,
    position: usize,
    owner: &str,
) -> syn::Result<ValueDecl> {
    let name: Ident = input.parse()?;
    let presence = if input.peek(Token![?]) {
        input.parse::<Token![?]>()?;
        ValuePresence::Optional
    } else {
        ValuePresence::Required
    };
    input.parse::<Token![:]>()?;
    let payload_type: Type = input.parse()?;
    let (docs, trait_default, redacted) = parse_value_attributes(attributes, owner)?;
    let explicit = if input.peek(Token![=]) {
        input.parse::<Token![=]>()?;
        if input.peek(Token![default]) {
            let retired: Token![default] = input.parse()?;
            return Err(retired
                .span
                .error("v1 `= default` was removed; place `#[default]` on the value declaration"));
        }
        Some(parse_stable_default(input)?)
    } else {
        None
    };
    if trait_default && explicit.is_some() {
        return Err(name
            .span()
            .error("`#[default]` and an explicit `= VALUE` initializer are mutually exclusive"));
    }
    let initializer = if trait_default {
        Some(DefaultExpr::TraitDefault(name.span()))
    } else {
        explicit
    };
    Ok(ValueDecl {
        name,
        payload_type,
        presence,
        initializer,
        observation: if redacted {
            Observation::Redacted
        } else {
            Observation::Ordinary
        },
        docs,
        position,
    })
}

fn parse_value_attributes(
    attributes: Vec<Attribute>,
    owner: &str,
) -> syn::Result<(Vec<LitStr>, bool, bool)> {
    let mut docs = Vec::new();
    let mut default = false;
    let mut redact = false;
    for attribute in attributes {
        if let Some(doc) = doc_attribute(&attribute)? {
            docs.push(doc);
            continue;
        }
        let (flag, label) = if attribute.path().is_ident("default") {
            (&mut default, "default")
        } else if attribute.path().is_ident("sensitive") {
            (&mut redact, "sensitive")
        } else if attribute.path().is_ident("redact") {
            return Err(attribute
                .span()
                .error("v2.1 renamed `#[redact]` to `#[sensitive]`"));
        } else if attribute.path().is_ident("deprecated") {
            return Err(attribute.span().error(
                "value-level deprecation is not supported; deprecate the owning client, scope, or endpoint",
            ));
        } else {
            return Err(attribute.span().error(format!(
                "unsupported attribute on {owner}; ordinary values support docs, `#[default]`, and `#[sensitive]`"
            )));
        };
        if !matches!(attribute.meta, Meta::Path(_)) {
            return Err(attribute
                .span()
                .error(format!("`#[{label}]` must be a bare attribute")));
        }
        if *flag {
            return Err(attribute
                .span()
                .error(format!("duplicate `#[{label}]` attribute")));
        }
        *flag = true;
    }
    Ok((docs, default, redact))
}

fn doc_attribute(attribute: &Attribute) -> syn::Result<Option<LitStr>> {
    if !attribute.path().is_ident("doc") {
        return Ok(None);
    }
    let Meta::NameValue(meta) = &attribute.meta else {
        return Err(attribute.span().error("malformed Rust doc attribute"));
    };
    let Expr::Lit(ExprLit {
        lit: Lit::Str(value),
        ..
    }) = &meta.value
    else {
        return Err(attribute
            .span()
            .error("Rust doc attributes require string text"));
    };
    Ok(Some(value.clone()))
}

pub(super) fn parse_stable_default(input: ParseStream<'_>) -> syn::Result<DefaultExpr> {
    if input.peek(LitStr) {
        return input.parse().map(DefaultExpr::String);
    }
    if input.peek(LitChar) {
        return input.parse().map(DefaultExpr::Character);
    }
    if input.peek(Token![-]) {
        input.parse::<Token![-]>()?;
        if input.peek(LitInt) {
            let literal: LitInt = input.parse()?;
            validate_integer(&literal, "default")?;
            return Ok(DefaultExpr::Integer {
                negative: true,
                literal,
            });
        }
        if input.peek(LitFloat) {
            return input.parse().map(|literal| DefaultExpr::Float {
                negative: true,
                literal,
            });
        }
        return Err(input.error("a negative default requires a numeric literal"));
    }
    if input.peek(LitInt) {
        let literal: LitInt = input.parse()?;
        validate_integer(&literal, "default")?;
        return Ok(DefaultExpr::Integer {
            negative: false,
            literal,
        });
    }
    if input.peek(LitFloat) {
        return input.parse().map(|literal| DefaultExpr::Float {
            negative: false,
            literal,
        });
    }
    if input.peek(LitBool) {
        return input.parse::<LitBool>().map(DefaultExpr::Boolean);
    }
    if input.fork().parse::<syn::Path>().is_ok() {
        let path = input.parse()?;
        if input.peek(syn::token::Paren)
            || input.peek(syn::token::Brace)
            || input.peek(syn::token::Bracket)
            || input.peek(Token![.])
            || input.peek(Token![!])
        {
            return Err(input.error(
                "defaults accept a Rust const/static path, not a call, method, macro, block, or collection expression",
            ));
        }
        return Ok(DefaultExpr::Path(path));
    }
    Err(input.error(
        "defaults accept only string, char, boolean, signed numeric literals, or a Rust const/static path",
    ))
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use crate::syntax::ast::{DefaultExpr, Observation, SourceChild, ValuePresence};

    #[test]
    fn parses_explicit_presence_defaults_redaction_and_body_specs() {
        let source = super::super::parse(quote! {
            client ValuesApi {
                base "https://example.test";
                timeout 30s;
                values {
                    required: Option<u32>;
                    optional?: Option<u32>;
                    #[default]
                    defaulted: bool;
                    #[sensitive]
                    private_public: String;
                }
                POST upload(
                    first: String,
                    body payload: Input via Body,
                    later?: bool,
                ) -> Output via Response;
            }
        })
        .expect("v2 values syntax");
        let SourceChild::Endpoint(endpoint) = &source.client.children[0] else {
            panic!("endpoint");
        };

        let values = &source.client.values.as_ref().unwrap().value;
        assert_eq!(values[0].presence, ValuePresence::Required);
        assert_eq!(values[1].presence, ValuePresence::Optional);
        assert!(matches!(
            values[2].initializer,
            Some(DefaultExpr::TraitDefault(_))
        ));
        assert_eq!(values[3].observation, Observation::Redacted);
        assert_eq!(endpoint.body.as_ref().unwrap().name, "payload");
        assert_eq!(endpoint.body.as_ref().unwrap().position, 1);
    }
}
