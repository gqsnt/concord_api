//! Built-in pagination controllers for core's single traversal engine.
//!
//! Every controller has a public initialization type (built by the
//! declaration's `init` clause) and private state, and maps one whole cursor
//! value onto the declared cursor slot:
//!
//! | Controller | Init | Cursor | Completion |
//! |---|---|---|---|
//! | [`OffsetPagination`] | [`OffsetInit`] | `u64` | short page is `Done`; else checked `offset + page_size` |
//! | [`PagePagination`] | [`PageInit`] | `NonZeroU64` | short page is `Done`; else checked `page + 1` |
//! | [`CursorPagination<C>`] | [`CursorInit<C>`] | `Option<C>` | absent [`HasNextCursor`] cursor is `Done` |
//! | [`NextHeaderPagination`] | [`CursorInit<String>`] | `Option<String>` | absent `X-Next-Cursor` is `Done` |
//! | [`LinkCursorPagination`] | [`CursorInit<String>`] | `Option<String>` | absent `rel=next` is `Done` |
//!
//! Controllers never truncate a page: a batch larger than the remaining item
//! budget fails with `CollectedItemCeilingExceeded`. Core independently
//! validates every continuation (shaping, fixed facts, cycles and budgets).

use std::num::{NonZeroU32, NonZeroU64};

use concord_core::contracts::{HeaderName, Url};
use concord_core::pagination::{
    PageDecisionLimits, PageResponse, PageStep, PaginationController, PaginationDescription,
    PaginationError, PaginationErrorKind,
};

/// A decoded page that exposes an optional next cursor by reference.
///
/// [`CursorPagination`] calls `next_cursor` once per decoded page inside its
/// controller `advance` step and clones the returned cursor into the next
/// continuation; an absent cursor completes the traversal.
///
/// # Extension contract
///
/// * **Ownership.** The page is borrowed for one call; the returned reference
///   borrows from the page and is cloned by the controller, never retained.
/// * **Concurrency.** Each traversal calls it on its own page value; no
///   state is shared between traversals.
/// * **Cancellation.** The callback is synchronous, must be non-blocking and
///   perform no external I/O; it runs under the traversal's aggregate
///   deadline checks.
/// * **Input sensitivity.** The cursor and page may be sensitive.
///   Implementations own logging and storage; core never prints cursors or
///   pages.
/// * **Errors.** The callback cannot fail; an absent cursor (`None`) is
///   exhaustion, distinct from a present empty cursor.
/// * **Resource obligations.** Implementations must keep synchronous CPU,
///   memory, retries, maps, queues, tasks and metric cardinality finite.
/// * **Negative capabilities.** The page only exposes a cursor: it cannot
///   choose the destination slot, send requests or change any other
///   endpoint value, and core re-validates every continuation.
#[diagnostic::on_unimplemented(
    message = "[C3-PAGE] page type `{Self}` does not expose a next cursor of type `{C}`",
    label = "`CursorPagination<{C}>` reads the next cursor from this page",
    note = "implement `HasNextCursor<{C}>` for the decoded page, returning `None` when the provider reports no further page",
    note = "or select a controller that matches this page shape with `paginate via Controller {{ ... }}`"
)]
pub trait HasNextCursor<C> {
    /// Returns the next cursor, distinguishing an absent cursor from a present
    /// cursor whose value is empty.
    fn next_cursor(&self) -> Option<&C>;
}

/// Public initialization of [`OffsetPagination`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OffsetInit {
    /// Zero-based offset of the first requested page.
    pub offset: u64,
    /// Exact page size requested from the provider; a shorter page proves
    /// exhaustion.
    pub page_size: NonZeroU32,
}

/// Checked offset pagination. `Cursor = u64` (the offset slot).
pub struct OffsetPagination {
    offset: u64,
    page_size: NonZeroU32,
}

