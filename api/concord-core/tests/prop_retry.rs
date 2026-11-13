//! C4 pure retry decisions, including extreme clocks, policies and random bits.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use concord_core::{
    __private::Idempotent,
    error::{Class, Kind},
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    retry::{again, Outcome},
    Headers, Retry,
};
use proptest::prelude::*;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

fn duration() -> impl Strategy<Value = Duration> {
    prop_oneof![
        Just(Duration::ZERO),
        Just(Duration::from_secs(60)),
        Just(Duration::MAX),
        (0u64..120, 0u32..1_000_000_000).prop_map(|(s, n)| Duration::new(s, n))
    ]
}
fn random() -> impl Strategy<Value = f64> {
    prop_oneof![
        any::<u64>().prop_map(f64::from_bits),
        0.0..=1.0,
        prop::sample::select(vec![
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -1.0,
            0.0,
            1.0,
            2.0
        ])
    ]
}
proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]
    #[test]
    /// Also proves C4.3, C4.7.
    fn c4_1_retry_verdict_obeys_all_bounds_and_gates(
        kind in prop::sample::select(vec![Kind::Build, Kind::Transport, Kind::Refused, Kind::Mismatch]),
        class in prop::option::of(prop::sample::select(vec![Class::Connect,Class::Timeout,Class::Body,Class::Blocked,Class::Other])),
        status in prop::option::of(100u16..1000),
        standard in any::<bool>(), tries in any::<u32>(), n in prop_oneof![0u32..100, any::<u32>()],
        method in prop::sample::select(vec![Method::GET,Method::HEAD,Method::OPTIONS,Method::PUT,Method::DELETE,Method::POST,Method::PATCH]),
        mark in prop::sample::select(vec![Idempotent::ByMethod, Idempotent::Yes, Idempotent::No]),
        listed in proptest::collection::vec(any::<u16>(),0..5), list_status in any::<bool>(), can_copy in any::<bool>(),
        left in prop::option::of(duration()), after in prop::option::of(any::<u64>()),
        rules in proptest::collection::vec(prop::option::of(duration()),0..5), vetoes in proptest::collection::vec(any::<bool>(),0..5),
        now_seconds in any::<u64>(), rng in random(),
    ) {
        let now = SystemTime::UNIX_EPOCH
            .checked_add(Duration::from_secs(now_seconds))
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let mut headers = HeaderMap::new();
        if let Some(after) = after {
            headers.insert(
                "retry-after",
                HeaderValue::from_str(&after.to_string()).unwrap(),
            );
        }
        let headers = Headers::from(headers);
        let mut policy = if standard {
            Retry::<()>::standard()
        } else {
            Retry::none()
        }
        .tries(tries);
        for code in &listed {
            policy = policy.not_applied_on(*code);
        }
        if list_status {
            if let Some(code) = status {
                policy = policy.not_applied_on(code);
            }
        }
        let calls = Arc::new(Mutex::new(Vec::new()));
        for (i, veto) in vetoes.iter().copied().enumerate() {
            let calls = calls.clone();
            policy = policy.unless(move |_| {
                calls.lock().unwrap().push((false, i));
                veto
            });
        }
        for (i, wait) in rules.iter().copied().enumerate() {
            let calls = calls.clone();
            policy = policy.when(move |_| {
                calls.lock().unwrap().push((true, i));
                wait
            });
        }
        let decision = again(
            &policy,
            Outcome::<()> {
                kind,
                class,
                status: status.map(|s| StatusCode::from_u16(s).unwrap()),
                headers: Some(&headers),
                problem: None,
            },
            n,
            &method,
            mark,
            can_copy,
            left,
            now,
            rng,
        );
        prop_assert_eq!(decision.header_wait, after.map(Duration::from_secs));
        let permitted = mark == Idempotent::Yes
            || mark == Idempotent::ByMethod
                && matches!(
                    method,
                    Method::GET | Method::HEAD | Method::OPTIONS | Method::PUT | Method::DELETE
                )
            || kind == Kind::Refused && status.is_some_and(|s| listed.contains(&s) || list_status);
        let gate = !matches!(kind, Kind::Build | Kind::Mismatch)
            && class != Some(Class::Blocked)
            && n < tries.max(1)
            && can_copy
            && permitted;
        let mut expected_calls = Vec::new();
        let asked = if gate {
            expected_calls.extend((0..vetoes.len()).map(|i| (false, i)));
            if vetoes.iter().any(|v| *v) {
                None
            } else {
                let mut result = None;
                for (i, wait) in rules.iter().copied().enumerate() {
                    expected_calls.push((true, i));
                    if wait.is_some() {
                        result = wait;
                        break;
                    }
                }
                result
            }
        } else {
            None
        };
        prop_assert_eq!(calls.lock().unwrap().clone(), expected_calls);
        prop_assert_eq!(decision.rule_wait, asked);
        prop_assert_eq!(
            decision.retry_after(),
            after.map(Duration::from_secs).max(asked)
        );
        let worth = asked.is_some()
            || standard
                && (matches!(class, Some(Class::Connect | Class::Timeout | Class::Body))
                    || matches!(status, Some(408 | 429 | 502 | 503 | 504)));
        let lower = asked
            .unwrap_or_default()
            .max(after.map(Duration::from_secs).unwrap_or_default());
        let ceiling = Duration::from_secs(if n <= 6 {
            1u64 << n.saturating_sub(1)
        } else {
            60
        });
        let fraction = if rng.is_finite() {
            rng.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let wanted = lower.max(ceiling.mul_f64(fraction));
        let expected = (gate
            && !vetoes.iter().any(|v| *v)
            && worth
            && lower <= Duration::from_secs(60)
            && left.is_none_or(|l| wanted < l))
        .then_some(wanted);
        prop_assert_eq!(decision.wait, expected);
        if let Some(wait) = decision.wait {
            prop_assert!(wait >= lower && wait <= Duration::from_secs(60));
            prop_assert!(left.is_none_or(|l| wait < l));
            prop_assert!(can_copy);
            if method == Method::POST && mark != Idempotent::Yes {
                prop_assert!(status.is_some_and(|s| listed.contains(&s) || list_status));
            }
        }
    }

    #[test]
    fn c4_3_retry_after_arbitrary_bytes_and_dates_never_panic(
        raw in proptest::collection::vec(any::<u8>(),0..512), now in any::<u64>(),
        date in prop::sample::select(vec!["Sun, 06 Nov 1994 08:49:37 GMT", "Sunday, 06-Nov-94 08:49:37 GMT", "Sun Nov  6 08:49:37 1994", "0", "60", "61", "18446744073709551616", "-1", "NaN", ""]),
    ) {
        let now = SystemTime::UNIX_EPOCH
            .checked_add(Duration::from_secs(now))
            .unwrap_or(SystemTime::UNIX_EPOCH);
        for bytes in [raw.as_slice(), date.as_bytes()] {
            let mut map = HeaderMap::new();
            if let Ok(value) = HeaderValue::from_bytes(bytes) {
                map.insert("retry-after", value);
            }
            let headers = Headers::from(map);
            let parsed = headers.retry_after_at(now);
            let decision = again(
                &Retry::<()>::standard(),
                Outcome {
                    kind: Kind::Refused,
                    status: Some(StatusCode::SERVICE_UNAVAILABLE),
                    class: None,
                    headers: Some(&headers),
                    problem: None,
                },
                1,
                &Method::GET,
                Idempotent::ByMethod,
                true,
                None,
                now,
                0.0,
            );
            prop_assert_eq!(decision.header_wait, parsed);
            prop_assert_eq!(
                decision.wait,
                parsed
                    .filter(|d| *d <= Duration::from_secs(60))
                    .or_else(|| parsed.is_none().then_some(Duration::ZERO))
            );
        }
    }
}
