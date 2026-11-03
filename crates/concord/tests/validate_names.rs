//! `Client::validate_header_names()` (H01–H07): an associated function that checks every active
//! endpoint's header names without a client, a configuration, endpoint values, a network or an
//! async runtime, with exactly the rules and verdicts of `prepare()`.

use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};

use concord::reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HOST, HeaderName};
use concord::{CodecError, Encode, ErrorKind, reqwest};

/// The same header as `AUTHORIZATION`, through another constant.
pub const AUTH: HeaderName = AUTHORIZATION;
/// A vendor header.
pub const X_KEY: HeaderName = HeaderName::from_static("x-key");
/// The same name, declared separately.
pub const X_KEY_AGAIN: HeaderName = HeaderName::from_static("x-key");

/// Everything user code does that validation must not trigger: defaults, `Display`, encoders.
static RAN: AtomicUsize = AtomicUsize::new(0);

/// A value whose `Display` counts its calls.
#[derive(Clone)]
pub struct Loud;

impl fmt::Display for Loud {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        RAN.fetch_add(1, Ordering::SeqCst);
        f.write_str("loud")
    }
}

fn loud_default() -> String {
    RAN.fetch_add(1, Ordering::SeqCst);
    "Bearer default".to_owned()
}

/// A body type that cannot be built at all: validation must not need one.
pub enum Never {}

/// A format whose encoder must never run during validation.
pub struct Panicking;

impl Encode<Never> for Panicking {
    const CONTENT_TYPE: Option<&'static str> = Some("application/x-never");
    fn encode(value: Never) -> Result<reqwest::Body, CodecError> {
        match value {}
    }
}

impl Encode<String> for Panicking {
    const CONTENT_TYPE: Option<&'static str> = Some("text/x-panicking");
    fn encode(value: String) -> Result<reqwest::Body, CodecError> {
        RAN.fetch_add(1, Ordering::SeqCst);
        Ok(value.into())
    }
}

concord::api! {
    /// Several endpoints, several problems; defaults, formatting and encoding all panic.
    pub client Broken(
        header(AUTHORIZATION) token: String = loud_default(),
        key?: Loud,
    ) {
        base "http://validate.invalid";
        headers { X_KEY = "{key}"; }
        request Panicking;

        /// Fine: an alias of the same header is overridden, not introduced twice.
        GET fine(n: Loud) { path "fine/{n}"; headers { override AUTH = "Bearer {n}"; } }
        /// `AUTH` is `authorization`, already set by the client.
        GET twice() { path "twice"; headers { AUTH = "again"; } }
        /// Removing a header nobody sets.
        GET nothing_to_remove() { path "r"; headers { remove ACCEPT; } }
        /// A header reqwest manages, through a native constant.
        GET reserved() { path "h"; headers { HOST = "x"; } }
        /// A declared body media type and a surviving `content-type` header.
        POST media(body b: Never) {
            path "m";
            request Panicking "application/x-declared";
            headers { CONTENT_TYPE = "text/plain"; }
        }
        /// The same name under two constants (equal names, distinct paths).
        GET equal_names() { path "e"; headers { remove X_KEY; X_KEY_AGAIN = "1"; append X_KEY = "2"; } }
        /// Disabled: its key path does not even exist, and it is not checked.
        #[cfg(any())]
        GET gone() { path "g"; headers { NO_SUCH_KEY = "x"; } }
        scope deep(d: Loud) {
            path "d/{d}";
            /// Nested two levels down: `X_KEY_AGAIN` names `x-key`, which the client sets.
            GET twice_too() { path "t"; headers { X_KEY_AGAIN = "x"; } }
        }
    }
}

concord::api! {
    /// Literal names only: checked when compiled; the validator has nothing left to check.
    pub client Literal(header("authorization") token: String) {
        base "http://validate.invalid";
        GET item() { headers { "x-a" = "1"; } }
    }
}

concord::api! {
    /// Native names, all coherent.
    pub client Coherent(header(AUTHORIZATION) token: String) {
        base "http://validate.invalid";
        headers { ACCEPT = "application/json"; }
        GET a() { headers { remove AUTH; X_KEY = "1"; } }
        GET b() { headers { override ACCEPT = "text/plain"; append X_KEY = "1"; } }
    }
}

/// H01–H05 in one test (the counter is shared): validation runs no default, `Display` or encoder,
/// needs no endpoint value (one body type is uninhabited), reports every endpoint's problem once in
/// declaration order, and each problem is the error that endpoint's `prepare()` returns.
#[test]
fn validation_runs_nothing_and_matches_prepare() {
    let errors = Broken::validate_header_names().unwrap_err();
    assert_eq!(RAN.load(Ordering::SeqCst), 0, "validation ran user code");
    let found: Vec<(&str, &str)> = errors
        .problems()
        .iter()
        .map(|p| (p.endpoint(), p.declaration()))
        .collect();
    assert_eq!(
        found,
        [
            ("Broken.twice", "`AUTH` on `Broken.twice`"),
            (
                "Broken.nothing_to_remove",
                "`remove ACCEPT` on `Broken.nothing_to_remove`"
            ),
            ("Broken.reserved", "`HOST` on `Broken.reserved`"),
            ("Broken.media", "`CONTENT_TYPE` on `Broken.media`"),
            (
                "Broken.deep.twice_too",
                "`X_KEY_AGAIN` on `Broken.deep.twice_too`"
            ),
        ]
    );
    assert_eq!(
        errors.problems()[0].earlier(),
        Some("`AUTHORIZATION` on `Broken`")
    );
    assert_eq!(
        errors.problems()[1].reason(),
        "changes a header that no enclosing owner sets"
    );
    // Display: declarations and endpoint names, one problem per line, no values.
    let shown = errors.to_string();
    assert_eq!(shown.lines().count(), 5, "{shown}");
    assert!(
        shown.starts_with("Broken.twice: `AUTH` on `Broken.twice` sets a header that an earlier declaration already sets"),
        "{shown}"
    );
    assert!(!shown.contains("Bearer"), "{shown}");
    // Validating again gives the same answer and still runs nothing.
    assert_eq!(
        Broken::validate_header_names().unwrap_err().problems(),
        errors.problems()
    );
    assert_eq!(RAN.load(Ordering::SeqCst), 0);

    // H04: what `prepare()` reports, endpoint by endpoint (these need values: now they run).
    let api = Broken::new(reqwest::Client::new()).set_key(Loud);
    let prepared = [
        api.twice().prepare().map(drop),
        api.nothing_to_remove().prepare().map(drop),
        api.reserved().prepare().map(drop),
    ];
    for (problem, result) in errors.problems().iter().zip(prepared) {
        let error = result.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Build);
        assert_eq!(error.endpoint(), Some(problem.endpoint()));
        assert!(error.to_string().contains(problem.declaration()), "{error}");
    }
    let error = api.deep(Loud).twice_too().prepare().unwrap_err();
    assert!(
        error
            .to_string()
            .contains(errors.problems()[4].declaration()),
        "{error}"
    );
    // The coherent endpoints prepare, and were not reported.
    drop(api.fine(Loud).prepare().unwrap());
    drop(api.equal_names().prepare().unwrap());
}

#[test]
fn literal_and_coherent_clients_validate() {
    Literal::validate_header_names().unwrap();
    Coherent::validate_header_names().unwrap();
}
