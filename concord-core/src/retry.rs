//! Whether and how to try again.

use crate::__private::Idempotent;
use crate::answer::Headers;
use crate::error::{Class, Kind};
use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;

mod http_date;

type When<P> = Arc<dyn Fn(&Tried<'_, P>) -> Option<Duration> + Send + Sync>;
type Unless<P> = Arc<dyn Fn(&Tried<'_, P>) -> bool + Send + Sync>;

/// A retry policy. Off until a client or a call is given one: `with_retry(Retry::standard())`.
///
/// A request goes out again when three things hold: it can be sent again (its body is not a
/// one-shot stream); it may be repeated (its method, `idempotent` mark, or a listed `not_applied_on` status permits it); and the failure is one that this
/// policy thinks worth another try. A call’s `with_retry` replaces the whole client policy,
/// including its tries and rules; extend the author’s policy explicitly to keep it. An unmarked `POST` is not repeated after a failed connection.
///
/// **The author's policy.** A description holds no policy, and `<client>::Retry` is only this
/// type with the client's problem type. The author of a client who knows the API's own rule
/// ships it as a function beside the macro (`pub fn retry() -> shop::Retry`); the client
/// uses it only when a caller passes it to `with_retry`.
///
/// **What a wait is.** Before try `n + 1`, the wait is the longest of: the wait that a rule of
/// [`when`](Retry::when) gave, the `Retry-After` of the answer, and a random part up to
/// 2<sup>n-1</sup> seconds (1 s, 2 s, 4 s, … at most 60 s), drawn again for each wait,
/// after [`none`](Retry::none) as after [`standard`](Retry::standard). So a rule's wait is
/// exact when it is at least 2<sup>n-1</sup> seconds and no longer `Retry-After` came. A
/// wait over 60 seconds, or one that would end after the call's `timeout`, means no other try.
pub struct Retry<P = ()> {
    tries: u32,
    standard: bool,
    not_applied: Vec<u16>,
    when: Vec<When<P>>,
    unless: Vec<Unless<P>>,
}

impl<P> std::fmt::Debug for Retry<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Retry")
            .field("tries", &self.tries)
            .finish_non_exhaustive()
    }
}

impl<P> Clone for Retry<P> {
    fn clone(&self) -> Retry<P> {
        Retry {
            tries: self.tries,
            standard: self.standard,
            not_applied: self.not_applied.clone(),
            when: self.when.clone(),
            unless: self.unless.clone(),
        }
    }
}

impl<P> Retry<P> {
    /// One try, and no table: never try again. A policy built from it tries again only when
    /// [`tries`](Retry::tries) allows more than one and a rule of [`when`](Retry::when)
    /// gives a wait: `Retry::none().tries(5).when(…)` is "only what my rule says".
    pub fn none() -> Retry<P> {
        Retry {
            tries: 1,
            standard: false,
            not_applied: Vec::new(),
            when: Vec::new(),
            unless: Vec::new(),
        }
    }

    /// At most 3 tries, using this table:
    ///
    /// | Failure | Worth another try |
    /// | --- | --- |
    /// | No connection (`Connect`) | Yes |
    /// | No answer (`Timeout`) | Yes |
    /// | An answer that broke (`Body`) | Yes |
    /// | Status 408, 429, 502, 503, 504 | Yes |
    ///
    /// The wait is random up to 1 second, then 2, then 4, capped at 60 seconds,
    /// and never less than `Retry-After`. The repeatability and body checks still apply.
    ///
    /// A `Retry-After` over 60 seconds is not waited. Nor is a wait that would end after the
    /// call's `timeout`: it is not started, and the call gives the last outcome at once (a
    /// `429` is then the error, with its [`retry_after`](crate::Error::retry_after)), not a
    /// timeout.
    pub fn standard() -> Retry<P> {
        Retry {
            tries: 3,
            standard: true,
            ..Self::none()
        }
    }

