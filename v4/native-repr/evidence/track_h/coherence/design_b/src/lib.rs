//! Design B — reuse `http`'s own conversions, as `reqwest::RequestBuilder::header` does
//! (`HeaderValue: TryFrom<V>`). No Concord trait: a domain type opts in with
//! `impl From<&MyType> for HeaderValue` (or `TryFrom`).
//!
//! Two shapes are possible for a value read from owner state:
//! - by reference: `for<'a> HeaderValue: TryFrom<&'a T>` (no clone bound; integers fail: `http`
//!   converts them by value only);
//! - by clone: `T: Clone, HeaderValue: TryFrom<T>` (integers work; `str` cannot be cloned out).

use http::{HeaderMap, HeaderName, HeaderValue};

#[derive(Default)]
pub struct Parts {
    pub headers: HeaderMap,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Invalid(pub &'static str);

impl Parts {
    fn push(&mut self, name: HeaderName, what: &'static str, mut value: HeaderValue, sensitive: bool) -> Result<(), Invalid> {
        if sensitive {
            value.set_sensitive(true);
        }
        self.headers.try_append(name, value).map_err(|_| Invalid(what))?;
        Ok(())
    }

    /// By reference.
    pub fn header_ref<T: ?Sized>(&mut self, name: HeaderName, what: &'static str, value: &T, sensitive: bool) -> Result<(), Invalid>
    where
        for<'a> HeaderValue: TryFrom<&'a T>,
    {
        let value = HeaderValue::try_from(value).map_err(|_| Invalid(what))?;
        self.push(name, what, value, sensitive)
    }

    /// By clone.
    pub fn header_owned<T: Clone>(&mut self, name: HeaderName, what: &'static str, value: &T, sensitive: bool) -> Result<(), Invalid>
    where
        HeaderValue: TryFrom<T>,
    {
        let value = HeaderValue::try_from(value.clone()).map_err(|_| Invalid(what))?;
        self.push(name, what, value, sensitive)
    }

    /// `each`, by reference: items of `&Vec<T>` are `&T`.
    pub fn header_each_ref<I>(&mut self, name: HeaderName, what: &'static str, values: I, sensitive: bool) -> Result<(), Invalid>
    where
        I: IntoIterator,
        HeaderValue: TryFrom<I::Item>,
    {
        for value in values {
            let value = HeaderValue::try_from(value).map_err(|_| Invalid(what))?;
            self.push(name.clone(), what, value, sensitive)?;
        }
        Ok(())
    }
}