impl<Page> PaginationController<Page> for OffsetPagination
where
    Page: IntoIterator + Send + 'static,
    Page::Item: Send + 'static,
{
    type Item = Page::Item;
    type Init = OffsetInit;
    type Cursor = u64;

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

    fn start(init: OffsetInit) -> Result<Self, PaginationError> {
        Ok(Self {
            offset: init.offset,
            page_size: init.page_size,
        })
    }

    fn cursor(&self) -> u64 {
        self.offset
    }

    fn advance(
        self,
        page: PageResponse<Page>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
        let items = collect_items(page.into_page(), limits)?;
        if is_short(items.len(), self.page_size) {
            return Ok(PageStep::Done { items });
        }
        let offset = self
            .offset
            .checked_add(u64::from(self.page_size.get()))
            .ok_or_else(|| PaginationError::new(PaginationErrorKind::Overflow))?;
        Ok(PageStep::Continue {
            next: Self {
                offset,
                page_size: self.page_size,
            },
            items,
        })
    }
}

/// Public initialization of [`PagePagination`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageInit {
    /// One-based number of the first requested page.
    pub page: NonZeroU64,
    /// Exact page size requested from the provider; a shorter page proves
    /// exhaustion.
    pub page_size: NonZeroU32,
}

/// Checked one-based page-number pagination. `Cursor = NonZeroU64`.
pub struct PagePagination {
    page: NonZeroU64,
    page_size: NonZeroU32,
}

impl<Page> PaginationController<Page> for PagePagination
where
    Page: IntoIterator + Send + 'static,
    Page::Item: Send + 'static,
{
    type Item = Page::Item;
    type Init = PageInit;
    type Cursor = NonZeroU64;

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

    fn start(init: PageInit) -> Result<Self, PaginationError> {
        Ok(Self {
            page: init.page,
            page_size: init.page_size,
        })
    }

    fn cursor(&self) -> NonZeroU64 {
        self.page
    }

    fn advance(
        self,
        response: PageResponse<Page>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
        let items = collect_items(response.into_page(), limits)?;
        if is_short(items.len(), self.page_size) {
            return Ok(PageStep::Done { items });
        }
        let page = self
            .page
            .checked_add(1)
            .ok_or_else(|| PaginationError::new(PaginationErrorKind::Overflow))?;
        Ok(PageStep::Continue {
            next: Self {
                page,
                page_size: self.page_size,
            },
            items,
        })
    }
}

/// Public initialization shared by the cursor controllers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CursorInit<C> {
    /// Cursor of the first request; `None` requests the first page without a
    /// cursor.
    pub cursor: Option<C>,
}

/// Typed cursor pagination driven by the page's [`HasNextCursor`] contract.
/// `Cursor = Option<C>`.
///
/// An absent next cursor proves exhaustion. A present empty cursor remains a
/// present candidate and is validated by core like any other continuation.
pub struct CursorPagination<C> {
    cursor: Option<C>,
}

impl<Page, C> PaginationController<Page> for CursorPagination<C>
where
    Page: HasNextCursor<C> + IntoIterator + Send + 'static,
    Page::Item: Send + 'static,
    C: Clone + Send + Sync + 'static,
{
    type Item = Page::Item;
    type Init = CursorInit<C>;
    type Cursor = Option<C>;

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

    fn start(init: CursorInit<C>) -> Result<Self, PaginationError> {
        Ok(Self {
            cursor: init.cursor,
        })
    }

    fn cursor(&self) -> Option<C> {
        self.cursor.clone()
    }

    fn advance(
        self,
        page: PageResponse<Page>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
        let next = page.page().next_cursor().cloned();
        let items = collect_items(page.into_page(), limits)?;
        Ok(match next {
            Some(cursor) => PageStep::Continue {
                next: Self {
                    cursor: Some(cursor),
                },
                items,
            },
            None => PageStep::Done { items },
        })
    }
}

/// The response header read by [`NextHeaderPagination`].
pub const NEXT_CURSOR_HEADER: &str = "x-next-cursor";

/// Cursor pagination driven by the declared `X-Next-Cursor` response header.
/// `Cursor = Option<String>`.
///
/// An absent header proves exhaustion; more than one field value or a value
/// that is not visible ASCII text is `InvalidContinuation`.
pub struct NextHeaderPagination {
    cursor: Option<String>,
}

