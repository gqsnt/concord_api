//! The check of an endpoint's declared type against the client's problem type.
//!
//! The generated code writes, for each endpoint, a function that is never called, in a module of
//! its own where none of the author's names is in scope:
//! `(&Output::<Declared>::new()).must_name_the_clients_problem_type::<ClientProblem>()`. A declared type that implements
//! [`CarriesProblem`] takes the first path, whose only bound is the equality of its `Problem`
//! with the client's problem type. Every other type takes the ordinary path, which checks
//! nothing. The macro names no type: the declared type selects the path.

use crate::error::CarriesProblem;
use std::marker::PhantomData;

/// The declared type of an endpoint, as a value with no data, for the check.
pub struct Output<T: ?Sized>(PhantomData<fn() -> Box<T>>);

impl<T: ?Sized> Output<T> {
    /// The witness of `T`.
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T: ?Sized> Default for Output<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: ?Sized> std::fmt::Debug for Output<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Output")
    }
}

/// The path of a type that carries a problem: its `Problem` must be the client's.
pub trait TypedStream {
    /// The declared type.
    type Carrier: CarriesProblem + ?Sized;

    /// Requires the declared type's problem to be `P`, the problem type of the client. Its
    /// name is what `rustc` prints under a mismatch: `required by a bound in
    /// TypedStream::must_name_the_clients_problem_type`.
    fn must_name_the_clients_problem_type<P>(&self)
    where
        Self::Carrier: CarriesProblem<Problem = P>,
    {
    }
}

impl<T: CarriesProblem + ?Sized> TypedStream for Output<T> {
    type Carrier = T;
}

/// The path of every other type: nothing to check. Reached through one more reference, so
/// that a type that carries a problem takes the other path.
pub trait OrdinaryOutput {
    /// Checks nothing.
    fn must_name_the_clients_problem_type<P>(&self) {}
}

impl<T: ?Sized> OrdinaryOutput for &Output<T> {}

#[cfg(test)]
mod tests {
    use super::*;

    struct Carrying<P>(PhantomData<P>);
    impl<P> CarriesProblem for Carrying<P> {
        type Problem = P;
    }

    // The reference is the one that generated code writes: it selects the path.
    #[allow(clippy::needless_borrow)]
    #[test]
    fn an_ordinary_type_and_a_type_that_carries_the_client_problem_pass() {
        (&Output::<u32>::new()).must_name_the_clients_problem_type::<String>();
        (&Output::<Carrying<String>>::new()).must_name_the_clients_problem_type::<String>();
        (&Output::<Option<Carrying<String>>>::new()).must_name_the_clients_problem_type::<String>();
        (&Output::<Result<Box<Carrying<String>>, u8>>::new())
            .must_name_the_clients_problem_type::<String>();
    }
}
