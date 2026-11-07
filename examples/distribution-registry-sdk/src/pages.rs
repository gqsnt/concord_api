//! The catalog and tag listings, paged by the registry's `Link` header.
//!
//! **What a listing means.** The spec says the catalog's contents are registry-specific: a
//! registry may return its full catalog, limit it to what the credential may access, omit
//! upstream results when it is a mirror, or not offer the endpoint at all (an
//! [`Api`](crate::RegistryError::Api) error such as `404`, `401`/`403` or `UNSUPPORTED`). A
//! repository absent from the catalog may still exist. This SDK reports exactly the pages the
//! registry sends and claims nothing beyond them. Both listings are lexically ordered by the
//! registry.
//!
//! **Rules, each tested:**
//!
//! - The first page is the declared request (`n` and `last` if set); every later page is the
//!   `Link: <url>; rel="next"` target **as sent** (resolved against the response URL), never a
//!   URL the SDK builds: the registry may change `n` on later pages, and the spec says to use the
//!   link when paging forward.
//! - A link must stay on the registry's origin and name the same collection (same path):
//!   otherwise [`ProtocolProblem::ForeignLink`] / [`ProtocolProblem::OtherCollection`], not
//!   requested. A link equal to the URL just requested is [`ProtocolProblem::NotAdvancing`]; a
//!   malformed `Link` header is [`ProtocolProblem::MalformedLink`] — never "last page".
//! - `Ok(None)` from `next_page()` only after a page without `rel="next"` (the spec: no link means
//!   the client has all results). A failure is `PagesError::Provider` with the page to retry.
//! - Each page goes through the client's Bearer challenge handling (`registry:catalog:*` for the
//!   catalog, `repository:<name>:pull` for tags).

use std::fmt;
use std::sync::Arc;

use concord::reqwest::Url;
use concord::reqwest::header::{HeaderMap, LINK};
use concord_pagination::{Page, Pages, Resume, link};

use crate::auth::Access;
use crate::client::{Registry, Repository};
use crate::error::{ProtocolProblem, RegistryError, api_error};
use crate::meta::ResponseMeta;
use crate::reference::PageSize;

/// The page the registry linked to (`rel="next"`): the key of the next request, and where an
/// interrupted listing resumes. `Debug` withholds the URL.
#[derive(Clone, PartialEq, Eq)]
pub struct NextLink {
    url: Url,
    collection: Arc<str>,
}

impl NextLink {
    /// The URL as the registry sent it (resolved, fragment removed).
    pub fn url(&self) -> &Url {
        &self.url
    }
}

impl fmt::Debug for NextLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NextLink(<withheld>)")
    }
}

/// The first page's parameters: `n` (page size) and `last` (start after this entry). Later pages
/// follow the registry's links instead.
#[derive(Clone, Debug, Default)]
pub struct ListOptions {
    page_size: Option<PageSize>,
    last: Option<String>,
}

impl ListOptions {
    /// `n`: entries per page (the registry may cap it, or change it on later pages).
    pub fn page_size(mut self, n: PageSize) -> Self {
        self.page_size = Some(n);
        self
    }

    /// `last`: start after this entry (the spec allows skipping forward this way).
    pub fn after(mut self, last: impl Into<String>) -> Self {
        self.last = Some(last.into());
        self
    }
}

/// One catalog page: repository names, the next link, the response metadata.
pub type CatalogPage = Page<String, NextLink, ResponseMeta>;
/// The catalog traversal ([`Registry::catalog`]).
pub type CatalogPages = Pages<String, NextLink, ResponseMeta, RegistryError>;
/// One tag page.
pub type TagPage = Page<String, NextLink, ResponseMeta>;
/// The tag traversal ([`Repository::tags`]).
pub type TagPages = Pages<String, NextLink, ResponseMeta, RegistryError>;

impl Registry {
    /// `GET /v2/_catalog`, every page, one request at a time (see the [module documentation](self)).
    /// Nothing is sent until the first `next_page()`.
    pub fn catalog(&self, options: ListOptions) -> CatalogPages {
        self.catalog_from(options, Resume::Start)
    }

