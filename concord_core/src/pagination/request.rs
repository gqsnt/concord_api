use std::fmt::Display;

use http::HeaderName;

use super::PaginationError;
use crate::error::PaginationErrorKind;

pub(crate) enum QueryPatch {
    Set { name: String, value: String },
    Remove { name: String },
}

pub(crate) enum HeaderPatch {
    Set { name: HeaderName, value: String },
    Remove { name: HeaderName },
}

#[derive(Default)]
pub(crate) struct PaginationPatch {
    pub(crate) query: Vec<QueryPatch>,
    pub(crate) headers: Vec<HeaderPatch>,
}

/// A capability-limited public-query and public-header patch for one page.
///
/// The wrapper exposes no body, credentials, host, path, native URL, native
/// request, transport, or generated endpoint storage.
pub struct PaginationRequest<'a> {
    patch: &'a mut PaginationPatch,
}

impl<'a> PaginationRequest<'a> {
    pub(crate) fn new(patch: &'a mut PaginationPatch) -> Self {
        Self { patch }
    }

    /// Sets one public query value, replacing every existing value with the
    /// same name while preserving all unrelated pair order.
    pub fn set_query<T>(&mut self, name: impl AsRef<str>, value: T) -> Result<(), PaginationError>
    where
        T: Display,
    {
        let name = validate_query_name(name.as_ref())?;
        self.patch.query.push(QueryPatch::Set {
            name,
            value: value.to_string(),
        });
        Ok(())
    }

    /// Removes every public query value with the supplied name.
    pub fn remove_query(&mut self, name: impl AsRef<str>) -> Result<(), PaginationError> {
        let name = validate_query_name(name.as_ref())?;
        self.patch.query.push(QueryPatch::Remove { name });
        Ok(())
    }

    /// Sets one validated public header value.
    pub fn set_header<T>(&mut self, name: HeaderName, value: T) -> Result<(), PaginationError>
    where
        T: Display,
    {
        if crate::request::reserved_public_header(&name) {
            return Err(controller_error());
        }
        let value = value.to_string();
        if http::HeaderValue::from_str(&value).is_err() {
            return Err(controller_error());
        }
        self.patch.headers.push(HeaderPatch::Set { name, value });
        Ok(())
    }

    /// Removes one public header value.
    pub fn remove_header(&mut self, name: &HeaderName) -> Result<(), PaginationError> {
        if crate::request::reserved_public_header(name) {
            return Err(controller_error());
        }
        self.patch
            .headers
            .push(HeaderPatch::Remove { name: name.clone() });
        Ok(())
    }
}

fn validate_query_name(name: &str) -> Result<String, PaginationError> {
    if name.is_empty() || name.chars().any(char::is_control) {
        Err(controller_error())
    } else {
        Ok(name.to_owned())
    }
}

fn controller_error() -> PaginationError {
    PaginationError::new(PaginationErrorKind::Controller)
}
