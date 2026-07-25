use proc_macro2::Span;
use syn::LitStr;
use url::{Host, Url};

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

    validate_host_shaping_base(&url, config, owner)?;
    apply_static_host_lower_bound(&mut url, config, owner)?;
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

fn validate_host_shaping_base(
    url: &Url,
    config: &NormalizedConfig,
    owner: Span,
) -> syn::Result<()> {
    if !config.host.is_empty() && !matches!(url.host(), Some(Host::Domain(_))) {
        return Err(config.host_span.unwrap_or(owner).error(
            "host-prefix shaping requires a DNS-domain fixed base; IP-literal bases cannot receive host labels",
        ));
    }
    Ok(())
}

fn apply_static_host_lower_bound(
    url: &mut Url,
    config: &NormalizedConfig,
    owner: Span,
) -> syn::Result<()> {
    if config.host.is_empty() {
        return Ok(());
    }
    let Some(base_host) = url.host_str() else {
        return Err(owner.error("the fixed base could not be modeled with a host"));
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
    let diagnostic_span = config.host_span.unwrap_or(owner);
    if host.len() > 253 {
        return Err(diagnostic_span
            .error("the statically known host-prefix shape exceeds the maximum DNS host length"));
    }
    url.set_host(Some(&host)).map_err(|_| {
        diagnostic_span.error("the statically known host-prefix shape cannot form a valid DNS host")
    })
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

    fn resolve(tokens: TokenStream) -> syn::Result<super::super::model::Api> {
        crate::syntax::parse(tokens).and_then(crate::semantic::resolve)
    }

    fn assert_ip_host_error(tokens: TokenStream) {
        let error = resolve(tokens)
            .err()
            .expect("host-prefix shaping over an IP fixed base");
        assert_eq!(
            error.to_string(),
            "host-prefix shaping requires a DNS-domain fixed base; IP-literal bases cannot receive host labels"
        );
    }

    #[test]
    fn ipv4_base_without_host_prefix_is_valid() {
        resolve(quote! {
            client Ipv4Api {
                base "http://127.0.0.1:8080/";
            }
            GET read() -> Bytes;
        })
        .expect("IPv4 fixed base without host shaping");
    }

    #[test]
    fn ipv6_base_without_host_prefix_is_valid() {
        resolve(quote! {
            client Ipv6Api {
                base "http://[2001:db8::1]:8080/";
            }
            GET read() -> Bytes;
        })
        .expect("IPv6 fixed base without host shaping");
    }

    #[test]
    fn dns_base_with_static_host_prefix_is_valid() {
        resolve(quote! {
            client DnsApi {
                base "https://example.test/";
            }
            GET read() -> Bytes {
                host ["api"];
            }
        })
        .expect("DNS fixed base with static host shaping");
    }

    #[test]
    fn dns_base_with_dynamic_host_prefix_is_valid() {
        resolve(quote! {
            client DnsApi {
                base "https://example.test/";
            }
            GET read(region: String) -> Bytes {
                host [ep.region];
            }
        })
        .expect("DNS fixed base with dynamic host shaping");
    }

    #[test]
    fn ipv4_base_with_client_host_prefix_is_rejected() {
        assert_ip_host_error(quote! {
            client Ipv4Api {
                base "http://127.0.0.1:8080/";
                host ["api"];
            }
            GET read() -> Bytes;
        });
    }

    #[test]
    fn ipv6_base_with_client_host_prefix_is_rejected() {
        assert_ip_host_error(quote! {
            client Ipv6Api {
                base "http://[2001:db8::1]:8080/";
                host ["api"];
            }
            GET read() -> Bytes;
        });
    }

    #[test]
    fn ipv4_base_with_scope_host_prefix_is_rejected() {
        assert_ip_host_error(quote! {
            client Ipv4Api {
                base "http://127.0.0.1:8080/";
            }
            scope shaped() {
                host ["api"];
                GET read() -> Bytes;
            }
        });
    }

    #[test]
    fn ipv6_base_with_endpoint_host_prefix_is_rejected() {
        assert_ip_host_error(quote! {
            client Ipv6Api {
                base "http://[2001:db8::1]:8080/";
            }
            GET read() -> Bytes {
                host ["api"];
            }
        });
    }

    #[test]
    fn inherited_host_prefix_over_ip_base_is_rejected() {
        assert_ip_host_error(quote! {
            client Ipv4Api {
                base "http://127.0.0.1:8080/";
            }
            scope shaped() {
                host ["api"];
                scope inherited() {
                    GET read() -> Bytes;
                }
            }
        });
    }

    #[test]
    fn host_off_removing_inherited_prefix_over_ip_base_is_valid() {
        resolve(quote! {
            client Ipv6Api {
                base "http://[2001:db8::1]:8080/";
                host ["api"];
            }
            scope unshaped() {
                host off;
                GET read() -> Bytes;
            }
        })
        .expect("host off removes an inherited IP-base prefix before endpoint validation");
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
