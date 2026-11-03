//! How a value becomes a header value.

use std::borrow::Cow;
use std::rc::Rc;
use std::sync::Arc;

use reqwest::header::HeaderValue;

mod sealed {
    /// Closes [`ToHeaderValue`](super::ToHeaderValue) to the types listed on it.
    pub trait Sealed {}
}

/// The types Concord sends as a whole header value (`header(NAME) x: T`, `NAME = x`,
/// `NAME = each xs`). The set is closed:
///
/// - [`HeaderValue`](reqwest::header::HeaderValue) itself: the value is cloned (a reference-count
///   increment, or nothing for `HeaderValue::from_static`) and sent byte for byte — including
///   bytes that are not text — with its sensitivity flag;
/// - text, `str` and `String`, through `HeaderValue::from_str`: whatever `http` accepts is sent
///   (including UTF-8 beyond ASCII), anything else is a build error naming the header;
/// - the integers `u8`–`u64`, `usize`, `i8`–`i64`, `isize` (decimal) and `bool`
///   (`true`/`false`);
/// - `&T`, `Box<T>`, `Rc<T>`, `Arc<T>` and `Cow<T>` of any of these.
///
/// Header values are not formatted with `Display`. To send your own type's `Display` text, use a
/// template (`headers { X_REGION = "{region}"; }`); to choose exact bytes, build a `HeaderValue`.
/// The only value that can carry a native sensitivity flag is a `HeaderValue`, which is never
/// written as text, so a value's text placements and its header placements are classified alike.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be sent as a header value",
    label = "not a header value",
    note = "header values are a native `HeaderValue`, text (`String`, `&str`), an integer or a `bool`; send another type's `Display` text with a template, `headers {{ NAME = \"{{value}}\"; }}`, or build a `HeaderValue`",
    note = "a value that may be absent is declared `x?: T`; a list is placed with `each`"
)]
pub trait ToHeaderValue: sealed::Sealed {
    /// The header value, or `None` when this value is not a valid one (the request is then a
    /// build error naming the header). Concord adds its own sensitivity classification to the
    /// returned value; it never clears a flag the value already carries.
    fn to_header_value(&self) -> Option<HeaderValue>;
}

impl sealed::Sealed for HeaderValue {}
impl ToHeaderValue for HeaderValue {
    #[inline]
    fn to_header_value(&self) -> Option<HeaderValue> {
        Some(self.clone())
    }
}

impl sealed::Sealed for str {}
impl ToHeaderValue for str {
    #[inline]
    fn to_header_value(&self) -> Option<HeaderValue> {
        HeaderValue::from_str(self).ok()
    }
}

impl sealed::Sealed for String {}
impl ToHeaderValue for String {
    #[inline]
    fn to_header_value(&self) -> Option<HeaderValue> {
        self.as_str().to_header_value()
    }
}

macro_rules! integers {
    ($($t:ty => $via:ty),*) => {$(
        impl sealed::Sealed for $t {}
        impl ToHeaderValue for $t {
            #[inline]
            fn to_header_value(&self) -> Option<HeaderValue> {
                Some(HeaderValue::from(<$via>::from(*self)))
            }
        }
    )*};
}

integers!(
    u8 => u16, u16 => u16, u32 => u32, u64 => u64, usize => usize,
    i8 => i16, i16 => i16, i32 => i32, i64 => i64, isize => isize
);

impl sealed::Sealed for bool {}
impl ToHeaderValue for bool {
    #[inline]
    fn to_header_value(&self) -> Option<HeaderValue> {
        Some(HeaderValue::from_static(if *self {
            "true"
        } else {
            "false"
        }))
    }
}

macro_rules! forward {
    ($($ptr:ty),*) => {$(
        impl<T: ToHeaderValue + ?Sized> sealed::Sealed for $ptr {}
        impl<T: ToHeaderValue + ?Sized> ToHeaderValue for $ptr {
            #[inline]
            fn to_header_value(&self) -> Option<HeaderValue> {
                (**self).to_header_value()
            }
        }
    )*};
}

forward!(&T, Box<T>, Rc<T>, Arc<T>);

impl<T: ToHeaderValue + ToOwned + ?Sized> sealed::Sealed for Cow<'_, T> {}
impl<T: ToHeaderValue + ToOwned + ?Sized> ToHeaderValue for Cow<'_, T> {
    #[inline]
    fn to_header_value(&self) -> Option<HeaderValue> {
        (**self).to_header_value()
    }
}
