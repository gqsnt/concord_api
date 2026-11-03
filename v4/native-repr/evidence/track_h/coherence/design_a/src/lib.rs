//! Design A — native-conversion-first.
//!
//! A small Concord trait says how a value becomes a header value. It is implemented for the native
//! type (read by reference, cloned: bytes and sensitivity flag preserved), for text (`str`,
//! `String`: visible ASCII only), for integers and `bool`, and forwarded through `&T`, `Box<T>`,
//! `Rc<T>`, `Arc<T>`, `Cow<T>`. Everything else is a compile error with a note naming the two
//! explicit escapes: a template hole (`"{v}"`, formatted with `Display`) or implementing the
//! trait for your own type. There is deliberately no `impl<T: Display>` (see the `neg_*`
//! features: it cannot coexist with the native impls).

use std::borrow::Cow;
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

pub use http::HeaderValue;
use http::{HeaderMap, HeaderName};

/// A value that can be sent as a header value.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be sent as a header value",
    label = "not a header value",
    note = "header values are a native `HeaderValue`, text (`String`, `&str`), an integer or a `bool`; implement `ToHeaderValue` for your own type, or format it with `Display` in a template: `\"name\" = \"{{value}}\"`",
    note = "`Option` and collections are placed with `?` or `each`"
)]
pub trait ToHeaderValue {
    /// The header value; `None` when this value is not a valid one.
    fn to_header_value(&self) -> Option<HeaderValue>;
}

impl ToHeaderValue for HeaderValue {
    #[inline]
    fn to_header_value(&self) -> Option<HeaderValue> {
        Some(self.clone())
    }
}

impl ToHeaderValue for str {
    #[inline]
    fn to_header_value(&self) -> Option<HeaderValue> {
        let visible = self.bytes().all(|b| b == b'\t' || (0x20..0x7f).contains(&b));
        if visible { HeaderValue::from_str(self).ok() } else { None }
    }
}

impl ToHeaderValue for String {
    #[inline]
    fn to_header_value(&self) -> Option<HeaderValue> {
        self.as_str().to_header_value()
    }
}

macro_rules! native_integers {
    ($($t:ty => $via:ty),*) => {$(
        impl ToHeaderValue for $t {
            #[inline]
            fn to_header_value(&self) -> Option<HeaderValue> {
                Some(HeaderValue::from(<$via>::from(*self)))
            }
        }
    )*};
}
native_integers!(u8 => u16, u16 => u16, u32 => u32, u64 => u64, usize => usize,
                 i8 => i16, i16 => i16, i32 => i32, i64 => i64, isize => isize);

impl ToHeaderValue for bool {
    #[inline]
    fn to_header_value(&self) -> Option<HeaderValue> {
        Some(HeaderValue::from_static(if *self { "true" } else { "false" }))
    }
}

macro_rules! forward {
    ($($ptr:ty),*) => {$(
        impl<T: ToHeaderValue + ?Sized> ToHeaderValue for $ptr {
            #[inline]
            fn to_header_value(&self) -> Option<HeaderValue> {
                (**self).to_header_value()
            }
        }
    )*};
}
forward!(&T, Box<T>, Rc<T>, Arc<T>);

impl<T: ToHeaderValue + ToOwned + ?Sized> ToHeaderValue for Cow<'_, T> {
    #[inline]
    fn to_header_value(&self) -> Option<HeaderValue> {
        (**self).to_header_value()
    }
}

// ---- Negative coherence probes -------------------------------------------------------------

/// E0119 expected: `http` may add `impl Display for HeaderValue` in a minor release.
#[cfg(feature = "neg_display_blanket")]
impl<T: fmt::Display + ?Sized> ToHeaderValue for T {
    fn to_header_value(&self) -> Option<HeaderValue> {
        HeaderValue::from_str(&self.to_string()).ok()
    }
}

