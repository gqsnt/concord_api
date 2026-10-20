//! Closed ordinary-value conversion used by generated payload setters.
//!
//! The complete conversion set is identity `T -> T`, `&str -> String` and
//! `&String -> String`. There is deliberately no numeric widening, no
//! `Display` fallback, no parsing, no array/slice conversion, no recursive
//! optional conversion and no registration hook: an unrelated field's type
//! inference can never change because an application adds an implementation.
//! Domain newtypes are constructed explicitly by the caller.
//!
//! The trait is sealed because every implementation is library-owned. It is
//! unrelated to the open [`RequestControls`](crate::contracts::RequestControls)
//! caller-composition trait.

mod conversion_seal {
    #[diagnostic::on_unimplemented(
        message = "[C3-PLACE] `IntoValue` is sealed: `{Self}` into `{T}` is not a library-owned conversion",
        label = "downstream crates cannot register ordinary-value conversions",
        note = "the complete conversion set is identity, `&str -> String` and `&String -> String`; a registration could change inference for unrelated fields",
        note = "construct the exact `{T}` value explicitly at the call site instead"
    )]
    pub trait Allowed<T> {}
    impl<T> Allowed<T> for T {}
    impl Allowed<String> for &str {}
    impl Allowed<String> for &String {}
}

/// Converts a caller argument into an exact ordinary payload of type `T`.
///
/// Implemented only for identity, `&str -> String` and `&String -> String`.
///
/// ```
/// use concord_core::contracts::IntoValue;
///
/// fn set<V: IntoValue<String>>(value: V) -> String { value.into_value() }
/// assert_eq!(set("literal"), "literal");
/// assert_eq!(set(&String::from("borrowed")), "borrowed");
/// assert_eq!(set(String::from("owned")), "owned");
/// ```
///
/// ```compile_fail
/// use concord_core::contracts::IntoValue;
/// fn widen<V: IntoValue<u64>>(value: V) -> u64 { value.into_value() }
/// let narrow: u32 = 7;
/// widen(narrow); // no numeric widening
/// ```
///
/// # Extension contract
///
/// * **Ownership.** `into_value` consumes the argument; a borrowed string is
///   copied into a new owned `String`.
/// * **Concurrency.** The conversion is a pure function of its argument with
///   no shared state.
/// * **Cancellation.** The conversion is synchronous and runs inside the
///   setter or constructor call, before request execution.
/// * **Input sensitivity.** Values may be sensitive; the conversion neither
///   formats nor retains them.
/// * **Errors.** The conversion cannot fail; rejected conversions are compile
///   errors.
/// * **Resource obligations.** At most one allocation, for the borrowed
///   string copy; there is no hidden parsing or formatting.
/// * **Negative capabilities.** The trait is sealed: applications cannot add
///   conversions, so adding a type can never change inference for an
///   unrelated field, and no numeric widening, `Display` fallback or parsing
///   exists.
#[diagnostic::on_unimplemented(
    message = "[C3-PLACE] `{Self}` does not convert into the declared input type `{T}`",
    label = "expected `{T}` or a closed conversion into it",
    note = "the complete conversion set is identity, `&str -> String` and `&String -> String`; there is no numeric widening, `Display` fallback, parsing or user registration",
    note = "construct the exact `{T}` value explicitly: call a domain newtype's constructor, or convert numbers with `From`/`TryFrom`"
)]
pub trait IntoValue<T>: conversion_seal::Allowed<T> {
    /// Performs the closed conversion.
    fn into_value(self) -> T;
}

impl<T> IntoValue<T> for T {
    fn into_value(self) -> T {
        self
    }
}

impl IntoValue<String> for &str {
    fn into_value(self) -> String {
        self.to_owned()
    }
}

impl IntoValue<String> for &String {
    fn into_value(self) -> String {
        self.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::IntoValue;

    fn accept<T, V: IntoValue<T>>(value: V) -> T {
        value.into_value()
    }

    #[test]
    fn closed_conversions() {
        let unsuffixed: u32 = accept(7);
        assert_eq!(unsuffixed, 7);
        let nested: Option<Option<u32>> = accept(Some(None));
        assert_eq!(nested, Some(None));
        let empty: Vec<String> = accept(Vec::new());
        assert!(empty.is_empty());
        let borrowed = String::from("x");
        let converted: String = accept(&borrowed);
        assert_eq!(converted, "x");
        let literal: String = accept("y");
        assert_eq!(literal, "y");
    }
}
