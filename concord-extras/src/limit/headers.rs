//! Bounded header shapes; unreadable input never changes prior knowledge.
use super::{Learn, Seen, Window};
use std::time::Duration;

const MAX_HEADER_BYTES: usize = 4096;
const MAX_WINDOWS: usize = 8;

fn text<'a>(seen: &'a Seen<'_>, name: &str) -> Option<&'a str> {
    seen.header(name)
        .filter(|text| text.len() <= MAX_HEADER_BYTES)
}

fn number(text: &str) -> Option<u64> {
    let text = text.trim();
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

fn pairs(text: &str) -> Option<Vec<(u64, u64)>> {
    let mut result = Vec::new();
    for pair in text.split(',') {
        if result.len() == MAX_WINDOWS {
            return None;
        }
        let (n, seconds) = pair.trim().split_once(':')?;
        let n = number(n)?;
        let seconds = number(seconds)?;
        if seconds == 0 || result.iter().any(|(_, old)| *old == seconds) {
            return None;
        }
        result.push((n, seconds));
    }
    result.sort_unstable_by_key(|(_, seconds)| *seconds);
    Some(result)
}

pub(super) fn counts(limits: String, counts: String) -> Box<Learn> {
    Box::new(move |seen| {
        let parse = || {
            let limits = pairs(text(seen, &limits)?)?;
            let counts = pairs(text(seen, &counts)?)?;
            if limits.len() != counts.len() {
                return None;
            }
            limits
                .into_iter()
                .zip(counts)
                .map(|((capacity, seconds), (used, count_seconds))| {
                    if capacity == 0 || seconds != count_seconds {
                        return None;
                    }
                    Some(
                        Window::rolling(capacity, Duration::from_secs(seconds))
                            .remaining(capacity.saturating_sub(used)),
                    )
                })
                .collect::<Option<Vec<_>>>()
        };
        parse().unwrap_or_default()
    })
}

pub(super) fn remaining(limit: String, remaining: String, reset: String) -> Box<Learn> {
    Box::new(move |seen| {
        let parse = || {
            let capacity = number(text(seen, &limit)?)?;
            let remaining = number(text(seen, &remaining)?)?;
            let reset = seen.until_unix(number(text(seen, &reset)?)?)?;
            if capacity == 0 || remaining > capacity || reset.is_zero() {
                return None;
            }
            Some(vec![Window::fixed(capacity, reset).remaining(remaining)])
        };
        parse().unwrap_or_default()
    })
}