/// E0119 expected: `str: Display`, so the blanket and `impl for str` overlap outright.
#[cfg(feature = "neg_display_blanket_str")]
pub trait TextFirst {
    fn to_header_value(&self) -> Option<HeaderValue>;
}
#[cfg(feature = "neg_display_blanket_str")]
impl<T: fmt::Display + ?Sized> TextFirst for T {
    fn to_header_value(&self) -> Option<HeaderValue> {
        HeaderValue::from_str(&self.to_string()).ok()
    }
}
#[cfg(feature = "neg_display_blanket_str")]
impl TextFirst for str {
    fn to_header_value(&self) -> Option<HeaderValue> {
        HeaderValue::from_str(self).ok()
    }
}

/// Bridging to `http`'s own conversions with a blanket, next to an explicit `str` impl.
#[cfg(feature = "neg_from_blanket")]
pub trait Bridge {
    fn to_header_value(&self) -> Option<HeaderValue>;
}
#[cfg(feature = "neg_from_blanket")]
impl<T: ?Sized> Bridge for T
where
    for<'a> HeaderValue: From<&'a T>,
{
    fn to_header_value(&self) -> Option<HeaderValue> {
        Some(HeaderValue::from(self))
    }
}
#[cfg(feature = "neg_from_blanket")]
impl Bridge for str {
    fn to_header_value(&self) -> Option<HeaderValue> {
        HeaderValue::from_str(self).ok()
    }
}

// ---- Variant: opt-in formatting for local `Display` types --------------------------------

/// Opt-in marker: "format my `Display` as header text". Coherent with the native impls (no other
/// crate can implement it for `HeaderValue`/`str`), but NOT with the `&T`/`Box<T>` forwarding impls:
/// a downstream crate may implement it for `&Local`/`Box<Local>` (fundamental types).
#[cfg(feature = "neg_marker")]
pub trait HeaderText: fmt::Display {}

#[cfg(feature = "neg_marker")]
impl<T: HeaderText + ?Sized> ToHeaderValue for T {
    fn to_header_value(&self) -> Option<HeaderValue> {
        use std::fmt::Write as _;
        let mut text = String::new();
        write!(text, "{self}").ok()?;
        text.as_str().to_header_value()
    }
}

// ---- What generated code calls ---------------------------------------------------------------

/// Mirrors `concord::__private::Parts` (headers only).
#[derive(Default)]
pub struct Parts {
    pub headers: HeaderMap,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Invalid(pub &'static str);

impl Parts {
    /// One header line: outgoing flag = incoming native flag OR the declaration's classification.
    pub fn header<T: ToHeaderValue + ?Sized>(
        &mut self,
        name: HeaderName,
        what: &'static str,
        value: &T,
        sensitive: bool,
    ) -> Result<(), Invalid> {
        let mut value = value.to_header_value().ok_or(Invalid(what))?;
        if sensitive {
            value.set_sensitive(true);
        }
        self.headers.try_append(name, value).map_err(|_| Invalid(what))?;
        Ok(())
    }

    /// One header line per item, in iteration order.
    pub fn header_each<I>(
        &mut self,
        name: HeaderName,
        what: &'static str,
        values: I,
        sensitive: bool,
    ) -> Result<(), Invalid>
    where
        I: IntoIterator,
        I::Item: ToHeaderValue,
    {
        for value in values {
            self.header(name.clone(), what, &value, sensitive)?;
        }
        Ok(())
    }

    /// Mirrors `Parts::query`: query values stay `Display` (for the cross-placement probe).
    pub fn query<T: fmt::Display + ?Sized>(&mut self, value: &T) -> String {
        value.to_string()
    }
}

/// Generated-`Debug` rendering of a header-only value: the header bytes, `<redacted>` when the
/// value carries the native sensitivity flag. No `Display` needed.
pub struct DebugHeader<'a, T: ?Sized>(pub &'a T);

impl<T: ToHeaderValue + ?Sized> fmt::Debug for DebugHeader<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0.to_header_value() {
            None => f.write_str("<invalid header value>"),
            Some(v) if v.is_sensitive() => f.write_str("<redacted>"),
            Some(v) => match v.to_str() {
                Ok(text) => fmt::Debug::fmt(text, f),
                Err(_) => fmt::Debug::fmt(&v, f),
            },
        }
    }
}
