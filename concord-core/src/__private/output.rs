//! The check of an endpoint's declared type against the client's problem type.
//!
//! The generated code runs the same check for every endpoint, whatever its type:
//! `(&Output::<Declared>::new()).problem::<ClientProblem>()`. A type whose value holds a
//! client's error says so by implementing [`CarriesProblem`]; the check then requires its
//! problem type to be the client's. Every other type takes the ordinary path, which checks
//! nothing. The macro names no type: the declared type selects the path.

use std::marker::PhantomData;

/// A type whose value holds the error of a client, and so the problem type of one client.
///
/// `concord::read::RecordStream<T, concord::Error<P>>` implements it, with `P`.
pub trait CarriesProblem {
    /// The problem type that the value's error names.
    type Problem;
}

/// Holds when `Self`, the problem type of a declared value, is `P`, the problem type of the
/// client.
#[diagnostic::on_unimplemented(
    message = "the endpoint's type names the problem type `{Self}`, and the client's problem type is `{P}`",
    label = "this type's error names `{Self}`",
    note = "a typed stream fails with the client's own error: write the client's `Error` (`RecordStream<Event, client::Error>`), and give the mapping the client's problem type (`RecordFail<{P}>`)"
)]
pub trait SameProblem<P> {}

impl<P> SameProblem<P> for P {}

/// The declared type of an endpoint, as a value with no data, for the check.
pub struct Output<T>(PhantomData<fn() -> T>);

impl<T> Output<T> {
    /// The witness of `T`.
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T> Default for Output<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> std::fmt::Debug for Output<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Output")
    }
}

/// The path of a type that holds a client's error: its problem type must be the client's.
pub trait CarriedProblem {
    /// Checks the problem type `P` of the client.
    fn problem<P>(&self)
    where
        Self: ProblemIs<P>,
    {
    }
}

impl<T: CarriesProblem> CarriedProblem for Output<T> {}

/// Holds when the problem type that a declared type carries is `P`.
pub trait ProblemIs<P> {}

impl<T: CarriesProblem, P> ProblemIs<P> for Output<T> where T::Problem: SameProblem<P> {}

/// The path of every other type: nothing to check. Reached through one more reference, so
/// that a type that carries a problem takes the other path.
pub trait OrdinaryOutput {
    /// Checks nothing.
    fn problem<P>(&self) {}
}

impl<T> OrdinaryOutput for &Output<T> {}

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
        (&Output::<u32>::new()).problem::<String>();
        (&Output::<Carrying<String>>::new()).problem::<String>();
    }
}