impl<Page> PaginationController<Page> for NextHeaderPagination
where
    Page: IntoIterator + Send + 'static,
    Page::Item: Send + 'static,
{
    type Item = Page::Item;
    type Init = CursorInit<String>;
    type Cursor = Option<String>;

    const DESCRIPTION: PaginationDescription =
        PaginationDescription::new(&[NEXT_CURSOR_HEADER], false);

    fn start(init: CursorInit<String>) -> Result<Self, PaginationError> {
        Ok(Self {
            cursor: init.cursor,
        })
    }

    fn cursor(&self) -> Option<String> {
        self.cursor.clone()
    }

    fn advance(
        self,
        page: PageResponse<Page>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
        let name = HeaderName::from_static(NEXT_CURSOR_HEADER);
        let next = {
            let mut values = page.header_values(&name);
            let next = values
                .next()
                .map(|value| {
                    value
                        .to_str()
                        .map(str::to_owned)
                        .map_err(|_| invalid_continuation())
                })
                .transpose()?;
            if values.next().is_some() {
                return Err(invalid_continuation());
            }
            next
        };
        let items = collect_items(page.into_page(), limits)?;
        Ok(match next {
            Some(cursor) => PageStep::Continue {
                next: Self {
                    cursor: Some(cursor),
                },
                items,
            },
            None => PageStep::Done { items },
        })
    }
}

/// Same-origin RFC 8288 `Link` cursor pagination owning only the `cursor`
/// query slot. `Cursor = Option<String>`.
///
/// The controller declares the `Link` header and the current logical URI. It
/// parses exactly one `rel=next` link (commas inside `<...>` and quoted
/// parameters are handled), resolves it against the current logical URI and
/// requires the same origin ([`PaginationErrorKind::CrossOriginContinuation`]
/// otherwise). It rejects, as `InvalidContinuation`, a next link that changes
/// the path, carries userinfo or a fragment, changes any non-cursor query
/// occurrence, or does not carry exactly one `cursor` query value. The link is
/// never used as an arbitrary request URL: only the cursor value is mapped.
/// An absent `rel=next` proves exhaustion.
pub struct LinkCursorPagination {
    cursor: Option<String>,
}

impl<Page> PaginationController<Page> for LinkCursorPagination
where
    Page: IntoIterator + Send + 'static,
    Page::Item: Send + 'static,
{
    type Item = Page::Item;
    type Init = CursorInit<String>;
    type Cursor = Option<String>;

    const DESCRIPTION: PaginationDescription = PaginationDescription::new(&["link"], true);

    fn start(init: CursorInit<String>) -> Result<Self, PaginationError> {
        Ok(Self {
            cursor: init.cursor,
        })
    }

    fn cursor(&self) -> Option<String> {
        self.cursor.clone()
    }

    fn advance(
        self,
        page: PageResponse<Page>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
        let current = page.current_uri().ok_or_else(|| {
            PaginationError::new(PaginationErrorKind::InvalidControllerDescription)
        })?;
        let name = HeaderName::from_static("link");
        let values = page
            .header_values(&name)
            .map(|value| value.to_str().map_err(|_| invalid_continuation()))
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = resolve_link_cursor(current, values)?;

        let items = collect_items(page.into_page(), limits)?;
        Ok(match next_cursor {
            Some(cursor) => PageStep::Continue {
                next: Self {
                    cursor: Some(cursor),
                },
                items,
            },
            None => PageStep::Done { items },
        })
    }
}

const fn invalid_continuation() -> PaginationError {
    PaginationError::new(PaginationErrorKind::InvalidContinuation)
}

fn is_short(items: usize, page_size: NonZeroU32) -> bool {
    u32::try_from(items).is_ok_and(|items| items < page_size.get())
}

