//! Cloudflare's rate-limit headers, read as the limits page documents them.
//!
//! - `Ratelimit`: a list of service limit items, each a limit name with its remaining quota
//!   (`r`) and the seconds until its window resets (`t`): `"default";r=50;t=30`.
//! - `Ratelimit-Policy`: a list of quota policies, each a name with its total quota (`q`) and
//!   window in seconds (`w`): `"burst";q=100;w=60`.
//! - `Retry-After`: seconds, rounded up, until more capacity is available; only on a request
//!   that exceeded a limit ([`RetryAfter`]).
//!
//! The global limit (per user or account token, per IP) and per-service limits can apply to the
//! same request, so the SDK keeps every item, by name and in order, and never folds them into
//! one bucket. It does not throttle on its own: these are observations for the caller.
//!
//! The lists are read with the HTTP structured-field list syntax (RFC 8941): an item is a
//! string or a token, followed by `;key=value` parameters. Parameters other than `r`, `t`, `q`
//! and `w` are ignored; those four must be non-negative integers. Several field lines form one
//! list. Anything else is a [`RateLimitHeaderError`], which never contains the header's value.

use std::time::{Duration, SystemTime};

use concord::reqwest::header::{HeaderMap, RETRY_AFTER};

/// One service limit item of `Ratelimit`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RateLimit {
    /// The limit's name (`"default"`).
    pub name: String,
    /// Remaining quota in the current window (`r`).
    pub remaining: Option<u64>,
    /// Time until the window resets (`t`, seconds).
    pub reset: Option<Duration>,
}

/// One quota policy of `Ratelimit-Policy`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RateLimitPolicy {
    /// The policy's name (`"burst"`).
    pub name: String,
    /// Total quota of the window (`q`).
    pub quota: Option<u64>,
    /// The window the quota applies to (`w`, seconds).
    pub window: Option<Duration>,
}

/// Every limit item and policy a response reported, in header order. Both lists are empty when
/// the headers are absent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct RateLimits {
    /// `Ratelimit` items.
    pub limits: Vec<RateLimit>,
    /// `Ratelimit-Policy` items.
    pub policies: Vec<RateLimitPolicy>,
}

impl RateLimits {
    /// The limit item called `name`, if reported.
    pub fn limit(&self, name: &str) -> Option<&RateLimit> {
        self.limits.iter().find(|l| l.name == name)
    }

    /// The items whose remaining quota is zero: the limits a further request would exceed.
    pub fn exhausted(&self) -> impl Iterator<Item = &RateLimit> {
        self.limits.iter().filter(|l| l.remaining == Some(0))
    }

    /// Reads both headers.
    ///
    /// # Errors
    ///
    /// [`RateLimitHeaderError`] naming the header that does not follow the documented syntax.
    pub fn from_headers(headers: &HeaderMap) -> Result<Self, RateLimitHeaderError> {
        let limits = members(headers, "ratelimit", RateLimitHeaderError::Ratelimit)?
            .into_iter()
            .map(|m| {
                Ok(RateLimit {
                    remaining: m.integer("r")?,
                    reset: m.integer("t")?.map(Duration::from_secs),
                    name: m.name,
                })
            })
            .collect::<Result<_, ()>>()
            .map_err(|()| RateLimitHeaderError::Ratelimit)?;
        let policies = members(headers, "ratelimit-policy", RateLimitHeaderError::Policy)?
            .into_iter()
            .map(|m| {
                Ok(RateLimitPolicy {
                    quota: m.integer("q")?,
                    window: m.integer("w")?.map(Duration::from_secs),
                    name: m.name,
                })
            })
            .collect::<Result<_, ()>>()
            .map_err(|()| RateLimitHeaderError::Policy)?;
        Ok(Self { limits, policies })
    }
}

/// A rate-limit header that does not follow the documented syntax. The value is not kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[derive(concord::derive::Error)]
#[display(with = display_rate_limit_header_error)]
pub enum RateLimitHeaderError {
    /// `Ratelimit` is malformed.
    Ratelimit,
    /// `Ratelimit-Policy` is malformed.
    Policy,
}

fn display_rate_limit_header_error(
    value: &RateLimitHeaderError,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    f.write_str(match value {
        RateLimitHeaderError::Ratelimit => "the Ratelimit header is malformed",
        RateLimitHeaderError::Policy => "the Ratelimit-Policy header is malformed",
    })
}

/// `Retry-After`, read strictly: one field, delay seconds or an HTTP date.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetryAfter {
    /// No `Retry-After` header.
    Absent,
    /// Wait at least this long.
    After(Duration),
    /// A header that is repeated, not a delay or a date, or too large.
    Invalid,
}

