//! GraphQL connections (`nodes` + `pageInfo`) and the adapter that turns one connection page
//! of a typed response into a [`concord_pagination::Page`].
//!
//! # The adapter's contract
//!
//! [`connection_page`] awaits one response, selects the continuation from `pageInfo` (the full
//! connection) **before** moving the nodes out, and keeps the response's `errors` and cost in the
//! page's [`PageMeta`]. It decides only what is not policy:
//!
//! - a connection that is not there — `data` absent or `null`, or a nullable parent that came back
//!   `null` (a deleted product, an unknown location) — is a [`PageError::NoConnection`] carrying
//!   the errors and the cost, never an empty page;
//! - an inconsistent `pageInfo` (`hasNextPage` without an `endCursor`, or an `endCursor` equal to
//!   the cursor just requested while `hasNextPage` is true) is a [`PageError::Protocol`] failure,
//!   never a guessed continuation or an endless chain.
//!
//! A page **with** data and errors (a field nulled by a per-field `ACCESS_DENIED`, say) is an
//! `Ok` page whose `meta.errors` is not empty: whether that is acceptable is the workflow's
//! policy, applied in its own page factory.

use std::fmt;
use std::future::Future;

use concord_pagination::Page;
use serde::Deserialize;

use crate::context::RequestError;
use crate::graphql::{Data, GraphqlError, GraphqlResponse, QueryCost, nullable};
use crate::scalars::Cursor;

/// A connection selected as `nodes { .. } pageInfo { hasNextPage endCursor }`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection<N> {
    /// The page's nodes.
    pub nodes: Vec<N>,
    /// Where the page ends.
    pub page_info: PageInfo,
}

/// `pageInfo { hasNextPage endCursor }`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    /// Whether results follow this page.
    pub has_next_page: bool,
    /// The cursor of the last node (`null` on an empty page).
    #[serde(deserialize_with = "nullable")]
    pub end_cursor: Option<Cursor>,
}

impl PageInfo {
    /// The `after` value of the following page: `None` when there is none.
    ///
    /// # Errors
    ///
    /// [`Protocol::NextWithoutCursor`] when `hasNextPage` is true without an `endCursor`;
    /// [`Protocol::CursorDidNotAdvance`] when it equals `requested`.
    pub fn next(&self, requested: Option<&Cursor>) -> Result<Option<Cursor>, Protocol> {
        if !self.has_next_page {
            return Ok(None);
        }
        let next = self.end_cursor.clone().ok_or(Protocol::NextWithoutCursor)?;
        if requested == Some(&next) {
            return Err(Protocol::CursorDidNotAdvance);
        }
        Ok(Some(next))
    }
}

impl<N> Connection<N> {
    /// Splits the connection into its nodes and the following page's cursor (see
    /// [`PageInfo::next`]).
    ///
    /// # Errors
    ///
    /// The [`Protocol`] violation of `pageInfo`.
    pub fn into_parts(
        self,
        requested: Option<&Cursor>,
    ) -> Result<(Vec<N>, Option<Cursor>), Protocol> {
        let next = self.page_info.next(requested)?;
        Ok((self.nodes, next))
    }
}

/// What a connection page carries beside its nodes.
#[derive(Debug, Default)]
pub struct PageMeta {
    /// The response's errors (data was present beside them).
    pub errors: Vec<GraphqlError>,
    /// The response's cost report.
    pub cost: Option<QueryCost>,
}

/// A `pageInfo` that cannot be followed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    /// `hasNextPage: true` with `endCursor: null`.
    NextWithoutCursor,
    /// `hasNextPage: true` and the `endCursor` is the cursor just requested.
    CursorDidNotAdvance,
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NextWithoutCursor => "hasNextPage is true but endCursor is null",
            Self::CursorDidNotAdvance => "endCursor did not advance past the requested cursor",
        })
    }
}

