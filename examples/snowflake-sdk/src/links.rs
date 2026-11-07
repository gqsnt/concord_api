//! Returned SQL API links: statement context and partition policy over shared Link syntax.
use crate::{Error, ProtocolError, Query};
use concord::{
    ResponseMeta,
    reqwest::{Url, header::LINK},
};
use concord_pagination::link;
use std::{collections::BTreeMap, fmt};

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
    let links = link::parse(
        meta.headers()
            .get_all(LINK)
            .iter()
            .map(|value| value.as_bytes()),
        base,
    )
    .map_err(|_| invalid(meta))?;
    // Empty targets have meaning in generic URI syntax, but not in a partition plan.
    if links.iter().any(|link| link.raw_target.is_empty()) {
        return Err(invalid(meta));
    }
    let mut targets = BTreeMap::new();
    // next wins over last when both describe the same partition with distinct request IDs.
    for (rel, expected) in [
        ("next", current.checked_add(1)),
        ("prev", current.checked_sub(1)),
        ("first", Some(0)),
        ("last", count.checked_sub(1)),
    ] {
        let mut selected = None;
        for link in links.iter().filter(|link| link.has_relation(rel)) {
            // This SDK only follows links in the current response context. Check every link,
            // even a duplicate with the same target, before collapsing identical URLs.
            if link
                .parameters
                .iter()
                .any(|p| p.name.eq_ignore_ascii_case("anchor"))
                || selected.is_some_and(|url| url != &link.target)
            {
                return Err(invalid(meta));
            }
            selected = Some(&link.target);
        }
        let Some(url) = selected.cloned() else {
            continue;
        };
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