    /// [`catalog`](Self::catalog) resumed at `from` (a `PagesError`'s `resume()`).
    pub fn catalog_from(&self, options: ListOptions, from: Resume<NextLink>) -> CatalogPages {
        let registry = self.clone();
        Pages::new(from, move |key| {
            catalog_page(registry.clone(), options.clone(), key)
        })
    }
}

impl Repository {
    /// `GET /v2/<name>/tags/list`, every page (see the [module documentation](self)).
    pub fn tags(&self, options: ListOptions) -> TagPages {
        self.tags_from(options, Resume::Start)
    }

    /// [`tags`](Self::tags) resumed at `from`.
    pub fn tags_from(&self, options: ListOptions, from: Resume<NextLink>) -> TagPages {
        let repository = self.clone();
        Pages::new(from, move |key| {
            tag_page(repository.clone(), options.clone(), key)
        })
    }
}

async fn catalog_page(
    registry: Registry,
    options: ListOptions,
    key: Option<NextLink>,
) -> Result<CatalogPage, RegistryError> {
    let collection: Arc<str> = format!(
        "{}/v2/_catalog",
        registry.base_url().path().trim_end_matches('/')
    )
    .into();
    let link = checked_key(&registry, &collection, key)?;
    let root = registry.root().clone();
    let received = registry
        .execute(&Access::Catalog, |auth| {
            let prepared = root
                .clone()
                .maybe_authorization(auth)
                .catalog()
                .maybe_n(options.page_size)
                .maybe_last(options.last.clone())
                .prepare()?;
            match &link {
                Some(url) => prepared.follow(url.as_str()),
                None => Ok(prepared),
            }
        })
        .await?;
    if !received.status().is_success() {
        return Err(api_error(received).await);
    }
    let meta = ResponseMeta::capture(&received);
    let next = next_link(&registry, &collection, received.url(), &meta)?;
    let body = received.decode().await?;
    Ok(Page {
        items: body.repositories.unwrap_or_default(),
        next,
        meta,
    })
}

async fn tag_page(
    repository: Repository,
    options: ListOptions,
    key: Option<NextLink>,
) -> Result<TagPage, RegistryError> {
    let registry = repository.registry().clone();
    let collection: Arc<str> = format!(
        "{}/v2/{}/tags/list",
        registry.base_url().path().trim_end_matches('/'),
        repository.name()
    )
    .into();
    let link = checked_key(&registry, &collection, key)?;
    let api = repository.api.clone();
    let received = registry
        .execute(&repository.pull(), |auth| {
            let prepared = api
                .clone()
                .maybe_authorization(auth)
                .tags()
                .maybe_n(options.page_size)
                .maybe_last(options.last.clone())
                .prepare()?;
            match &link {
                Some(url) => prepared.follow(url.as_str()),
                None => Ok(prepared),
            }
        })
        .await?;
    if !received.status().is_success() {
        return Err(api_error(received).await);
    }
    let meta = ResponseMeta::capture(&received);
    let next = next_link(&registry, &collection, received.url(), &meta)?;
    let body = received.decode().await?;
    if body.name != repository.name().as_str() {
        return Err(RegistryError::protocol(
            ProtocolProblem::NameMismatch,
            Some(meta),
        ));
    }
    Ok(Page {
        items: body.tags.unwrap_or_default(),
        next,
        meta,
    })
}

/// A resume key is used only for its own listing, and re-checked: the caller can pass any.
fn checked_key(
    registry: &Registry,
    collection: &Arc<str>,
    key: Option<NextLink>,
) -> Result<Option<Url>, RegistryError> {
    let Some(key) = key else {
        return Ok(None);
    };
    if key.collection != *collection {
        return Err(RegistryError::protocol(
            ProtocolProblem::ContinuationMismatch,
            None,
        ));
    }
    check_link(registry, collection, &key.url).map_err(|p| RegistryError::protocol(p, None))?;
    Ok(Some(key.url))
}