/// Why a connection page is not a page.
///
/// `Display`/`Debug` name the failure, error codes and counts, never cursors or contents.
#[derive(concord::Debug, concord::derive::Error)]
#[display(with = display_page_error)]
pub enum PageError {
    /// The request failed or the envelope did not decode (Concord's error, unread response kept
    /// for a status error), or the served API version was not the pinned one.
    Request(#[source] RequestError),
    /// The response has no such connection: `data` absent or `null`, or a nullable parent was
    /// `null`. The errors explain why (throttling, access, an unknown id); `data` says which.
    NoConnection {
        /// What `data` was.
        data: DataShape,
        /// The response's errors (possibly empty: an unknown id is `null` without error).
        errors: Vec<GraphqlError>,
        /// The response's cost report, if any.
        cost: Option<QueryCost>,
    },
    /// `pageInfo` cannot be followed.
    Protocol {
        /// The violation.
        violation: Protocol,
        /// The response's errors.
        errors: Vec<GraphqlError>,
        /// The response's cost report.
        cost: Option<QueryCost>,
    },
}

/// The shape of `data` in a response without the expected connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataShape {
    /// No `data` member: the request failed before execution.
    Absent,
    /// `"data": null`.
    Null,
    /// Data present, but the connection's nullable parent was `null`.
    ParentNull,
}

impl PageError {
    /// The GraphQL errors of the response, if one was decoded.
    pub fn graphql_errors(&self) -> &[GraphqlError] {
        match self {
            Self::Request(_) => &[],
            Self::NoConnection { errors, .. } | Self::Protocol { errors, .. } => errors,
        }
    }

    /// The cost report of the response, if one was decoded.
    pub fn cost(&self) -> Option<&QueryCost> {
        match self {
            Self::Request(_) => None,
            Self::NoConnection { cost, .. } | Self::Protocol { cost, .. } => cost.as_ref(),
        }
    }

    /// Whether an error carries `THROTTLED`.
    pub fn is_throttled(&self) -> bool {
        self.graphql_errors()
            .iter()
            .any(|e| e.code() == Some(&crate::graphql::ErrorCode::Throttled))
    }
}

fn display_page_error(value: &PageError, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match value {
        PageError::Request(e) => write!(f, "connection page: {e}"),
        PageError::NoConnection { data, errors, .. } => {
            write!(f, "connection page: no connection (data {data:?}")?;
            match errors.first() {
                Some(first) => write!(f, ", {} error(s), first: {first})", errors.len()),
                None => f.write_str(", no error)"),
            }
        }
        PageError::Protocol { violation, .. } => write!(f, "connection page: {violation}"),
    }
}

/// Awaits `response` (one request of a connection page, sent with `after = requested`) and
/// projects the connection out of its data; see the [module documentation](self).
///
/// `project` receives the whole typed `data` and moves the connection out of it (`None` when its
/// nullable parent was `null`).
///
/// # Errors
///
/// [`PageError`]; the response's errors and cost travel with it.
pub async fn connection_page<D, N, Fut>(
    response: Fut,
    requested: Option<&Cursor>,
    project: impl FnOnce(D) -> Option<Connection<N>>,
) -> Result<Page<N, Cursor, PageMeta>, PageError>
where
    Fut: Future<Output = Result<GraphqlResponse<D>, RequestError>>,
{
    let GraphqlResponse {
        data,
        errors,
        extensions,
    } = response.await?;
    let cost = extensions.and_then(|e| e.cost);
    let data = match data {
        Data::Present(data) => data,
        Data::Absent | Data::Null => {
            let shape = if matches!(data, Data::Absent) {
                DataShape::Absent
            } else {
                DataShape::Null
            };
            return Err(PageError::NoConnection {
                data: shape,
                errors,
                cost,
            });
        }
    };
    let Some(connection) = project(data) else {
        return Err(PageError::NoConnection {
            data: DataShape::ParentNull,
            errors,
            cost,
        });
    };
    match connection.into_parts(requested) {
        Ok((items, next)) => Ok(Page {
            items,
            next,
            meta: PageMeta { errors, cost },
        }),
        Err(violation) => Err(PageError::Protocol {
            violation,
            errors,
            cost,
        }),
    }
}