impl RetryAfter {
    /// Reads `Retry-After` from `headers` (a date is compared with the current system time).
    pub fn from_headers(headers: &HeaderMap) -> Self {
        let values = headers.get_all(RETRY_AFTER).iter().map(|v| v.as_bytes());
        match concord_retry::http::retry_after(values, SystemTime::now()) {
            Ok(None) => Self::Absent,
            Ok(Some(delay)) => Self::After(delay),
            Err(_) => Self::Invalid,
        }
    }

    /// The delay, when one was sent and is valid.
    pub fn delay(self) -> Option<Duration> {
        match self {
            Self::After(delay) => Some(delay),
            Self::Absent | Self::Invalid => None,
        }
    }
}

struct Member {
    name: String,
    params: Vec<(String, Param)>,
}

enum Param {
    Integer(i64),
    Other,
}

impl Member {
    /// The non-negative integer parameter `key`; `Err` if present with another value.
    fn integer(&self, key: &str) -> Result<Option<u64>, ()> {
        match self.params.iter().rev().find(|(k, _)| k == key) {
            None => Ok(None),
            Some((_, Param::Integer(n))) => u64::try_from(*n).map(Some).map_err(|_| ()),
            Some((_, Param::Other)) => Err(()),
        }
    }
}

fn members(
    headers: &HeaderMap,
    name: &str,
    error: RateLimitHeaderError,
) -> Result<Vec<Member>, RateLimitHeaderError> {
    let mut joined = Vec::new();
    for (i, value) in headers.get_all(name).iter().enumerate() {
        if i > 0 {
            joined.push(b',');
        }
        joined.extend_from_slice(value.as_bytes());
    }
    parse_list(&joined).ok_or(error)
}

/// RFC 8941 §4.2.1 list, restricted to items (no inner lists).
fn parse_list(input: &[u8]) -> Option<Vec<Member>> {
    let mut p = Parser { input, at: 0 };
    let mut out = Vec::new();
    p.skip(|b| b == b' ' || b == b'\t');
    if p.done() {
        return Some(out);
    }
    loop {
        out.push(p.member()?);
        p.skip(|b| b == b' ' || b == b'\t');
        if p.done() {
            return Some(out);
        }
        if !p.eat(b',') {
            return None;
        }
        p.skip(|b| b == b' ' || b == b'\t');
        if p.done() {
            return None; // a trailing comma
        }
    }
}

struct Parser<'a> {
    input: &'a [u8],
    at: usize,
}

