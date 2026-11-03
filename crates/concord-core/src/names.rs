//! The result of a generated client's `validate_header_names()`.

use std::error::Error as StdError;
use std::fmt;

/// The header-name problems a generated client's `validate_header_names()` found, one per
/// endpoint at most, in declaration order.
///
/// `validate_header_names()` checks, without building a client, a request or any value, the
/// rules that depend only on which header each contribution names: on every endpoint compiled
/// under the current `cfg` whose header chain writes a native `HeaderName` constant (chains with
/// literal names only were already checked when the declaration compiled). Those rules are
/// exactly the ones `prepare()` applies before converting any value — a header introduced twice,
/// an `override`/`else inherit`/`remove` with nothing to act on, a header field reqwest manages,
/// a `content-type` header next to a declared body media type — and their verdict is the same
/// whatever optional values are present. A problem here is the error that endpoint's `prepare()`
/// returns for every request; `Ok(())` means no `prepare()` fails on header **names**. Values,
/// their conversion, credentials and what servers do are not checked.
///
/// `Display` lists the problems, one per line; it shows declarations and endpoint names, never
/// values. `Debug` (what `unwrap()` prints) shows the same lines as a list.
pub struct HeaderNameErrors {
    pub(crate) problems: Vec<HeaderNameError>,
}

impl HeaderNameErrors {
    /// The problems, in declaration order.
    pub fn problems(&self) -> &[HeaderNameError] {
        &self.problems
    }
}

impl fmt::Display for HeaderNameErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, problem) in self.problems.iter().enumerate() {
            if i > 0 {
                f.write_str("\n")?;
            }
            write!(f, "{problem}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for HeaderNameErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        /// A problem rendered by its `Display`, inside the list.
        struct Line<'a>(&'a HeaderNameError);
        impl fmt::Debug for Line<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(self.0, f)
            }
        }
        f.debug_list()
            .entries(self.problems.iter().map(Line))
            .finish()
    }
}

impl StdError for HeaderNameErrors {}

/// One endpoint's header-name problem (see [`HeaderNameErrors`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeaderNameError {
    pub(crate) endpoint: &'static str,
    pub(crate) declaration: &'static str,
    pub(crate) earlier: Option<&'static str>,
    pub(crate) reason: &'static str,
}

impl HeaderNameError {
    /// The endpoint, such as `Riot.platform.summoner`.
    pub fn endpoint(&self) -> &'static str {
        self.endpoint
    }

    /// The declaration that breaks the rule, such as `` `override X_KEY` on `Api.admin` ``.
    pub fn declaration(&self) -> &'static str {
        self.declaration
    }

    /// For a header introduced twice, the earlier declaration that sets it.
    pub fn earlier(&self) -> Option<&'static str> {
        self.earlier
    }

    /// Which rule, in words.
    pub fn reason(&self) -> &'static str {
        self.reason
    }
}

impl fmt::Display for HeaderNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {} {}", self.endpoint, self.declaration, self.reason)?;
        if let Some(earlier) = self.earlier {
            write!(f, " (earlier: {earlier})")?;
        }
        Ok(())
    }
}
