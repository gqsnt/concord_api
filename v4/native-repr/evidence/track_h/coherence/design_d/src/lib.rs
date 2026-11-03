//! Design D — syntax-directed: the declaration says which route a header value takes.
//! `header("accept") native accept: HeaderValue` → `header_native` (`Borrow<HeaderValue>`);
//! anything else → `header_text` (`Display`, as today). The default route is wrapped in a Concord
//! trait so a forgotten keyword can carry a hint (whether the hint is shown is a probe).

use std::borrow::Borrow;
use std::fmt::{self, Display};

use http::{HeaderMap, HeaderName, HeaderValue};

/// The `Display` route, behind a Concord trait for the diagnostic.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not formatted header text",
    label = "header values without `native` are written with `Display`",
    note = "for a native `HeaderValue`, write `header(\"..\") native x: HeaderValue`"
)]
pub trait HeaderTextRoute: Display {}
impl<T: Display + ?Sized> HeaderTextRoute for T {}

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

    pub fn header_text<T: HeaderTextRoute + ?Sized>(&mut self, name: HeaderName, what: &'static str, value: &T, sensitive: bool) -> Result<(), Invalid> {
        let mut text = String::new();
        fmt::Write::write_fmt(&mut text, format_args!("{value}")).map_err(|_| Invalid(what))?;
        let visible = text.bytes().all(|b| b == b'\t' || (0x20..0x7f).contains(&b));
        let value = HeaderValue::from_str(&text).ok().filter(|_| visible).ok_or(Invalid(what))?;
        self.push(name, what, value, sensitive)
    }

    /// Plain `Display` bound (what Concord has today), for comparing diagnostics.
    pub fn header_plain<T: Display + ?Sized>(&mut self, name: HeaderName, what: &'static str, value: &T, sensitive: bool) -> Result<(), Invalid> {
        self.header_text(name, what, value, sensitive)
    }

    pub fn header_native<T: Borrow<HeaderValue> + ?Sized>(&mut self, name: HeaderName, what: &'static str, value: &T, sensitive: bool) -> Result<(), Invalid> {
        let value: &HeaderValue = value.borrow();
        self.push(name, what, value.clone(), sensitive)
    }
}