    /// Permit repeating a refused request with this status because the API did not apply it.
    /// A body failure or timeout that retains this status does not open this gate.
    /// This does not permit repeating it after a lost connection, and all other
    /// retry checks and rules still apply, even with `idempotent false`.
    /// A number outside the status-code range is retained and matches nothing.
    ///
    /// ```compile_fail
    /// #![deny(unused_must_use)]
    /// # use concord_core as concord;
    /// let retry = concord::Retry::<()>::none();
    /// retry.not_applied_on(503);
    /// ```
    #[must_use]
    pub fn not_applied_on(mut self, status: u16) -> Retry<P> {
        self.not_applied.push(status);
        self
    }

    /// How many tries in all, the first one counted. At least 1.
    ///
    /// ```compile_fail
    /// #![deny(unused_must_use)]
    /// # use concord_core as concord;
    /// let retry = concord::Retry::<()>::none();
    /// retry.tries(5);
    /// ```
    #[must_use]
    pub fn tries(mut self, tries: u32) -> Retry<P> {
        self.tries = tries.max(1);
        self
    }

    /// A rule of your own that adds a case, asked when another try is possible. `Some(wait)`:
    /// try again, after at least this long (the longest of it, a `Retry-After` and the random
    /// part: see [`Retry`]). `None`: ask the next rule; when no rule gives a wait, the
    /// standard table decides on a policy built from [`standard`](Retry::standard), and on
    /// one built from [`none`](Retry::none) there is no other try.
    /// It answers only "is
    /// this failure worth another try": a request that may not be repeated still is not.
    /// To stop a try that the policy would make, see [`unless`](Retry::unless).
    /// Rules add up in insertion order; the first `when` that gives a wait wins.
    ///
    /// ```compile_fail
    /// #![deny(unused_must_use)]
    /// # use concord_core as concord;
    /// let retry = concord::Retry::<()>::none();
    /// retry.when(|_| None);
    /// ```
    #[must_use]
    pub fn when(
        mut self,
        rule: impl Fn(&Tried<'_, P>) -> Option<Duration> + Send + Sync + 'static,
    ) -> Retry<P> {
        self.when.push(Arc::new(rule));
        self
    }

    /// A rule of your own that says no, asked first when another try is possible. `true`: no
    /// other try, whatever the policy and [`when`](Retry::when) say.
    /// Rules add up: every `unless` is asked before any `when`, in insertion order.
    /// Any `true` prevents another try.
    ///
    /// ```
    /// # use concord_core as concord;
    /// use concord::Retry;
    /// struct Problem { code: String }
    /// let retry = Retry::<Problem>::standard()
    ///     .unless(|tried| tried.problem().is_some_and(|p| p.code == "maintenance"));
    /// ```
    ///
    /// ```compile_fail
    /// #![deny(unused_must_use)]
    /// # use concord_core as concord;
    /// let retry = concord::Retry::<()>::none();
    /// retry.unless(|_| false);
    /// ```
    #[must_use]
    pub fn unless(
        mut self,
        rule: impl Fn(&Tried<'_, P>) -> bool + Send + Sync + 'static,
    ) -> Retry<P> {
        self.unless.push(Arc::new(rule));
        self
    }
}

/// A try that failed, as a rule of [`Retry::when`] or [`Retry::unless`] sees it.
pub struct Tried<'a, P> {
    number: u32,
    status: Option<http::StatusCode>,
    class: Option<Class>,
    problem: Option<&'a P>,
    headers: Option<&'a Headers>,
}

impl<P> std::fmt::Debug for Tried<'_, P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tried")
            .field("number", &self.number)
            .field("status", &self.status)
            .field("class", &self.class)
            .finish_non_exhaustive()
    }
}

impl<'a, P> Tried<'a, P> {
    /// The number of this try: 1 for the first.
    pub fn number(&self) -> u32 {
        self.number
    }

    /// The status, when an answer arrived.
    pub fn status(&self) -> Option<http::StatusCode> {
        self.status
    }

    /// The headers, when an answer arrived.
    pub fn headers(&self) -> Option<&'a Headers> {
        self.headers
    }

    /// The API's own problem, typed, when the API refused and its body was one.
    pub fn problem(&self) -> Option<&'a P> {
        self.problem
    }

    /// Which failure of the network, when no answer arrived.
    pub fn class(&self) -> Option<Class> {
        self.class
    }
}

