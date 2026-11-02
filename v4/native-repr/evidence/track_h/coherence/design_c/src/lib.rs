//! Design C — autoref-based dispatch in generated code ("autoref specialization").
//!
//! The macro emits `(&Probe(&value)).__header()` with both traits in scope. Method resolution
//! tries the receiver `&Probe<T>` first, where only the native impl (`Probe<HeaderValue>`, by
//! value of `Probe`) matches; then `&&Probe<T>`, where the `Display` fallback is implemented.
//! Stable Rust, no specialization feature — but the choice is made by method lookup on the
//! *concrete* type at the call site, so it silently changes in generic code.

use std::fmt::{self, Display};

use http::{HeaderMap, HeaderName, HeaderValue};

pub struct Probe<'a, T: ?Sized>(pub &'a T);

pub trait NativeRoute {
    fn __header(&self) -> Option<HeaderValue>;
}

impl NativeRoute for Probe<'_, HeaderValue> {
    fn __header(&self) -> Option<HeaderValue> {
        Some(self.0.clone())
    }
}

pub trait DisplayRoute {
    fn __header(&self) -> Option<HeaderValue>;
}

impl<T: Display + ?Sized> DisplayRoute for &Probe<'_, T> {
    fn __header(&self) -> Option<HeaderValue> {
        let mut text = String::new();
        fmt::Write::write_fmt(&mut text, format_args!("{}", self.0)).ok()?;
        let visible = text.bytes().all(|b| b == b'\t' || (0x20..0x7f).contains(&b));
        if visible { HeaderValue::from_str(&text).ok() } else { None }
    }
}

#[derive(Default)]
pub struct Parts {
    pub headers: HeaderMap,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Invalid(pub &'static str);

impl Parts {
    /// Generated code passes the already-converted value.
    pub fn header(&mut self, name: HeaderName, what: &'static str, value: Option<HeaderValue>, sensitive: bool) -> Result<(), Invalid> {
        let mut value = value.ok_or(Invalid(what))?;
        if sensitive {
            value.set_sensitive(true);
        }
        self.headers.try_append(name, value).map_err(|_| Invalid(what))?;
        Ok(())
    }
}
