//! Resolved descriptor classification.
//!
//! This module consumes semantic route, I/O, auth, and pagination facts. The
//! parser does not own descriptor policy, and code generation only translates
//! this IR to the current core integration contract.

use super::*;

pub(super) fn resolve_endpoint_descriptor(
    scheme: Scheme,
    domain: &LitStr,
    prefix_pieces: &[PrefixPiece],
    request: &ResolvedRequestBodyIo,
    response: &ResolvedResponseBodyIo,
    pagination: Option<&PaginateResolved>,
) -> EndpointDescriptorIr {
    EndpointDescriptorIr {
        origin: classify_endpoint_origin(scheme, domain, prefix_pieces),
        request_body: request_body_descriptor(request),
        response_format: response_format_descriptor(response),
        pagination_can_change_origin: pagination
            .is_some_and(|pagination| pagination_can_change_origin(prefix_pieces, pagination)),
    }
}

fn classify_endpoint_origin(
    scheme: Scheme,
    domain: &LitStr,
    prefix_pieces: &[PrefixPiece],
) -> EndpointOriginIr {
    let mut prefix_labels = Vec::with_capacity(prefix_pieces.len());
    for piece in prefix_pieces {
        match piece {
            PrefixPiece::Static(label) if valid_dns_label(label) => {
                prefix_labels.push(label.as_str());
            }
            PrefixPiece::Static(_) => return EndpointOriginIr::Dynamic,
            PrefixPiece::CxVar { .. } | PrefixPiece::EpVar { .. } | PrefixPiece::Fmt(_) => {
                return EndpointOriginIr::Dynamic;
            }
        }
    }
    let Some(authority) = validated_origin_authority(scheme, &domain.value(), &prefix_labels)
    else {
        return EndpointOriginIr::Dynamic;
    };
    EndpointOriginIr::Fixed(FixedOriginIr {
        scheme: match scheme {
            Scheme::Http => OriginSchemeIr::Http,
            Scheme::Https => OriginSchemeIr::Https,
        },
        authority,
    })
}

fn validated_origin_authority(
    scheme: Scheme,
    base_authority: &str,
    prefix_labels: &[&str],
) -> Option<String> {
    if base_authority.is_empty()
        || base_authority
            .chars()
            .any(|ch| ch.is_whitespace() || ch.is_control())
        || base_authority
            .chars()
            .any(|ch| matches!(ch, '@' | '/' | '\\' | '?' | '#'))
    {
        return None;
    }

    let scheme_text = match scheme {
        Scheme::Http => "http",
        Scheme::Https => "https",
    };
    let base_url = url::Url::parse(&format!("{scheme_text}://{base_authority}")).ok()?;
    if !valid_origin_url(&base_url, scheme_text) {
        return None;
    }

    let (raw_host, port) = split_authority(base_authority)?;
    let host = base_url.host()?;
    let host_is_domain = matches!(&host, url::Host::Domain(_));
    let host_text = match host {
        url::Host::Domain(domain) => {
            if !raw_host.is_ascii()
                || !raw_host.eq_ignore_ascii_case(domain)
                || !valid_dns_name(domain)
            {
                return None;
            }
            domain.to_string()
        }
        url::Host::Ipv4(address) => {
            if !prefix_labels.is_empty()
                || raw_host.parse::<std::net::Ipv4Addr>().ok() != Some(address)
            {
                return None;
            }
            address.to_string()
        }
        url::Host::Ipv6(address) => {
            if !prefix_labels.is_empty()
                || raw_host
                    .strip_prefix('[')
                    .and_then(|host| host.strip_suffix(']'))
                    .and_then(|host| host.parse::<std::net::Ipv6Addr>().ok())
                    != Some(address)
            {
                return None;
            }
            format!("[{address}]")
        }
    };
    let combined_host = if prefix_labels.is_empty() {
        host_text
    } else {
        format!("{}.{host_text}", prefix_labels.join("."))
    };
    if host_is_domain && !valid_dns_name(&combined_host) {
        return None;
    }
    let mut authority = combined_host;
    if let Some(port) = port {
        authority.push(':');
        authority.push_str(port);
    }

    let candidate = url::Url::parse(&format!("{scheme_text}://{authority}")).ok()?;
    valid_origin_url(&candidate, scheme_text).then_some(authority)
}

fn valid_origin_url(url: &url::Url, expected_scheme: &str) -> bool {
    url.scheme() == expected_scheme
        && url.username().is_empty()
        && url.password().is_none()
        && url.host().is_some()
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none()
}