/// The evidence needed by the pure retry decision, before any middleware edits.
#[doc(hidden)]
pub struct Outcome<'a, P> {
    /// Error kind.
    pub kind: Kind,
    /// Response status, if any.
    pub status: Option<http::StatusCode>,
    /// Transport failure class.
    pub class: Option<Class>,
    /// Response headers, if any.
    pub headers: Option<&'a Headers>,
    /// Decoded API problem, if any.
    pub problem: Option<&'a P>,
}
impl<P> std::fmt::Debug for Outcome<'_, P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Outcome")
            .field("kind", &self.kind)
            .field("status", &self.status)
            .field("class", &self.class)
            .finish_non_exhaustive()
    }
}

/// One retry verdict and the waits observed while making it. No rule needs re-evaluation.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Decision {
    /// Start another attempt after this wait, or stop.
    pub wait: Option<Duration>,
    /// The valid delay from Retry-After, even if no further try is possible.
    pub header_wait: Option<Duration>,
    /// The first when rule that supplied a wait; absent if none was asked or supplied one.
    pub rule_wait: Option<Duration>,
}
impl Decision {
    /// The longer of the header delay and the evaluated rule's requested delay.
    pub fn retry_after(&self) -> Option<Duration> {
        self.header_wait.max(self.rule_wait)
    }
}

