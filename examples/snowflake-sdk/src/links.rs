//! Returned SQL API links. The Link tokenizer is adapted from the existing GitHub SDK.
use crate::{Error, ProtocolError, Query};
use concord::{
    ResponseMeta,
    reqwest::{Url, header::LINK},
};
use std::{collections::BTreeMap, fmt};

#[derive(Default)]
struct Links {
    next: Option<Url>,
    prev: Option<Url>,
    first: Option<Url>,
    last: Option<Url>,
}
struct LinkError;
const RELATIONS: [&str; 4] = ["next", "prev", "first", "last"];

/// A plan's known targets, shared by clones. Debug never exposes opaque URL queries.
#[derive(Default)]
pub(crate) struct PartitionLinks(pub BTreeMap<usize, Url>);
impl fmt::Debug for PartitionLinks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.0.keys()).finish()
    }
}

pub(crate) fn resolve(
    query: &Query,
    base: &Url,
    raw: &str,
    meta: &ResponseMeta,
) -> Result<Url, Error> {
    if raw.is_empty() || raw.trim() != raw || raw.chars().any(char::is_control) {
        return Err(invalid(meta));
    }
    let url = base.join(raw).map_err(|_| invalid(meta))?;
    validate(query, base, &url, meta)?;
    Ok(url)
}
fn validate(query: &Query, base: &Url, url: &Url, meta: &ResponseMeta) -> Result<(), Error> {
    let endpoint = query
        .client
        .api
        .statement(query.handle.clone())
        .status()
        .prepare()?;
    if url.origin() != base.origin()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.path() != endpoint.request().url().path()
    {
        return Err(invalid(meta));
    }
    Ok(())
}

pub(crate) fn partition_links(
    query: &Query,
    base: &Url,
    meta: &ResponseMeta,
    count: usize,
    current: usize,
) -> Result<BTreeMap<usize, Url>, Error> {
    let mut text = String::new();
    for value in meta.headers().get_all(LINK) {
        if !text.is_empty() {
            text.push(',');
        }
        text.push_str(value.to_str().map_err(|_| invalid(meta))?);
    }
    let links = parse_str(&text, base).map_err(|_| invalid(meta))?;
    let mut targets = BTreeMap::new();
    // next wins over last when both describe the same partition with distinct request IDs.
    for (url, expected) in [
        (links.next, current.checked_add(1)),
        (links.prev, current.checked_sub(1)),
        (links.first, Some(0)),
        (links.last, count.checked_sub(1)),
    ] {
        let Some(url) = url else { continue };
        validate(query, base, &url, meta)?;
        let index = partition_index(&url, meta)?.ok_or_else(|| invalid(meta))?;
        if index >= count || Some(index) != expected {
            return Err(invalid(meta));
        }
        targets.entry(index).or_insert(url);
    }
    Ok(targets)
}

pub(crate) fn partition_index(url: &Url, meta: &ResponseMeta) -> Result<Option<usize>, Error> {
    let mut index = None;
    for (_, value) in url.query_pairs().filter(|(key, _)| key == "partition") {
        if index.is_some() {
            return Err(invalid(meta));
        }
        index = Some(value.parse().map_err(|_| invalid(meta))?);
    }
    Ok(index)
}
fn invalid(meta: &ResponseMeta) -> Error {
    Error::protocol(ProtocolError::InvalidLink, Some(meta))
}

fn parse_str(text: &str, response_url: &Url) -> Result<Links, LinkError> {
    let bytes = text.as_bytes();
    let mut links = Links::default();
    let mut i = 0;
    loop {
        // Separators and optional whitespace between link-values (empty list elements allowed).
        while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b',') {
            i += 1;
        }
        if i >= bytes.len() {
            return Ok(links);
        }
        if bytes[i] != b'<' {
            return Err(LinkError);
        }
        let end = text[i + 1..]
            .find('>')
            .map(|offset| i + 1 + offset)
            .ok_or(LinkError)?;
        let target = &text[i + 1..end];
        if target.is_empty() || target.trim() != target || target.chars().any(char::is_control) {
            return Err(LinkError);
        }
        i = end + 1;
        let mut rels: Vec<String> = Vec::new();
        let mut seen_rel = false;
        let mut anchored = false;
        // Parameters until the next top-level comma or the end.
        loop {
            while i < bytes.len() && matches!(bytes[i], b' ' | b'\t') {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] == b',' {
                break;
            }
            if bytes[i] != b';' {
                return Err(LinkError);
            }
            i += 1;
            while i < bytes.len() && matches!(bytes[i], b' ' | b'\t') {
                i += 1;
            }
            let name_start = i;
            while i < bytes.len() && is_tchar(bytes[i]) {
                i += 1;
            }
            if i == name_start {
                // `;` followed by nothing, or by a non-token: tolerate a trailing `;` only.
                if i >= bytes.len() || bytes[i] == b',' {
                    break;
                }
                return Err(LinkError);
            }
            let name = text[name_start..i].to_ascii_lowercase();
            while i < bytes.len() && matches!(bytes[i], b' ' | b'\t') {
                i += 1;
            }
            let mut value = None;
            if i < bytes.len() && bytes[i] == b'=' {
                i += 1;
                while i < bytes.len() && matches!(bytes[i], b' ' | b'\t') {
                    i += 1;
                }
                if i < bytes.len() && bytes[i] == b'"' {
                    i += 1;
                    let mut out = String::new();
                    loop {
                        match bytes.get(i) {
                            None => return Err(LinkError),
                            Some(b'"') => {
                                i += 1;
                                break;
                            }
                            Some(b'\\') => {
                                let escaped = bytes.get(i + 1).ok_or(LinkError)?;
                                out.push(char::from(*escaped));
                                i += 2;
                            }
                            Some(&c) => {
                                out.push(char::from(c));
                                i += 1;
                            }
                        }
                    }
                    value = Some(out);
                } else {
                    let value_start = i;
                    while i < bytes.len() && is_tchar(bytes[i]) {
                        i += 1;
                    }
                    if i == value_start {
                        return Err(LinkError);
                    }
                    value = Some(text[value_start..i].to_owned());
                }
            }
            // RFC 8288 §3.3: only the first `rel` parameter counts.
            if name == "anchor" {
                anchored = true;
            }
            if name == "rel" && !seen_rel {
                seen_rel = true;
                let value = value.ok_or(LinkError)?;
                rels = value
                    .split_ascii_whitespace()
                    .map(str::to_ascii_lowercase)
                    .collect();
            }
        }
        let wanted: Vec<&'static str> = RELATIONS
            .iter()
            .copied()
            .filter(|r| rels.iter().any(|rel| rel == r))
            .collect();
        if wanted.is_empty() {
            continue;
        }
        // This SDK only follows links in the current response context.
        if anchored {
            return Err(LinkError);
        }
        let url = response_url.join(target).map_err(|_| LinkError)?;
        for rel in wanted {
            let slot = match rel {
                "next" => &mut links.next,
                "prev" => &mut links.prev,
                "first" => &mut links.first,
                _ => &mut links.last,
            };
            match slot {
                Some(existing) if *existing != url => return Err(LinkError),
                Some(_) => {}
                None => *slot = Some(url.clone()),
            }
        }
    }
}

/// RFC 9110 `tchar`.
fn is_tchar(c: u8) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}
