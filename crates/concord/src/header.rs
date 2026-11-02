//! How a value becomes a header value.

use std::borrow::Cow;
use std::rc::Rc;
use std::sync::Arc;

use reqwest::header::HeaderValue;

/// A value Concord can send as a header value (`header("name") x: T`, `"name" = x`,
/// `"name" = each xs`).
///
/// Implemented for:
///
/// - [`HeaderValue`] itself: the value is cloned (a reference-count increment, or nothing for
///   `HeaderValue::from_static`) and sent byte for byte — including bytes that are not text — with
///   its sensitivity flag;
/// - text, `str` and `String`: it must be visible ASCII, space or tab, otherwise the request is a
///   build error naming the header;
/// - the integers `u8`–`u64`, `usize`, `i8`–`i64`, `isize` (decimal) and `bool`
///   (`true`/`false`);
/// - `&T`, `Box<T>`, `Rc<T>`, `Arc<T>` and `Cow<T>` of any of these.
///
/// Implement it for your own type to choose its header spelling:
///
/// ```
/// use concord::ToHeaderValue;
/// use concord::reqwest::header::HeaderValue;
///
/// #[derive(Clone, Copy)]
/// pub enum Detail { Short, Full }
///
/// impl ToHeaderValue for Detail {
///     fn to_header_value(&self) -> Option<HeaderValue> {
///         Some(HeaderValue::from_static(match self {
///             Detail::Short => "short",
///             Detail::Full => "full",
///         }))
///     }
/// }
/// ```
///
/// Header values are not formatted with `Display`. To send a type's `Display` text, use a template
/// (`headers { "x-region" = "{region}"; }`), which is text and follows the text rule above.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be sent as a header value",
    label = "not a header value",
    note = "header values are a native `HeaderValue`, text (`String`, `&str`), an integer or a `bool`; implement `concord::ToHeaderValue` for your own type, or send its `Display` text with a template: `headers {{ \"name\" = \"{{value}}\"; }}`",
    note = "a value that may be absent is declared `x?: T`; a list is placed with `each`"
)]
pub trait ToHeaderValue {
    /// The header value, or `None` when this value is not a valid one (the request is then a
    /// build error naming the header). Concord adds its own sensitivity classification to the
    /// returned value; it never clears a flag the value already carries.
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
        // `HeaderValue::from_str` also accepts bytes >= 0x80 (obs-text); text must be ASCII.
        let visible = self
            .bytes()
            .all(|b| b == b'\t' || (0x20..0x7f).contains(&b));
        if visible {
            HeaderValue::from_str(self).ok()
        } else {
            None
        }
    }
}

impl ToHeaderValue for String {
    #[inline]
    fn to_header_value(&self) -> Option<HeaderValue> {
        self.as_str().to_header_value()
    }
}

macro_rules! integers {
    ($($t:ty => $via:ty),*) => {$(
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
