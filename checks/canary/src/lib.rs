//! Desirability instrument I2 (canary): a secret marked or documented as withheld must never
//! appear in anything Concord or an SDK prints, and must appear on the wire.
//!
//! Each SDK's `tests/canary.rs` builds its clients through public constructors only, putting a
//! distinct canary from [`Canaries::issue`] into every credential, secret input and value its docs say is
//! redacted. It drives the scenarios the test server can produce (2xx, 4xx with the provider error
//! document, 5xx, malformed body, wrong `Content-Type`, truncated body, refused connection),
//! formats everything public it can reach with `{:?}` (and `{}` where implemented), and checks:
//! - [`Canaries::assert_absent`]: no canary in any rendered text (metric S1 = 0);
//! - [`Canaries::assert_on_wire`]: every canary is in at least one recorded request (S1+ = 100%).
//!
//! Canaries are `CANARY<SDK><n>Z` in ASCII upper-case letters and digits, so percent-encoding,
//! header encoding and JSON escaping leave them unchanged and a substring search finds them.

use concord_example_testserver::Recorded;

/// The canaries of one SDK test, in the order they were handed out.
#[derive(Debug)]
pub struct Canaries {
    sdk: String,
    issued: Vec<String>,
    checked: usize,
}

impl Canaries {
    /// `sdk` is reduced to its ASCII letters and digits, upper-cased (`"kraken-spot-rest"` →
    /// `KRAKENSPOTREST`).
    pub fn new(sdk: &str) -> Self {
        let sdk: String = sdk
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_uppercase())
            .collect();
        assert!(
            !sdk.is_empty(),
            "the SDK name needs at least one ASCII letter or digit"
        );
        Self {
            sdk,
            issued: Vec::new(),
            checked: 0,
        }
    }

    /// A new distinct canary, `CANARY<SDK><n>Z` (n from 1).
    pub fn issue(&mut self) -> String {
        let value = format!("CANARY{}{}Z", self.sdk, self.issued.len() + 1);
        self.issued.push(value.clone());
        value
    }

    /// Every canary issued so far.
    pub fn issued(&self) -> &[String] {
        &self.issued
    }

    /// Panics if any issued canary occurs in `rendered`. `label` names what was rendered
    /// (`"Debug of the client"`), so a failure says where the leak is.
    pub fn assert_absent(&mut self, label: &str, rendered: &str) {
        self.checked += 1;
        for canary in &self.issued {
            assert!(
                !rendered.contains(canary.as_str()),
                "canary leak: {canary} appears in {label}: {rendered}"
            );
        }
    }

    /// `assert_absent` on `{:?}` of `value`.
    pub fn assert_debug_absent<T: std::fmt::Debug + ?Sized>(&mut self, label: &str, value: &T) {
        let rendered = format!("{value:?}");
        self.assert_absent(label, &rendered);
        let pretty = format!("{value:#?}");
        self.assert_absent(label, &pretty);
    }

    /// `assert_absent` on `{}` of `value`.
    pub fn assert_display_absent<T: std::fmt::Display + ?Sized>(&mut self, label: &str, value: &T) {
        let rendered = format!("{value}");
        self.assert_absent(label, &rendered);
    }

    /// `assert_absent` on `{:?}` and `{}` of an error and on every `source()` in its chain.
    pub fn assert_error_absent(&mut self, label: &str, error: &(dyn std::error::Error + 'static)) {
        let mut current: Option<&(dyn std::error::Error + 'static)> = Some(error);
        let mut depth = 0;
        while let Some(e) = current {
            self.assert_absent(
                &format!("{label} (Display, source depth {depth})"),
                &e.to_string(),
            );
            self.assert_absent(
                &format!("{label} (Debug, source depth {depth})"),
                &format!("{e:?}"),
            );
            current = e.source();
            depth += 1;
        }
    }

    /// Positive control: panics unless every issued canary occurs in at least one recorded
    /// request (raw target, any header value — a `Basic` credential decoded — or body).
    pub fn assert_on_wire(&self, requests: &[Recorded]) {
        self.assert_on_wire_except(requests, &[]);
    }

    /// [`assert_on_wire`](Self::assert_on_wire), except the listed canaries, which by design never
    /// travel (a signing secret: only its signature is sent). Each exception must be named with
    /// its reason in the test's header comment.
    pub fn assert_on_wire_except(&self, requests: &[Recorded], never_sent: &[&str]) {
        let wire: Vec<String> = requests.iter().map(wire_text).collect();
        for canary in &self.issued {
            if never_sent.contains(&canary.as_str()) {
                continue;
            }
            assert!(
                wire.iter().any(|w| w.contains(canary.as_str())),
                "positive control failed: {canary} was not sent in any of {} recorded request(s)",
                requests.len()
            );
        }
    }

    /// How many renderings were checked (a test asserts this is not trivially small).
    pub fn checked(&self) -> usize {
        self.checked
    }
}

/// The searchable text of a recorded request: the raw target (path and query), every header
/// line (a `Basic` credential also decoded), and the body.
pub fn wire_text(request: &Recorded) -> String {
    let mut out = String::new();
    out.push_str(&request.method);
    out.push(' ');
    out.push_str(&request.target);
    out.push('\n');
    for (name, value) in &request.headers {
        out.push_str(name);
        out.push_str(": ");
        out.push_str(value);
        out.push('\n');
        if name == "authorization"
            && let Some(encoded) = value.strip_prefix("Basic ")
            && let Some(decoded) = base64_decode(encoded.trim())
        {
            out.push_str("authorization (decoded): ");
            out.push_str(&String::from_utf8_lossy(&decoded));
            out.push('\n');
        }
    }
    out.push_str(&String::from_utf8_lossy(&request.body));
    out
}

/// Standard base64 with padding (RFC 4648), enough for `Basic` credentials.
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0;
    for &b in input.as_bytes() {
        if b == b'=' {
            break;
        }
        let v = ALPHABET.iter().position(|&a| a == b)? as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canaries_are_distinct_and_encoding_stable() {
        let mut c = Canaries::new("kraken-spot-rest");
        let a = c.issue();
        let b = c.issue();
        assert_eq!(a, "CANARYKRAKENSPOTREST1Z");
        assert_ne!(a, b);
        assert!(
            a.chars()
                .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
        );
    }

    #[test]
    #[should_panic(expected = "canary leak")]
    fn a_leak_is_reported() {
        let mut c = Canaries::new("x");
        let secret = c.issue();
        c.assert_absent("a test string", &format!("token={secret}"));
    }

    #[test]
    fn basic_credentials_are_decoded_for_the_wire_check() {
        // "user:CANARYX1Z" in base64
        assert_eq!(
            base64_decode("dXNlcjpDQU5BUllYMVo=").unwrap(),
            b"user:CANARYX1Z"
        );
    }

    #[test]
    fn absence_passes_on_clean_text() {
        let mut c = Canaries::new("x");
        c.issue();
        c.assert_absent("clean", "token=<redacted>");
        assert_eq!(c.checked(), 1);
    }
}