fn split_authority(authority: &str) -> Option<(&str, Option<&str>)> {
    let (host, port) = if authority.starts_with('[') {
        let close = authority.find(']')?;
        let host = authority.get(..=close)?;
        let remainder = authority.get(close + 1..)?;
        if remainder.is_empty() {
            return Some((host, None));
        }
        (host, Some(remainder.strip_prefix(':')?))
    } else {
        match authority.rsplit_once(':') {
            Some((host, port)) if !host.contains(':') => (host, Some(port)),
            Some(_) => return None,
            None => (authority, None),
        }
    };
    if host.is_empty() {
        return None;
    }
    if let Some(port) = port {
        if port.is_empty() || !port.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        let _port = port.parse::<u16>().ok()?;
    }
    Some((host, port))
}

fn valid_dns_name(domain: &str) -> bool {
    !domain.is_empty() && domain.len() <= 253 && domain.split('.').all(valid_dns_label)
}

fn valid_dns_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 63
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

pub(super) fn classify_api_origin(endpoints: &[ResolvedEndpoint]) -> ApiOriginIr {
    let mut origins = std::collections::BTreeSet::new();
    for endpoint in endpoints {
        // A pagination binding that can alter a host component makes the API
        // ineligible for client-wide fixed-origin status retry even when the
        // initial endpoint origin was otherwise classified as fixed.
        if endpoint.descriptor.pagination_can_change_origin {
            return ApiOriginIr::Dynamic;
        }
        match &endpoint.descriptor.origin {
            EndpointOriginIr::Fixed(origin) => {
                origins.insert(origin.clone());
            }
            EndpointOriginIr::Dynamic => return ApiOriginIr::Dynamic,
        }
    }
    match origins.len() {
        0 => ApiOriginIr::Dynamic,
        1 => ApiOriginIr::FixedSingle(
            origins
                .into_iter()
                .next()
                .expect("one resolved fixed origin"),
        ),
        _ => ApiOriginIr::Multi,
    }
}

fn pagination_can_change_origin(
    prefix_pieces: &[PrefixPiece],
    pagination: &PaginateResolved,
) -> bool {
    let host_fields = prefix_pieces.iter().flat_map(|piece| match piece {
        PrefixPiece::EpVar { field } => vec![field.to_string()],
        PrefixPiece::Fmt(fmt) => fmt
            .pieces
            .iter()
            .filter_map(|piece| match piece {
                FmtResolvedPiece::Var {
                    source: FmtVarSource::Ep,
                    field,
                    ..
                } => Some(field.to_string()),
                FmtResolvedPiece::Lit(_) | FmtResolvedPiece::Var { .. } => None,
            })
            .collect(),
        PrefixPiece::Static(_) | PrefixPiece::CxVar { .. } => Vec::new(),
    });
    let host_fields = host_fields.collect::<std::collections::BTreeSet<_>>();
    pagination
        .bindings
        .iter()
        .any(|binding| host_fields.contains(&binding.endpoint_rust_field.to_string()))
}

fn request_body_descriptor(request: &ResolvedRequestBodyIo) -> RequestBodyDescriptorIr {
    match request {
        ResolvedRequestBodyIo::None => RequestBodyDescriptorIr::None,
        ResolvedRequestBodyIo::BufferedCodec(io) => {
            let codec = &io.codec_path;
            RequestBodyDescriptorIr::Buffered {
                codec: quote::quote!(#codec).to_string(),
            }
        }
        ResolvedRequestBodyIo::RawStream { media_ty } => RequestBodyDescriptorIr::Streaming {
            media: quote::quote!(#media_ty).to_string(),
        },
        ResolvedRequestBodyIo::Multipart { .. } => RequestBodyDescriptorIr::Multipart,
    }
}

fn response_format_descriptor(response: &ResolvedResponseBodyIo) -> ResponseFormatDescriptorIr {
    match response {
        ResolvedResponseBodyIo::BufferedCodec(io) => {
            let codec = &io.codec_path;
            ResponseFormatDescriptorIr::Buffered {
                codec: quote::quote!(#codec).to_string(),
            }
        }
        ResolvedResponseBodyIo::BufferedBytes => ResponseFormatDescriptorIr::Bytes,
        ResolvedResponseBodyIo::NoContent => ResponseFormatDescriptorIr::NoContent,
        ResolvedResponseBodyIo::RawStream { media_ty } => ResponseFormatDescriptorIr::Streaming {
            media: quote::quote!(#media_ty).to_string(),
        },
    }
}