impl Parser<'_> {
    fn done(&self) -> bool {
        self.at >= self.input.len()
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.at).copied()
    }

    fn eat(&mut self, byte: u8) -> bool {
        let hit = self.peek() == Some(byte);
        if hit {
            self.at += 1;
        }
        hit
    }

    fn skip(&mut self, keep: impl Fn(u8) -> bool) {
        while self.peek().is_some_and(&keep) {
            self.at += 1;
        }
    }

    fn take(&mut self, keep: impl Fn(u8) -> bool) -> &[u8] {
        let start = self.at;
        self.skip(keep);
        &self.input[start..self.at]
    }

    fn member(&mut self) -> Option<Member> {
        let name = match self.peek()? {
            b'"' => self.string()?,
            b if b.is_ascii_alphabetic() || b == b'*' => self.token(),
            _ => return None,
        };
        let mut params = Vec::new();
        while self.eat(b';') {
            self.skip(|b| b == b' ');
            let key = self.key()?;
            let value = if self.eat(b'=') {
                self.bare_item()?
            } else {
                Param::Other // a boolean `true` parameter
            };
            params.push((key, value));
        }
        Some(Member { name, params })
    }

    fn key(&mut self) -> Option<String> {
        let first = self.peek()?;
        if !(first.is_ascii_lowercase() || first == b'*') {
            return None;
        }
        let key = self.take(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-' | b'.' | b'*')
        });
        Some(String::from_utf8_lossy(key).into_owned())
    }

    fn bare_item(&mut self) -> Option<Param> {
        match self.peek()? {
            b'-' | b'0'..=b'9' => self.number(),
            b'"' => self.string().map(|_| Param::Other),
            b'?' => {
                self.at += 1;
                matches!(self.peek()?, b'0' | b'1').then(|| self.at += 1)?;
                Some(Param::Other)
            }
            b if b.is_ascii_alphabetic() || b == b'*' => {
                self.token();
                Some(Param::Other)
            }
            _ => None,
        }
    }

    fn number(&mut self) -> Option<Param> {
        let negative = self.eat(b'-');
        let digits = self.take(|b| b.is_ascii_digit());
        if digits.is_empty() || digits.len() > 15 {
            return None;
        }
        let value: i64 = std::str::from_utf8(digits).ok()?.parse().ok()?;
        if self.eat(b'.') {
            // A decimal: syntactically valid, but not an integer parameter.
            let fraction = self.take(|b| b.is_ascii_digit());
            return (!fraction.is_empty() && fraction.len() <= 3).then_some(Param::Other);
        }
        Some(Param::Integer(if negative { -value } else { value }))
    }

    fn string(&mut self) -> Option<String> {
        if !self.eat(b'"') {
            return None;
        }
        let mut out = Vec::new();
        loop {
            match self.peek()? {
                b'"' => {
                    self.at += 1;
                    return String::from_utf8(out).ok();
                }
                b'\\' => {
                    self.at += 1;
                    let escaped = self.peek()?;
                    if escaped != b'"' && escaped != b'\\' {
                        return None;
                    }
                    out.push(escaped);
                    self.at += 1;
                }
                b @ 0x20..=0x7e => {
                    out.push(b);
                    self.at += 1;
                }
                _ => return None,
            }
        }
    }

    fn token(&mut self) -> String {
        let token = self.take(|b| {
            b.is_ascii_alphanumeric()
                || matches!(
                    b,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                        | b':'
                        | b'/'
                )
        });
        String::from_utf8_lossy(token).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use concord::reqwest::header::HeaderValue;

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.append(*name, HeaderValue::from_static(value));
        }
        map
    }

    #[test]
    fn documented_examples() {
        let limits = RateLimits::from_headers(&headers(&[
            ("ratelimit", r#""default";r=50;t=30"#),
            ("ratelimit-policy", r#""burst";q=100;w=60"#),
        ]))
        .unwrap();
        assert_eq!(
            limits.limits,
            [RateLimit {
                name: "default".into(),
                remaining: Some(50),
                reset: Some(Duration::from_secs(30)),
            }]
        );
        assert_eq!(
            limits.policies,
            [RateLimitPolicy {
                name: "burst".into(),
                quota: Some(100),
                window: Some(Duration::from_secs(60)),
            }]
        );
    }

    #[test]
    fn several_items_and_lines_stay_separate() {
        let limits = RateLimits::from_headers(&headers(&[
            ("ratelimit", r#""default";r=0;t=12, dns;r=7;t=3"#),
            ("ratelimit", r#""burst";r=1"#),
        ]))
        .unwrap();
        let names: Vec<_> = limits.limits.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["default", "dns", "burst"]);
        assert_eq!(limits.limit("dns").unwrap().remaining, Some(7));
        assert_eq!(limits.limit("burst").unwrap().reset, None);
        let exhausted: Vec<_> = limits.exhausted().map(|l| l.name.as_str()).collect();
        assert_eq!(exhausted, ["default"]);
    }

    #[test]
    fn absent_headers_are_empty() {
        assert_eq!(
            RateLimits::from_headers(&HeaderMap::new()),
            Ok(RateLimits::default())
        );
    }

    #[test]
    fn malformed_values_are_refused_without_the_value() {
        for bad in [
            r#""default";r=-1"#,
            r#""default";r=5.5"#,
            r#""default";r="5""#,
            r#""default";r=1,"#,
            r#""unterminated;r=1"#,
            r#"1;r=1"#,
            r#""a";R=1"#,
        ] {
            let mut map = HeaderMap::new();
            map.insert("ratelimit", HeaderValue::from_str(bad).unwrap());
            let error = RateLimits::from_headers(&map).unwrap_err();
            assert_eq!(error, RateLimitHeaderError::Ratelimit, "{bad}");
            assert!(!error.to_string().contains("default"));
        }
        let mut map = HeaderMap::new();
        map.insert(
            "ratelimit-policy",
            HeaderValue::from_static(r#""burst";w=x1"#),
        );
        assert_eq!(
            RateLimits::from_headers(&map),
            Err(RateLimitHeaderError::Policy)
        );
    }

    #[test]
    fn unknown_parameters_are_ignored() {
        let limits = RateLimits::from_headers(&headers(&[(
            "ratelimit",
            r#""default";r=3;t=4;pk=:YWJj:;x="y";flag"#,
        )]));
        // A byte sequence (`:..:`) is not part of this reader's subset: refused, not guessed.
        assert_eq!(limits, Err(RateLimitHeaderError::Ratelimit));
        let limits = RateLimits::from_headers(&headers(&[(
            "ratelimit",
            r#""default";r=3;t=4;x="y";flag;z=?1;v=1.5"#,
        )]))
        .unwrap();
        assert_eq!(limits.limits[0].remaining, Some(3));
    }

    #[test]
    fn retry_after_is_strict() {
        assert_eq!(
            RetryAfter::from_headers(&HeaderMap::new()),
            RetryAfter::Absent
        );
        let one = headers(&[("retry-after", "30")]);
        assert_eq!(
            RetryAfter::from_headers(&one),
            RetryAfter::After(Duration::from_secs(30))
        );
        let repeated = headers(&[("retry-after", "30"), ("retry-after", "30")]);
        assert_eq!(RetryAfter::from_headers(&repeated), RetryAfter::Invalid);
        let signed = headers(&[("retry-after", "+5")]);
        assert_eq!(RetryAfter::from_headers(&signed), RetryAfter::Invalid);
    }
}
