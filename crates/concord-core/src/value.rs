//! The closed set of conversions accepted wherever a declared value is supplied.

use std::fmt;

use reqwest::header::HeaderValue;

mod sealed {
    use reqwest::header::HeaderValue;

    pub trait Sealed<T> {}
    impl<T> Sealed<T> for T {}
    impl Sealed<String> for &str {}
    impl Sealed<String> for &String {}
    impl Sealed<HeaderValue> for &HeaderValue {}
}

/// A value accepted for a declared input of type `T`.
///
/// Implemented exactly for:
///
/// - `T` itself (so integer and float literals infer, e.g. `set_limit(7)` for a `u64`),
/// - `&str` and `&String` when `T` is `String`,
/// - `&HeaderValue` when `T` is `HeaderValue` (cloned: a `static` built with
///   `HeaderValue::from_static` is passed as `&ACCEPT`).
///
/// There are no other conversions: no numeric widening, no `Into`, no `Display` coercion and no
/// wrapping into `Option`. Domain newtypes are constructed explicitly. The trait is sealed.
///
/// It applies to positional arguments, `set_*` methods and initializers. The fields of a
/// generated `{Client}Config` struct take exactly `T` (a string literal needs `.into()`), and
/// `maybe_*` takes exactly `Option<T>`.
///
/// Because such a parameter is generic, an expression that takes its type from its target
/// (`"x".into()`, `s.parse().unwrap()`, `Default::default()`) has no target to take it from:
/// rustc reports `E0283` ("type annotations needed"). Pass the value itself (`"x"`, `7`) or name
/// the type (`s.parse::<u64>()`, `u64::default()`):
///
/// ```compile_fail,E0283
/// fn set_limit(value: impl concord_core::IntoValue<u64>) -> u64 {
///     concord_core::IntoValue::into_value(value)
/// }
/// set_limit("7".parse().unwrap());
/// ```
///
/// ```
/// # fn set_limit(value: impl concord_core::IntoValue<u64>) -> u64 {
/// #     concord_core::IntoValue::into_value(value)
/// # }
/// assert_eq!(set_limit("7".parse::<u64>().unwrap()), 7);
/// assert_eq!(set_limit(u64::default()), 0);
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not accepted for a value of type `{T}`",
    label = "expected `{T}`",
    note = "Concord accepts the exact declared type, `&str`/`&String` for `String`, or `&HeaderValue` for `HeaderValue`; construct newtypes and `Option`s explicitly"
)]
pub trait IntoValue<T>: sealed::Sealed<T> {
    /// Performs the conversion.
    fn into_value(self) -> T;
}

impl<T> IntoValue<T> for T {
    #[inline]
    fn into_value(self) -> T {
        self
    }
}

impl IntoValue<String> for &str {
    #[inline]
    fn into_value(self) -> String {
        self.to_owned()
    }
}

impl IntoValue<String> for &String {
    #[inline]
    fn into_value(self) -> String {
        self.clone()
    }
}

impl IntoValue<HeaderValue> for &HeaderValue {
    #[inline]
    fn into_value(self) -> HeaderValue {
        self.clone()
    }
}

/// A value that writes its own query pairs: the parameter of an endpoint declared with
/// `query(..) name?: T`. Use it when the key itself depends on the value — one of several
/// mutually exclusive cursors (`around`/`before`/`after`), provider parameters named at run time
/// (`param_<name>`):
///
/// ```
/// use concord_core::{QueryPairs, QueryPairsWriter};
///
/// /// Exactly one of three cursors.
/// pub enum Anchor { Before(u64), After(u64), Around(u64) }
///
/// impl QueryPairs for Anchor {
///     fn query_pairs(&self, out: &mut QueryPairsWriter<'_>) {
///         match self {
///             Anchor::Before(id) => out.pair("before", id),
///             Anchor::After(id) => out.pair("after", id),
///             Anchor::Around(id) => out.pair("around", id),
///         }
///     }
/// }
/// ```
///
/// The pairs are written where the value is declared among the endpoint's parameters (after every
/// key the chain already has, with all its occurrences; before keys first introduced by the
/// endpoint's `query { }` block), in the order written. An empty key is a
/// build error. Concord cannot redact a key it only learns at run time, so such a parameter
/// cannot be `#[sensitive]`, and its values appear in the request's `Debug` URL — except under a
/// key on the well-known secret list (`token`, `api_key`, ...), which is withheld as a declared
/// key would be.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be a `query(..)` value: it does not implement `QueryPairs`",
    label = "a `query(..)` value writes its own query pairs",
    note = "implement `concord::QueryPairs` for the type, or give the value a key: `query(\"k\") x: T`"
)]
pub trait QueryPairs {
    /// Writes the pairs, in order.
    fn query_pairs(&self, out: &mut QueryPairsWriter<'_>);
}

impl<T: QueryPairs + ?Sized> QueryPairs for &T {
    fn query_pairs(&self, out: &mut QueryPairsWriter<'_>) {
        (**self).query_pairs(out);
    }
}

impl<K: AsRef<str>, V: fmt::Display> QueryPairs for [(K, V)] {
    fn query_pairs(&self, out: &mut QueryPairsWriter<'_>) {
        for (key, value) in self {
            out.pair(key.as_ref(), value);
        }
    }
}

impl<K: AsRef<str>, V: fmt::Display> QueryPairs for Vec<(K, V)> {
    fn query_pairs(&self, out: &mut QueryPairsWriter<'_>) {
        self.as_slice().query_pairs(out);
    }
}

impl<K: AsRef<str>, V: fmt::Display> QueryPairs for std::collections::BTreeMap<K, V> {
    fn query_pairs(&self, out: &mut QueryPairsWriter<'_>) {
        for (key, value) in self {
            out.pair(key.as_ref(), value);
        }
    }
}

/// Where a [`QueryPairs`] value writes its pairs.
pub struct QueryPairsWriter<'a> {
    out: &'a mut Vec<(std::borrow::Cow<'static, str>, String)>,
    failed: Option<&'static str>,
}

impl<'a> QueryPairsWriter<'a> {
    pub(crate) fn new(out: &'a mut Vec<(std::borrow::Cow<'static, str>, String)>) -> Self {
        Self { out, failed: None }
    }

    /// Appends `key=value` (`value` formatted with `Display`, then percent-encoded with the rest
    /// of the query).
    pub fn pair(&mut self, key: &str, value: impl fmt::Display) {
        if self.failed.is_some() {
            return;
        }
        if key.is_empty() {
            self.failed = Some("writes a query pair with an empty key");
            return;
        }
        let mut text = String::new();
        if fmt::Write::write_fmt(&mut text, format_args!("{value}")).is_err() {
            self.failed = Some("could not be formatted (its Display failed)");
            return;
        }
        self.out
            .push((std::borrow::Cow::Owned(key.to_owned()), text));
    }

    pub(crate) fn finish(self) -> Result<(), &'static str> {
        self.failed.map_or(Ok(()), Err)
    }
}
