//! The closed set of conversions accepted wherever a declared value is supplied.

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
