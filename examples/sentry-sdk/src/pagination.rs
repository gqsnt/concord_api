use crate::{
    Error, Event, Issue, Response, SentryOrganizationIssueEvents, SentryOrganizationIssues,
};
use concord::reqwest::{Url, header::HeaderMap};
use concord_pagination::{
    Page, Pages, Resume,
    link::{self, Relation},
};
use std::num::NonZeroUsize;

/// Bounded sequential cursor traversal. `Ok(None)` proves provider exhaustion.
/// A page cap returns `PagesError::Bounded`, preserving a resume cursor.
pub type Listing<T> = Pages<T, String, concord::ResponseMeta, Error>;

macro_rules! listing {
    ($operation:ty, $item:ty) => {
        impl $operation {
            /// Walk from an explicit cursor position with a mandatory logical page cap.
            /// Existing query filters and page size remain fixed on every generated send.
            pub fn pages(self, from: Resume<String>, max_pages: NonZeroUsize) -> Listing<$item> {
                Pages::new(from, move |cursor| {
                    let operation = self.clone().maybe_cursor(cursor);
                    async move {
                        let Response {
                            value,
                            next_cursor,
                            meta,
                            ..
                        } = operation.send().await?;
                        Ok(Page {
                            items: value,
                            next: next_cursor,
                            meta,
                        })
                    }
                })
                .max_pages(max_pages)
            }
        }
    };
}
listing!(SentryOrganizationIssues, Issue);
listing!(SentryOrganizationIssueEvents, Event);

pub(crate) fn next_cursor(
    headers: &HeaderMap,
    current: &Url,
) -> Result<Option<String>, &'static str> {
    let links = link::parse(
        headers.get_all("link").iter().map(|v| v.as_bytes()),
        current,
    )
    .map_err(|_| "malformed Link header")?;
    let next = match link::relation(&links, "next") {
        Relation::Absent => return Ok(None),
        Relation::Several => return Err("ambiguous next links"),
        Relation::One(next) => next,
    };
    let parameter = |name: &str| -> Result<Option<String>, &'static str> {
        let mut values = next
            .parameters
            .iter()
            .filter(|p| p.name.eq_ignore_ascii_case(name));
        let first = values.next();
        if values.next().is_some() {
            return Err("repeated Link parameter");
        }
        first
            .map(|p| {
                let raw = p
                    .raw_value
                    .as_deref()
                    .ok_or("missing Link parameter value")?;
                let text = raw
                    .strip_prefix('"')
                    .and_then(|s| s.strip_suffix('"'))
                    .unwrap_or(raw);
                let mut result = String::new();
                let mut chars = text.chars();
                while let Some(c) = chars.next() {
                    result.push(if c == '\\' {
                        chars.next().ok_or("bad Link escape")?
                    } else {
                        c
                    });
                }
                Ok(result)
            })
            .transpose()
    };
    if parameter("rel")?.is_none() || parameter("anchor")?.is_some() {
        return Err("unsupported Link context");
    }
    match parameter("results")?.as_deref() {
        Some("false") => return Ok(None),
        Some("true") => {}
        _ => return Err("next Link needs a boolean results flag"),
    }
    let target = &next.target;
    if target.origin() != current.origin()
        || target.path() != current.path()
        || !target.username().is_empty()
        || target.password().is_some()
        || target.fragment().is_some()
    {
        return Err("next Link leaves the current collection");
    }
    let mut cursors = Vec::new();
    let original: Vec<_> = current
        .query_pairs()
        .filter(|(k, _)| k != "cursor")
        .collect();
    for (key, value) in target.query_pairs() {
        if key == "cursor" {
            cursors.push(value.into_owned());
        } else if !key.is_empty() && !original.iter().any(|(k, v)| *k == key && *v == value) {
            return Err("next Link changes query filters");
        }
    }
    let [cursor] = cursors.as_slice() else {
        return Err("next Link needs one cursor");
    };
    if cursor.is_empty() || cursor.len() > 2048 {
        return Err("invalid cursor length");
    }
    if parameter("cursor")?.is_some_and(|p| p != *cursor) {
        return Err("cursor parameter disagrees with target");
    }
    if current
        .query_pairs()
        .any(|(k, v)| k == "cursor" && v == *cursor)
    {
        return Err("next cursor does not advance");
    }
    Ok(Some(cursor.clone()))
}
