//! The types that a generated method accepts as an argument.

use crate::secret::Secret;

// rustdoc reconstructs cross-crate blanket impls without their doc(hidden)
// attribute. A hidden definition suppresses those impls; the inline re-export
// publishes the trait's own documentation at arg::Arg. The facade must also
// explicitly inline this trait: a glob re-export skips its hidden definition.
// Check other type pages too: their reconstructed blanket impls ignore the impl's own hidden attribute.
mod contract {
    use super::sealed;

    /// An argument for an input of type `T`: the type itself, a reference to it (cloned inside),
    /// text for a `String` or a `Secret`, a slice or an array for a `Vec`. For an input declared
    /// `count?: T`: any of these, or an `Option<T>` itself, so `.count(None)` clears it.
    /// For `milestone?: Option<T>`, a bare `T` or `Some(T)` sets the number,
    /// `None::<T>` sets a present null, and `None::<Option<T>>` clears it.
    /// A bare `None` is ambiguous for that nested optional input.
    /// Conversions lift through at most two `Option` levels. Rust's coherence
    /// rules reject recursive lifting alongside the exact-value conversion.
    /// Deeper optional setters accept their exact declared type (or its reference)
    /// and the outer `Option` that clears the input:
    ///
    /// ```
    /// # use concord_core as concord;
    /// use concord::arg::Arg;
    /// // The setter signature generated for `n?: Option<Option<u64>>`.
    /// fn n(value: impl Arg<Option<Option<Option<u64>>>>) -> Option<Option<Option<u64>>> {
    ///     value.into_input()
    /// }
    /// let exact: Option<Option<u64>> = Some(Some(5));
    /// assert_eq!(n(exact), Some(exact));
    /// assert_eq!(n(&exact), Some(exact));
    /// assert_eq!(n(None::<Option<u64>>), Some(None));
    /// assert_eq!(n(None::<Option<Option<u64>>>), None);
    /// ```
    ///
    /// Closed: only Concord implements it, so a type from outside cannot add a conversion.
    #[doc(hidden)]
    pub trait Arg<T>: sealed::Sealed<T> {
        /// The value of the input.
        fn into_input(self) -> T;
    }
}

#[doc(inline)]
pub use contract::Arg;

mod sealed {
    pub trait Sealed<T> {}
}

macro_rules! arg {
    ($(impl[$($g:tt)*] $target:ty => for $from:ty, |$v:ident| $body:expr;)*) => {$(
        impl<$($g)*> sealed::Sealed<$target> for $from {}
        #[doc(hidden)]
        impl<$($g)*> Arg<$target> for $from {
            fn into_input(self) -> $target { let $v = self; $body }
        }
    )*};
}

arg! {
    impl[T] T => for T, |v| v;
    impl[T: Clone] T => for &T, |v| v.clone();
    impl[] String => for &str, |v| v.to_owned();
    impl[] Secret => for &str, |v| Secret::new(v);
    impl[] Secret => for String, |v| Secret::new(v);
    impl[T] Option<T> => for T, |v| Some(v);
    impl[T] Option<Option<T>> => for T, |v| Some(Some(v));
    impl[T: Clone] Option<Option<T>> => for &T, |v| Some(Some(v.clone()));
    impl[] Option<Option<String>> => for &str, |v| Some(Some(v.to_owned()));
    impl[] Option<Option<Secret>> => for &str, |v| Some(Some(Secret::new(v)));
    impl[] Option<Option<Secret>> => for String, |v| Some(Some(Secret::new(v)));
    impl[T: Clone] Option<T> => for &T, |v| Some(v.clone());
    impl[] Option<String> => for &str, |v| Some(v.to_owned());
    impl[] Option<Secret> => for &str, |v| Some(Secret::new(v));
    impl[] Option<Secret> => for String, |v| Some(Secret::new(v));
    impl[T: Clone] Vec<T> => for &[T], |v| v.to_vec();
    impl[T, const N: usize] Vec<T> => for [T; N], |v| v.into();
    impl[] Vec<String> => for &[&str], |v| v.iter().map(|s| (*s).to_owned()).collect();
    impl[const N: usize] Vec<String> => for [&str; N], |v| v.iter().map(|s| (*s).to_owned()).collect();
    impl[] Option<Vec<String>> => for &[&str], |v| Some(v.iter().map(|s| (*s).to_owned()).collect());
    impl[const N: usize] Option<Vec<String>> => for [&str; N], |v| Some(v.iter().map(|s| (*s).to_owned()).collect());
    impl[T: Clone] Option<Option<Vec<T>>> => for &[T], |v| Some(Some(v.to_vec()));
    impl[T, const N: usize] Option<Option<Vec<T>>> => for [T; N], |v| Some(Some(v.into()));
    impl[] Option<Option<Vec<String>>> => for &[&str], |v| Some(Some(v.iter().map(|s| (*s).to_owned()).collect()));
    impl[const N: usize] Option<Option<Vec<String>>> => for [&str; N], |v| Some(Some(v.iter().map(|s| (*s).to_owned()).collect()));
    impl[T: Clone] Option<Vec<T>> => for &[T], |v| Some(v.to_vec());
    impl[T, const N: usize] Option<Vec<T>> => for [T; N], |v| Some(v.into());
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    // The references are intentional: this test covers borrowed input conversions.
    #[allow(clippy::needless_borrows_for_generic_args)]
    fn nested_optional_inputs_inherit_conversions() {
        fn number(v: impl Arg<Option<Option<u64>>>) -> Option<Option<u64>> {
            v.into_input()
        }
        fn text(v: impl Arg<Option<Option<String>>>) -> Option<Option<String>> {
            v.into_input()
        }
        assert_eq!(number(&5u64), Some(Some(5)));
        assert_eq!(number(Some(5)), Some(Some(5)));
        assert_eq!(number(None::<u64>), Some(None));
        assert_eq!(number(None::<Option<u64>>), None);
        assert_eq!(text("hello"), Some(Some("hello".to_owned())));
        assert_eq!(text(&"hello".to_owned()), Some(Some("hello".to_owned())));
        fn list(v: impl Arg<Option<Option<Vec<String>>>>) -> Option<Option<Vec<String>>> {
            v.into_input()
        }
        assert_eq!(list(["a", "b"]), Some(Some(vec!["a".into(), "b".into()])));
        assert_eq!(
            list(&["a", "b"][..]),
            Some(Some(vec!["a".into(), "b".into()]))
        );
        fn secret(v: impl Arg<Option<Option<Secret>>>) -> Option<Option<Secret>> {
            v.into_input()
        }
        assert_eq!(&*secret("key").unwrap().unwrap().expose(), "key");
        assert_eq!(
            &*secret(String::from("key")).unwrap().unwrap().expose(),
            "key"
        );
    }
}