/// Decide whether to start another chain attempt, without I/O or clock reads.
/// `random` is a supplied uniform fraction in 0..=1; `time_left` is None without a deadline.
#[doc(hidden)]
// PANIC-SAFETY: The retry decision receives each independent contract input; no panic lint is relaxed.
#[allow(clippy::too_many_arguments)]
pub fn again<P>(
    policy: &Retry<P>,
    outcome: Outcome<'_, P>,
    n: u32,
    method: &http::Method,
    mark: Idempotent,
    can_copy: bool,
    time_left: Option<Duration>,
    now: SystemTime,
    random: f64,
) -> Decision {
    // 7. Retry-After is either delta seconds or an HTTP date relative to the supplied clock.
    let header_wait = outcome
        .headers
        .and_then(|h| h.as_map().get("retry-after"))
        .and_then(|h| h.to_str().ok())
        .and_then(|text| {
            let text = text.trim();
            if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) {
                Some(Duration::from_secs(text.parse::<u64>().unwrap_or(u64::MAX)))
            } else {
                http_date::parse(text, now).map(|date| date.duration_since(now).unwrap_or_default())
            }
        });
    let mut decision = Decision {
        header_wait,
        ..Decision::default()
    };
    // 1. These outcomes never retry, even under a custom rule.
    if matches!(outcome.kind, Kind::Build | Kind::Mismatch) || outcome.class == Some(Class::Blocked)
    {
        return decision;
    }
    // 2 and 3. Check the attempt budget and replayability before asking rules.
    if n >= policy.tries || !can_copy {
        return decision;
    }
    // 4. A listed not-applied status permits a POST, without permitting connection retries.
    let may_repeat = mark == Idempotent::Yes
        || mark == Idempotent::ByMethod
            && matches!(
                *method,
                http::Method::GET
                    | http::Method::HEAD
                    | http::Method::OPTIONS
                    | http::Method::PUT
                    | http::Method::DELETE
            )
        || outcome.kind == Kind::Refused
            && outcome
                .status
                .is_some_and(|s| policy.not_applied.contains(&s.as_u16()));
    if !may_repeat {
        return decision;
    }
    let tried = Tried {
        number: n,
        status: outcome.status,
        class: outcome.class,
        problem: outcome.problem,
        headers: outcome.headers,
    };
    // 5. Ask every unless, in order, even when one vetoes.
    // PANIC-SAFETY: Every user rule must run; any() would short-circuit; no panic lint is relaxed.
    #[allow(clippy::unnecessary_fold)] // Every rule must run; `any` would short-circuit.
    let veto = policy
        .unless
        .iter()
        .fold(false, |veto, rule| rule(&tried) || veto);
    if veto {
        return decision;
    }
    // 6. First custom wait wins; otherwise consult the standard table.
    let asked = policy.when.iter().find_map(|rule| rule(&tried));
    decision.rule_wait = asked;
    if asked.is_none()
        && !(policy.standard
            && (matches!(
                outcome.class,
                Some(Class::Connect | Class::Timeout | Class::Body)
            ) || outcome
                .status
                .is_some_and(|s| matches!(s.as_u16(), 408 | 429 | 502 | 503 | 504))))
    {
        return decision;
    }
    // 7. Keep both requested waits, even when the verdict is to stop.
    let after = decision.header_wait.unwrap_or_default();
    let asked = decision.rule_wait.unwrap_or_default();
    let cap = Duration::from_secs(60);
    if asked > cap || after > cap {
        return decision;
    }
    let ceiling = (2u64.saturating_pow(n.saturating_sub(1))).min(60);
    let random = if random.is_finite() {
        random.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let wait = asked
        .max(after)
        .max(Duration::from_secs(ceiling).mul_f64(random));
    if time_left.is_some_and(|left| wait >= left) {
        return decision;
    }
    // 8.
    decision.wait = Some(wait);
    decision
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c4_1_not_applied_on_keeps_invalid_numbers_without_panicking() {
        let policy = Retry::<()>::standard()
            .not_applied_on(0)
            .not_applied_on(99)
            .not_applied_on(1000)
            .not_applied_on(u16::MAX);
        assert_eq!(policy.not_applied, [0, 99, 1000, u16::MAX]);
        assert_eq!(
            again(
                &policy,
                status(429),
                1,
                &http::Method::POST,
                Idempotent::ByMethod,
                true,
                None,
                SystemTime::UNIX_EPOCH,
                0.5
            )
            .wait,
            None
        );
        assert!(again(
            &policy.not_applied_on(429),
            status(429),
            1,
            &http::Method::POST,
            Idempotent::ByMethod,
            true,
            None,
            SystemTime::UNIX_EPOCH,
            0.5
        )
        .wait
        .is_some());
    }

    #[test]
    fn c4_7_rules_accumulate_in_order_and_clones_keep_them() {
        use std::sync::Mutex;
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut retry = Retry::<()>::standard();
        for number in [1, 2, 3] {
            let when_events = events.clone();
            let unless_events = events.clone();
            retry = retry
                .when(move |_| {
                    when_events.lock().unwrap().push(number);
                    (number > 1).then_some(Duration::from_secs(number))
                })
                .unless(move |_| {
                    unless_events.lock().unwrap().push(number + 10);
                    number == 2
                });
        }
        let retry = retry.clone();
        assert_eq!((retry.when.len(), retry.unless.len()), (3, 3));
        assert_eq!(decide(&retry, status(503)), None);
        assert_eq!(*events.lock().unwrap(), [11, 12, 13]);
        // No when rule may run after a veto.
    }

    #[test]
    fn c10_7_tried_debug_shows_only_number_status_and_class() {
        struct PrivateProblem;
        let problem = PrivateProblem;
        let tried = Tried {
            number: 2,
            status: Some(http::StatusCode::TOO_MANY_REQUESTS),
            class: None,
            problem: Some(&problem),
            headers: None,
        };
        assert_eq!(
            format!("{tried:?}"),
            "Tried { number: 2, status: Some(429), class: None, .. }"
        );
        let failed = Tried::<String> {
            number: 3,
            status: None,
            class: Some(Class::Timeout),
            problem: Some(&String::from("private-problem")),
            headers: None,
        };
        assert_eq!(
            format!("{failed:?}"),
            "Tried { number: 3, status: None, class: Some(Timeout), .. }"
        );
        assert!(!format!("{failed:#?}").contains("private-problem"));
    }
    fn status(code: u16) -> Outcome<'static, ()> {
        Outcome {
            kind: Kind::Refused,
            status: Some(http::StatusCode::from_u16(code).unwrap()),
            class: None,
            headers: None,
            problem: None,
        }
    }
    fn decide(policy: &Retry<()>, outcome: Outcome<'_, ()>) -> Option<Duration> {
        again(
            policy,
            outcome,
            1,
            &http::Method::GET,
            Idempotent::ByMethod,
            true,
            None,
            SystemTime::UNIX_EPOCH,
            0.5,
        )
        .wait
    }
    #[test]
    fn c4_1_step_one_never_retries_build_mismatch_or_blocked() {
        let policy = Retry::standard().when(|_| panic!("rules must not run"));
        for kind in [Kind::Build, Kind::Mismatch] {
            let mut o = status(503);
            o.kind = kind;
            assert_eq!(decide(&policy, o), None);
        }
        let mut o = status(503);
        o.class = Some(Class::Blocked);
        assert_eq!(decide(&policy, o), None);
    }
    #[test]
    fn c4_1_step_two_tries_one_stops_before_rules() {
        assert_eq!(
            decide(&Retry::standard().tries(1).when(|_| panic!()), status(503)),
            None
        );
        assert_eq!(decide(&Retry::standard().tries(0), status(503)), None);
    }
    #[test]
    fn c4_1_step_three_a_one_shot_body_stops_before_rules() {
        assert_eq!(
            again(
                &Retry::standard().when(|_| panic!()),
                status(503),
                1,
                &http::Method::GET,
                Idempotent::Yes,
                false,
                None,
                SystemTime::UNIX_EPOCH,
                0.5
            )
            .wait,
            None
        );
    }
    #[test]
    fn c4_1_step_four_post_after_503_and_429() {
        let check = |policy: &Retry<()>, code, mark| {
            again(
                policy,
                status(code),
                1,
                &http::Method::POST,
                mark,
                true,
                None,
                SystemTime::UNIX_EPOCH,
                0.5,
            )
            .wait
        };
        assert_eq!(check(&Retry::standard(), 503, Idempotent::ByMethod), None);
        assert_eq!(
            check(&Retry::standard(), 503, Idempotent::Yes),
            Some(Duration::from_millis(500))
        );
        let retry = Retry::standard().not_applied_on(429);
        assert_eq!(
            check(&retry, 429, Idempotent::ByMethod),
            Some(Duration::from_millis(500))
        );
        assert_eq!(
            check(&retry, 429, Idempotent::No),
            Some(Duration::from_millis(500))
        );
        let o = Outcome {
            kind: Kind::Transport,
            status: None,
            class: Some(Class::Connect),
            headers: None,
            problem: None,
        };
        assert_eq!(
            again(
                &retry,
                o,
                1,
                &http::Method::POST,
                Idempotent::ByMethod,
                true,
                None,
                SystemTime::UNIX_EPOCH,
                0.5
            )
            .wait,
            None
        );
    }
    #[test]
    fn c4_7_step_six_two_when_rules_run_until_the_first_wait() {
        use std::sync::Mutex;
        let events = Arc::new(Mutex::new(Vec::new()));
        let first = events.clone();
        let second = events.clone();
        let policy = Retry::standard()
            .when(move |_| {
                first.lock().unwrap().push(1);
                None
            })
            .when(move |_| {
                second.lock().unwrap().push(2);
                Some(Duration::from_secs(2))
            })
            .when(|_| panic!("first wait wins"));
        assert_eq!(decide(&policy, status(400)), Some(Duration::from_secs(2)));
        assert_eq!(*events.lock().unwrap(), [1, 2]);
        events.lock().unwrap().clear();
        let first = events.clone();
        let second = events.clone();
        let policy = Retry::standard()
            .when(move |_| {
                first.lock().unwrap().push(1);
                Some(Duration::from_secs(3))
            })
            .when(move |_| {
                second.lock().unwrap().push(2);
                Some(Duration::from_secs(1))
            })
            .clone();
        assert_eq!(decide(&policy, status(503)), Some(Duration::from_secs(3)));
        assert_eq!(*events.lock().unwrap(), [1]);
    }

    #[test]
    fn c4_1_step_four_method_and_mark_checks_happen_before_any_rules() {
        let policy = Retry::standard().when(|_| panic!("a forbidden repeat must not ask a rule"));
        for method in [http::Method::POST, http::Method::PATCH] {
            assert_eq!(
                again(
                    &policy,
                    status(503),
                    1,
                    &method,
                    Idempotent::ByMethod,
                    true,
                    None,
                    SystemTime::UNIX_EPOCH,
                    0.5
                )
                .wait,
                None
            );
        }
        assert_eq!(
            again(
                &policy,
                status(503),
                1,
                &http::Method::GET,
                Idempotent::No,
                true,
                None,
                SystemTime::UNIX_EPOCH,
                0.5
            )
            .wait,
            None
        );
        for method in [
            http::Method::GET,
            http::Method::HEAD,
            http::Method::OPTIONS,
            http::Method::PUT,
            http::Method::DELETE,
        ] {
            assert_eq!(
                again(
                    &Retry::standard(),
                    status(503),
                    1,
                    &method,
                    Idempotent::ByMethod,
                    true,
                    None,
                    SystemTime::UNIX_EPOCH,
                    0.5
                )
                .wait,
                Some(Duration::from_millis(500))
            );
        }
    }

    #[test]
    fn c4_7_rules_receive_the_status_headers_problem_class_and_attempt() {
        let mut h = http::HeaderMap::new();
        h.insert("x-limit", "quota".parse().unwrap());
        let headers = Headers::from(h);
        let problem = String::from("maintenance");
        let policy = Retry::<String>::standard().unless(|tried| {
            assert_eq!(tried.number(), 1);
            assert_eq!(tried.status(), Some(http::StatusCode::SERVICE_UNAVAILABLE));
            assert_eq!(tried.headers().unwrap().as_map()["x-limit"], "quota");
            assert_eq!(tried.problem().unwrap(), "maintenance");
            assert_eq!(tried.class(), None);
            true
        });
        let outcome = Outcome {
            kind: Kind::Refused,
            status: Some(http::StatusCode::SERVICE_UNAVAILABLE),
            class: None,
            headers: Some(&headers),
            problem: Some(&problem),
        };
        assert_eq!(
            again(
                &policy,
                outcome,
                1,
                &http::Method::GET,
                Idempotent::ByMethod,
                true,
                None,
                SystemTime::UNIX_EPOCH,
                0.5
            )
            .wait,
            None
        );
    }
    #[test]
    fn c4_2_step_six_standard_table_and_no_other_failure() {
        for code in [408, 429, 502, 503, 504] {
            assert!(decide(&Retry::standard(), status(code)).is_some());
        }
        for class in [Class::Connect, Class::Timeout, Class::Body, Class::Other] {
            let o = Outcome {
                kind: Kind::Transport,
                status: None,
                class: Some(class),
                headers: None,
                problem: None,
            };
            assert_eq!(
                decide(&Retry::standard(), o).is_some(),
                class != Class::Other
            );
        }
        assert_eq!(decide(&Retry::standard(), status(500)), None);
        assert_eq!(decide(&Retry::none().tries(3), status(503)), None);
    }
    #[test]
    fn c4_3_step_seven_retry_after_90_and_rule_over_60_stop() {
        let mut h = http::HeaderMap::new();
        h.insert("retry-after", "90".parse().unwrap());
        let headers = Headers::from(h);
        let mut o = status(503);
        o.headers = Some(&headers);
        assert_eq!(decide(&Retry::standard(), o), None);
        assert_eq!(
            decide(
                &Retry::standard().when(|_| Some(Duration::from_secs(61))),
                status(503)
            ),
            None
        );
    }
    #[test]
    fn c4_3_step_seven_retry_after_date_seconds_and_invalid_text() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        for value in [
            "12".to_owned(),
            "Tue, 14 Nov 2023 22:13:32 GMT".to_owned(),
            "invalid".to_owned(),
        ] {
            let mut h = http::HeaderMap::new();
            h.insert("retry-after", value.parse().unwrap());
            let headers = Headers::from(h);
            let mut o = status(503);
            o.headers = Some(&headers);
            assert_eq!(
                again(
                    &Retry::standard(),
                    o,
                    1,
                    &http::Method::GET,
                    Idempotent::ByMethod,
                    true,
                    None,
                    now,
                    0.5
                )
                .wait,
                Some(if value == "invalid" {
                    Duration::from_millis(500)
                } else {
                    Duration::from_secs(12)
                })
            );
        }
    }

    fn with_retry_after(value: &str, now: SystemTime) -> Option<Duration> {
        let mut headers = http::HeaderMap::new();
        headers.insert("retry-after", value.parse().unwrap());
        let headers = Headers::from(headers);
        let mut outcome = status(503);
        outcome.headers = Some(&headers);
        again(
            &Retry::standard(),
            outcome,
            1,
            &http::Method::GET,
            Idempotent::ByMethod,
            true,
            None,
            now,
            0.5,
        )
        .wait
    }

    #[test]
    fn c4_3_retry_accepts_rfc_leap_seconds_in_each_form() {
        // 2016-12-31 23:59:00 UTC, just before the leap second.
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_483_228_740);
        for text in [
            "Sat, 31 Dec 2016 23:59:60 GMT",
            "Saturday, 31-Dec-16 23:59:60 GMT",
            "Sat Dec 31 23:59:60 2016",
        ] {
            let mut headers = http::HeaderMap::new();
            headers.insert("retry-after", text.parse().unwrap());
            let headers = Headers::from(headers);
            let mut outcome = status(503);
            outcome.headers = Some(&headers);
            let decision = again(
                &Retry::standard(),
                outcome,
                1,
                &http::Method::GET,
                Idempotent::ByMethod,
                true,
                None,
                now,
                0.5,
            );
            assert_eq!(
                decision.header_wait,
                Some(Duration::from_secs(60)),
                "{text}"
            );
            assert_eq!(decision.wait, decision.header_wait, "{text}");
        }
    }

    #[test]
    fn c4_3_retry_after_reads_all_three_http_date_forms() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(784111777);
        for date in [
            "Sun, 06 Nov 1994 08:49:49 GMT",
            "Sunday, 06-Nov-94 08:49:49 GMT",
            "Sun Nov  6 08:49:49 1994",
        ] {
            assert_eq!(
                with_retry_after(date, now),
                Some(Duration::from_secs(12)),
                "{date}"
            );
        }
    }

    // Two dates that check the rolling-century interpretation of a two-digit year.
    #[test]
    fn c4_3_retry_after_rfc850_uses_the_supplied_clocks_century() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(3_155_760_000);
        assert_eq!(
            with_retry_after("Wednesday, 01-Jan-70 00:00:12 GMT", now),
            Some(Duration::from_secs(12))
        );
    }

    #[test]
    fn c4_3_retry_after_rfc850_future_date_over_sixty_seconds_stops_today() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_794_441_600);
        assert_eq!(
            with_retry_after("Tuesday, 01-Jan-75 00:00:00 GMT", now),
            None
        );
        assert_eq!(with_retry_after("Tue, 01 Jan 2075 00:00:00 GMT", now), None);
        assert_eq!(with_retry_after("Tue Jan  1 00:00:00 2075", now), None);
    }

    #[test]
    fn c4_3_retry_after_past_dates_and_invalid_text_keep_the_fixed_wait() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_794_441_600);
        for date in [
            "Sun, 06 Nov 1994 08:49:37 GMT",
            "Sunday, 06-Nov-94 08:49:37 GMT",
            "Sun Nov  6 08:49:37 1994",
            "Wed, 31 Dec 1969 00:00:00 GMT",
            "not a date",
        ] {
            assert_eq!(
                with_retry_after(date, now),
                Some(Duration::from_millis(500)),
                "{date}"
            );
        }
    }
    #[test]
    fn c4_3_steps_seven_and_eight_backoff_deadline_and_jitter() {
        let policy = Retry::standard().tries(100);
        for (n, seconds) in [(1, 1), (2, 2), (3, 4), (7, 60), (99, 60)] {
            let wait = Duration::from_secs(seconds);
            assert_eq!(
                again(
                    &policy,
                    status(503),
                    n,
                    &http::Method::GET,
                    Idempotent::ByMethod,
                    true,
                    None,
                    SystemTime::UNIX_EPOCH,
                    1.0
                )
                .wait,
                Some(wait)
            );
            assert_eq!(
                again(
                    &policy,
                    status(503),
                    n,
                    &http::Method::GET,
                    Idempotent::ByMethod,
                    true,
                    Some(wait),
                    SystemTime::UNIX_EPOCH,
                    1.0
                )
                .wait,
                None
            );
        }
    }

    #[test]
    fn c4_3_oversized_rule_wait_is_retained_and_the_rule_is_called_once() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let count = Arc::new(AtomicUsize::new(0));
        let seen = count.clone();
        let retry = Retry::standard().when(move |_| {
            seen.fetch_add(1, Ordering::SeqCst);
            Some(Duration::from_secs(90))
        });
        let decision = again(
            &retry,
            status(429),
            1,
            &http::Method::GET,
            Idempotent::ByMethod,
            true,
            None,
            SystemTime::UNIX_EPOCH,
            0.0,
        );
        assert_eq!(decision.wait, None);
        assert_eq!(decision.header_wait, None);
        assert_eq!(decision.rule_wait, Some(Duration::from_secs(90)));
        assert_eq!(decision.retry_after(), Some(Duration::from_secs(90)));
        assert_eq!(decision.retry_after(), Some(Duration::from_secs(90)));
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn c4_3_decision_keeps_header_and_first_rule_wait_and_reports_the_longer_one() {
        let mut h = http::HeaderMap::new();
        h.insert("retry-after", "30".parse().unwrap());
        let headers = Headers::from(h);
        let mut outcome = status(503);
        outcome.headers = Some(&headers);
        let policy = Retry::standard()
            .when(|_| None)
            .when(|_| Some(Duration::from_secs(45)))
            .when(|_| panic!("the first rule giving a wait wins"));
        let decision = again(
            &policy,
            outcome,
            1,
            &http::Method::GET,
            Idempotent::ByMethod,
            true,
            None,
            SystemTime::UNIX_EPOCH,
            0.0,
        );
        assert_eq!(decision.header_wait, Some(Duration::from_secs(30)));
        assert_eq!(decision.rule_wait, Some(Duration::from_secs(45)));
        assert_eq!(decision.retry_after(), Some(Duration::from_secs(45)));
        assert_eq!(decision.wait, Some(Duration::from_secs(45)));
        assert_eq!(
            Decision {
                header_wait: Some(Duration::from_secs(50)),
                ..decision
            }
            .retry_after(),
            Some(Duration::from_secs(50))
        );
    }

    #[test]
    fn c4_3_a_stopped_decision_keeps_the_header_without_asking_a_rule() {
        let mut h = http::HeaderMap::new();
        h.insert("retry-after", "30".parse().unwrap());
        let headers = Headers::from(h);
        let mut outcome = status(503);
        outcome.headers = Some(&headers);
        let decision = again(
            &Retry::standard()
                .tries(1)
                .when(|_| panic!("budget exhausted")),
            outcome,
            1,
            &http::Method::GET,
            Idempotent::ByMethod,
            true,
            None,
            SystemTime::UNIX_EPOCH,
            0.0,
        );
        assert_eq!(decision.wait, None);
        assert_eq!(decision.rule_wait, None);
        assert_eq!(decision.retry_after(), Some(Duration::from_secs(30)));
    }
    #[test]
    fn c4_1_only_a_refusal_with_a_listed_status_opens_the_post_gate() {
        let policy = Retry::<()>::standard().not_applied_on(429);
        for (kind, class, status, retries) in [
            (Kind::Transport, Some(Class::Body), 429, false),
            (Kind::Transport, Some(Class::Timeout), 429, false),
            (Kind::Transport, Some(Class::Body), 200, false),
            (Kind::Refused, None, 429, true),
        ] {
            let decision = again(
                &policy,
                Outcome {
                    kind,
                    class,
                    status: Some(http::StatusCode::from_u16(status).unwrap()),
                    headers: None,
                    problem: None,
                },
                1,
                &http::Method::POST,
                Idempotent::ByMethod,
                true,
                None,
                SystemTime::UNIX_EPOCH,
                0.5,
            );
            assert_eq!(decision.wait.is_some(), retries);
        }
    }
}
