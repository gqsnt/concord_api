use proc_macro2::Span;
use syn::LitStr;
use url::Url;

use super::SpanError;
use super::model::{NormalizedConfig, ResolvedExpr};
use crate::{
    MAX_REQUEST_HEAD_COMPONENTS, MAX_REQUEST_HEADER_BYTES, MAX_REQUEST_HEADER_COUNT,
    MAX_REQUEST_URL_BYTES,
};

pub(super) fn validate_static_head(
    base: &LitStr,
    config: &NormalizedConfig,
    owner: Span,
) -> syn::Result<()> {
    if config.headers.len() > MAX_REQUEST_HEADER_COUNT {
        return Err(owner.error(format!(
            "one effective request head permits at most {MAX_REQUEST_HEADER_COUNT} public headers"
        )));
    }

    let components = [
        config.host.len(),
        config.path.len(),
        config.query.len(),
        config.headers.len(),
        config.auth.len(),
    ]
    .into_iter()
    .try_fold(0_usize, usize::checked_add)
    .ok_or_else(|| owner.error("effective request-head component accounting overflowed"))?;
    if components > MAX_REQUEST_HEAD_COMPONENTS {
        return Err(owner.error(format!(
            "one effective request head permits at most {MAX_REQUEST_HEAD_COMPONENTS} retained components"
        )));
    }

    let mut header_bytes = 0_usize;
    for header in &config.headers {
        header_bytes = header_bytes
            .checked_add(header.canonical_name.len())
            .and_then(|bytes| {
                static_expr_value(&header.value)
                    .map_or(Some(bytes), |value| bytes.checked_add(value.len()))
            })
            .ok_or_else(|| owner.error("effective request-header byte accounting overflowed"))?;
        if header_bytes > MAX_REQUEST_HEADER_BYTES {
            return Err(owner.error(format!(
                "statically known public request headers exceed {MAX_REQUEST_HEADER_BYTES} aggregate bytes"
            )));
        }
    }

    let mut url = Url::parse(&base.value())
        .map_err(|_| owner.error("the fixed base could not be modeled as a request URL"))?;
    if url.as_str().len() > MAX_REQUEST_URL_BYTES {
        return Err(owner.error(format!(
            "the statically known request URL exceeds {MAX_REQUEST_URL_BYTES} bytes"
        )));
    }

    apply_static_host_lower_bound(&mut url, config);
    let mut appended_segment = false;
    for segment in &config.path {
        let mut value = static_expr_value(segment).unwrap_or_else(|| "x".to_owned());
        if value.is_empty() {
            value.push('x');
        }
        let mut path = url
            .path_segments_mut()
            .map_err(|()| owner.error("the fixed base cannot receive path segments"))?;
        if !appended_segment {
            path.pop_if_empty();
        }
        path.push(&value);
        drop(path);
        appended_segment = true;
        validate_url_len(&url, owner)?;
    }
    for query in &config.query {
        let value = static_expr_value(&query.value).unwrap_or_default();
        url.query_pairs_mut()
            .append_pair(&query.key.value(), &value);
        validate_url_len(&url, owner)?;
    }
    validate_url_len(&url, owner)
}

fn apply_static_host_lower_bound(url: &mut Url, config: &NormalizedConfig) {
    if config.host.is_empty() {
        return;
    }
    let Some(base_host) = url.host_str() else {
        return;
    };
    let mut host = String::new();
    for label in &config.host {
        let mut value = static_expr_value(label).unwrap_or_else(|| "x".to_owned());
        if value.is_empty() {
            value.push('x');
        }
        host.push_str(&value);
        host.push('.');
    }
    host.push_str(base_host);
    let _ = url.set_host(Some(&host));
}

fn validate_url_len(url: &Url, owner: Span) -> syn::Result<()> {
    if url.as_str().len() > MAX_REQUEST_URL_BYTES {
        Err(owner.error(format!(
            "the statically known request URL exceeds {MAX_REQUEST_URL_BYTES} bytes"
        )))
    } else {
        Ok(())
    }
}

fn static_expr_value(value: &ResolvedExpr) -> Option<String> {
    match value {
        ResolvedExpr::String(value) => Some(value.value()),
        ResolvedExpr::Integer(value) => Some(value.base10_digits().to_owned()),
        ResolvedExpr::Value(_) => None,
        ResolvedExpr::Format(values) => {
            let mut output = String::new();
            for value in values {
                output.push_str(&static_expr_value(value)?);
            }
            Some(output)
        }
    }
}

#[cfg(test)]
mod tests {
    use proc_macro2::TokenStream;
    use quote::quote;
    use syn::LitStr;

    use super::*;

    fn declaration(path: TokenStream, headers: TokenStream) -> TokenStream {
        let path = (!path.is_empty()).then(|| quote!(path [#path];));
        let headers = (!headers.is_empty()).then(|| quote!(headers { #headers }));
        quote! {
            client HeadLimitApi {
                base "https://example.test/";
            }
            GET fetch() -> Bytes {
                #path
                #headers
            }
        }
    }

    #[test]
    fn static_exact_component_boundary_resolves_and_first_excess_fails() {
        let exact = std::iter::repeat_with(|| quote!("x"))
            .take(MAX_REQUEST_HEAD_COMPONENTS)
            .collect::<Vec<_>>();
        let exact = quote!(#(#exact),*);
        crate::syntax::parse(declaration(exact, quote!()))
            .and_then(crate::semantic::resolve)
            .expect("exact static component boundary");

        let excess = std::iter::repeat_with(|| quote!("x"))
            .take(MAX_REQUEST_HEAD_COMPONENTS + 1)
            .collect::<Vec<_>>();
        let excess = quote!(#(#excess),*);
        let error = crate::syntax::parse(declaration(excess, quote!()))
            .and_then(crate::semantic::resolve)
            .err()
            .expect("first excess static component");
        assert!(
            error
                .to_string()
                .contains("at most 256 retained components")
        );
    }

    #[test]
    fn static_header_count_and_known_byte_excess_are_rejected() {
        let headers = (0..=MAX_REQUEST_HEADER_COUNT)
            .map(|index| {
                let name = LitStr::new(&format!("x-head-{index}"), Span::call_site());
                quote!(#name = "x";)
            })
            .collect::<Vec<_>>();
        let headers = quote!(#(#headers)*);
        let error = crate::syntax::parse(declaration(quote!(), headers))
            .and_then(crate::semantic::resolve)
            .err()
            .expect("first excess static header");
        assert!(
            error.to_string().contains("at most 256 public headers"),
            "{error}"
        );

        let first = LitStr::new(&"a".repeat(MAX_REQUEST_HEADER_BYTES / 2), Span::call_site());
        let second = LitStr::new(&"b".repeat(MAX_REQUEST_HEADER_BYTES / 2), Span::call_site());
        let error = crate::syntax::parse(declaration(
            quote!(),
            quote!("x-first" = #first; "x-second" = #second;),
        ))
        .and_then(crate::semantic::resolve)
        .err()
        .expect("statically known aggregate header excess");
        assert!(error.to_string().contains("aggregate bytes"));
    }

    #[test]
    fn obvious_static_url_excess_is_rejected() {
        let first = LitStr::new(&"a".repeat(65_536), Span::call_site());
        let second = LitStr::new(&"b".repeat(65_536), Span::call_site());
        let error = crate::syntax::parse(declaration(quote!(#first, #second), quote!()))
            .and_then(crate::semantic::resolve)
            .err()
            .expect("statically known URL excess");
        assert!(
            error
                .to_string()
                .contains("request URL exceeds 131072 bytes")
        );
    }
}
