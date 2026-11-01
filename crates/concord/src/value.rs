//! The closed set of conversions accepted wherever a declared value is supplied.

mod sealed {
    pub trait Sealed<T> {}
    impl<T> Sealed<T> for T {}
    impl Sealed<String> for &str {}
    impl Sealed<String> for &String {}
}

/// A value accepted for a declared input of type `T`.
///
/// Implemented exactly for:
///
/// - `T` itself (so integer and float literals infer, e.g. `set_limit(7)` for a `u64`),
/// - `&str` and `&String` when `T` is `String`.
///
/// There are no other conversions: no numeric widening, no `Into`, no `Display` coercion and no
/// wrapping into `Option`. Domain newtypes are constructed explicitly. The trait is sealed.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not accepted for a value of type `{T}`",
    label = "expected `{T}`",
    note = "Concord accepts the exact declared type, or `&str`/`&String` for `String`; construct newtypes and `Option`s explicitly"
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