fn resolve_link_cursor<'a>(
    current: &Url,
    values: impl IntoIterator<Item = &'a str>,
) -> Result<Option<String>, PaginationError> {
    let mut next_target = None;
    for value in values {
        for entry in split_link_entries(value)? {
            if entry.is_empty() {
                continue;
            }
            let Some(target) = next_link_target(entry)? else {
                continue;
            };
            if next_target.is_some() {
                return Err(invalid_continuation());
            }
            next_target = Some(target);
        }
    }
    let Some(target) = next_target else {
        return Ok(None);
    };
    let resolved = current.join(target).map_err(|_| invalid_continuation())?;
    if resolved.origin() != current.origin() {
        return Err(PaginationError::new(
            PaginationErrorKind::CrossOriginContinuation,
        ));
    }
    if !resolved.username().is_empty()
        || resolved.password().is_some()
        || resolved.fragment().is_some()
        || resolved.path() != current.path()
    {
        return Err(invalid_continuation());
    }
    let other_pairs = |url: &Url| {
        url.query_pairs()
            .filter(|(name, _)| name != "cursor")
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect::<Vec<_>>()
    };
    if other_pairs(&resolved) != other_pairs(current) {
        return Err(invalid_continuation());
    }
    let mut cursors = resolved
        .query_pairs()
        .filter(|(name, _)| name == "cursor")
        .map(|(_, value)| value.into_owned());
    let cursor = cursors.next().ok_or_else(invalid_continuation)?;
    if cursors.next().is_some() {
        return Err(invalid_continuation());
    }
    Ok(Some(cursor))
}

/// Splits one `Link` field value into entries at top-level commas only.
fn split_link_entries(value: &str) -> Result<Vec<&str>, PaginationError> {
    let mut entries = Vec::new();
    let mut start = 0_usize;
    let mut in_angle = false;
    let mut quoted = false;
    let mut escaped = false;
    for (index, byte) in value.bytes().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        match byte {
            b'\\' if quoted => escaped = true,
            b'"' if !in_angle => quoted = !quoted,
            b'<' if !quoted && !in_angle => in_angle = true,
            b'>' if !quoted && in_angle => in_angle = false,
            b',' if !quoted && !in_angle => {
                entries.push(value[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    if escaped || quoted || in_angle {
        return Err(invalid_continuation());
    }
    entries.push(value[start..].trim());
    Ok(entries)
}

/// Returns the target of one link entry when its `rel` parameter contains the
/// `next` relation type.
fn next_link_target(entry: &str) -> Result<Option<&str>, PaginationError> {
    let rest = entry.strip_prefix('<').ok_or_else(invalid_continuation)?;
    let target_end = rest.find('>').ok_or_else(invalid_continuation)?;
    let target = &rest[..target_end];
    let parameters = &rest[target_end + 1..];
    let mut is_next = false;
    for parameter in split_parameters(parameters)? {
        let parameter = parameter.trim();
        if parameter.is_empty() {
            continue;
        }
        let Some((name, value)) = parameter.split_once('=') else {
            continue;
        };
        if !name.trim().eq_ignore_ascii_case("rel") {
            continue;
        }
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .unwrap_or(value);
        if value
            .split_ascii_whitespace()
            .any(|relation| relation.eq_ignore_ascii_case("next"))
        {
            is_next = true;
        }
    }
    Ok(is_next.then_some(target))
}

/// Splits link parameters at `;` outside quoted strings.
fn split_parameters(parameters: &str) -> Result<Vec<&str>, PaginationError> {
    let mut parts = Vec::new();
    let mut start = 0_usize;
    let mut quoted = false;
    let mut escaped = false;
    for (index, byte) in parameters.bytes().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        match byte {
            b'\\' if quoted => escaped = true,
            b'"' => quoted = !quoted,
            b';' if !quoted => {
                parts.push(&parameters[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    if quoted || escaped {
        return Err(invalid_continuation());
    }
    parts.push(&parameters[start..]);
    let mut parts = parts.into_iter();
    // Anything between `>` and the first `;` must be whitespace.
    if parts
        .next()
        .is_some_and(|leading| !leading.trim().is_empty())
    {
        return Err(invalid_continuation());
    }
    Ok(parts.collect())
}

/// Collects a page's complete batch, failing (never truncating) when it would
/// exceed the remaining item budget.
fn collect_items<Page>(
    page: Page,
    limits: PageDecisionLimits,
) -> Result<Vec<Page::Item>, PaginationError>
where
    Page: IntoIterator,
{
    let mut items = Vec::new();
    for item in page {
        if items.len() == limits.remaining_items() {
            return Err(PaginationError::new(
                PaginationErrorKind::CollectedItemCeilingExceeded,
            ));
        }
        items.push(item);
    }
    Ok(items)
}

#[cfg(test)]
mod tests;
