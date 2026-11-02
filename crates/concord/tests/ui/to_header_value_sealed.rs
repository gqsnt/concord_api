// `ToHeaderValue` is closed: a domain type cannot choose its own header bytes or sensitivity flag.
// The only value that can carry a native flag is a `HeaderValue`, which has no `Display` and so is
// never text: the review lead "a natively flagged value also written as text, whose header was
// removed" cannot be written. Send a domain type's text with a template, or build a `HeaderValue`.
use concord::ToHeaderValue;
use concord::reqwest::header::HeaderValue;

pub struct Signature(String);

impl ToHeaderValue for Signature {
    fn to_header_value(&self) -> Option<HeaderValue> {
        let mut value = HeaderValue::from_str(&self.0).ok()?;
        value.set_sensitive(true);
        Some(value)
    }
}

fn main() {}