/// The checked `rel="next"` of a page, read from the head before the body.
fn next_link(
    registry: &Registry,
    collection: &Arc<str>,
    requested: &Url,
    meta: &ResponseMeta,
) -> Result<Option<NextLink>, RegistryError> {
    let protocol = |problem| RegistryError::protocol(problem, Some(meta.clone()));
    let Some(mut url) = parse_next(meta.headers(), requested).map_err(protocol)? else {
        return Ok(None);
    };
    url.set_fragment(None);
    check_link(registry, collection, &url).map_err(protocol)?;
    let mut requested = requested.clone();
    requested.set_fragment(None);
    if url == requested {
        return Err(protocol(ProtocolProblem::NotAdvancing));
    }
    Ok(Some(NextLink {
        url,
        collection: collection.clone(),
    }))
}

fn check_link(registry: &Registry, collection: &str, url: &Url) -> Result<(), ProtocolProblem> {
    if url.origin() != registry.base_url().origin()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(ProtocolProblem::ForeignLink);
    }
    if url.path() != collection {
        return Err(ProtocolProblem::OtherCollection);
    }
    Ok(())
}

/// The `rel="next"` target of the `Link` field lines (RFC 8288), resolved against `base`.
/// Parsing is strict where a guess would end a listing silently: an unterminated target or
/// quoted string, a malformed parameter, an unresolvable target, or two different `next` targets
/// are [`ProtocolProblem::MalformedLink`]. Other relations are ignored; only the first `rel`
/// parameter of a link counts, and it may list several relation types.
pub(crate) fn parse_next(headers: &HeaderMap, base: &Url) -> Result<Option<Url>, ProtocolProblem> {
    let links = link::parse(
        headers.get_all(LINK).iter().map(|value| value.as_bytes()),
        base,
    )
    .map_err(|_| ProtocolProblem::MalformedLink)?;
    // Identical repeated targets are harmless; distinct next targets are ambiguous.
    let mut next = None;
    for link in links.into_iter().filter(|link| link.has_relation("next")) {
        match &next {
            Some(existing) if *existing != link.target => {
                return Err(ProtocolProblem::MalformedLink);
            }
            _ => next = Some(link.target),
        }
    }
    Ok(next)
}

#[cfg(test)]
mod tests {
    use concord::reqwest::header::HeaderValue;

    use super::*;

    fn next_of(values: &[&str]) -> Result<Option<String>, ProtocolProblem> {
        let mut headers = HeaderMap::new();
        for value in values {
            headers.append(LINK, HeaderValue::from_str(value).unwrap());
        }
        let base = Url::parse("https://registry.test/v2/_catalog?n=2").unwrap();
        parse_next(&headers, &base).map(|url| url.map(|u| u.to_string()))
    }

    #[test]
    fn the_documented_form_resolves_against_the_response() {
        assert_eq!(
            next_of(&[r#"</v2/_catalog?last=b&n=2>; rel="next""#])
                .unwrap()
                .as_deref(),
            Some("https://registry.test/v2/_catalog?last=b&n=2")
        );
        assert_eq!(
            next_of(&[r#"<https://other.test/x>; rel="prev", </v2/_catalog?last=c>; rel=next"#])
                .unwrap()
                .as_deref(),
            Some("https://registry.test/v2/_catalog?last=c")
        );
        assert_eq!(
            next_of(&[r#"</a>; rel="prev""#, r#"</b>; title="x"; rel="last next""#])
                .unwrap()
                .as_deref(),
            Some("https://registry.test/b")
        );
        assert_eq!(next_of(&[]), Ok(None));
    }

    #[test]
    fn malformed_links_are_errors_not_the_last_page() {
        for bad in [
            r#"/v2/_catalog; rel="next""#,
            r#"</v2/_catalog; rel="next""#,
            r#"</v2/_catalog>; rel="next"#,
            r#"</v2/_catalog>; ="next""#,
            r#"</v2/_catalog>; rel="next" junk"#,
            r#"</a>; rel="next", </b>; rel="next""#,
        ] {
            assert_eq!(
                next_of(&[bad]),
                Err(ProtocolProblem::MalformedLink),
                "{bad}"
            );
        }
    }
}
