//! Content digests: `algorithm ":" hex`, and the verification of content against them.
//!
//! The registry API addresses blobs (and manifests by digest) by content digest. The spec's
//! grammar is `digest := algorithm ":" hex`, `algorithm := /[A-Za-z0-9_+.-]+/`,
//! `hex := /[A-Fa-f0-9]+/`; it says compliant implementations should use `sha256`, that a client
//! must always verify content against the digest used to fetch it, and that a
//! `Docker-Content-Digest` header must not be trusted over the local digest.
//!
//! This SDK can *verify* `sha256` and `sha512` (lowercase hex of exactly 64 and 128 characters, as
//! the registry's own digest library writes them). Any other algorithm parses (it is valid by the
//! grammar) but cannot be verified: fetching content by such a digest is refused before anything
//! is sent ([`IntegrityProblem::UnsupportedAlgorithm`](crate::IntegrityProblem)), never accepted
//! unverified.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::Digest as _;

use crate::error::InputError;

/// A content digest such as `sha256:2cf24dba…`. `Display` and `Debug` show it as written: a
/// digest identifies public content, it is not a secret.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Digest {
    text: String,
    split: usize,
}

/// A digest algorithm this SDK can compute.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Algorithm {
    /// `sha256`, 64 lowercase hex characters.
    Sha256,
    /// `sha512`, 128 lowercase hex characters.
    Sha512,
}

impl Algorithm {
    /// The algorithm's name in a digest (`sha256`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Sha256 => "sha256",
            Self::Sha512 => "sha512",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        match name {
            "sha256" => Some(Self::Sha256),
            "sha512" => Some(Self::Sha512),
            _ => None,
        }
    }

    fn hex_len(self) -> usize {
        match self {
            Self::Sha256 => 64,
            Self::Sha512 => 128,
        }
    }
}

impl Digest {
    /// Parses a digest by the spec's grammar. A `sha256` or `sha512` digest must also have the
    /// algorithm's length in lowercase hex.
    ///
    /// # Errors
    ///
    /// [`InputError::Digest`] when the text is not a digest.
    pub fn parse(text: &str) -> Result<Self, InputError> {
        let (algorithm, hex) = text.split_once(':').ok_or(InputError::Digest)?;
        let algorithm_ok = !algorithm.is_empty()
            && algorithm
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'+' | b'.' | b'-'));
        let hex_ok = !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit());
        if !algorithm_ok || !hex_ok {
            return Err(InputError::Digest);
        }
        if let Some(known) = Algorithm::from_name(algorithm)
            && (hex.len() != known.hex_len() || hex.bytes().any(|b| b.is_ascii_uppercase()))
        {
            return Err(InputError::Digest);
        }
        Ok(Self {
            text: text.to_owned(),
            split: algorithm.len(),
        })
    }

    /// The `sha256` digest of `bytes`.
    pub fn sha256_of(bytes: &[u8]) -> Self {
        let mut hasher = Hasher::new(Algorithm::Sha256);
        hasher.update(bytes);
        hasher.finish()
    }

    /// The digest of `bytes` with `algorithm`.
    pub fn compute(algorithm: Algorithm, bytes: &[u8]) -> Self {
        let mut hasher = Hasher::new(algorithm);
        hasher.update(bytes);
        hasher.finish()
    }

    /// The algorithm part (`sha256`).
    pub fn algorithm(&self) -> &str {
        &self.text[..self.split]
    }

    /// The hex part.
    pub fn hex(&self) -> &str {
        &self.text[self.split + 1..]
    }

    /// The whole digest text.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The algorithm, if this SDK can compute it.
    pub fn supported_algorithm(&self) -> Option<Algorithm> {
        Algorithm::from_name(self.algorithm())
    }

    /// Whether `bytes` have this digest. `None` when the algorithm cannot be computed here.
    pub fn matches(&self, bytes: &[u8]) -> Option<bool> {
        let algorithm = self.supported_algorithm()?;
        Some(Self::compute(algorithm, bytes) == *self)
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl fmt::Debug for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Digest({})", self.text)
    }
}

impl FromStr for Digest {
    type Err = InputError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

impl Serialize for Digest {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.text)
    }
}

impl<'de> Deserialize<'de> for Digest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(|_| serde::de::Error::custom("not a content digest"))
    }
}

/// An incremental digest computation.
pub(crate) struct Hasher {
    state: State,
}

enum State {
    Sha256(sha2::Sha256),
    Sha512(sha2::Sha512),
}

impl Hasher {
    pub(crate) fn new(algorithm: Algorithm) -> Self {
        let state = match algorithm {
            Algorithm::Sha256 => State::Sha256(sha2::Sha256::new()),
            Algorithm::Sha512 => State::Sha512(sha2::Sha512::new()),
        };
        Self { state }
    }

    pub(crate) fn update(&mut self, bytes: &[u8]) {
        match &mut self.state {
            State::Sha256(state) => state.update(bytes),
            State::Sha512(state) => state.update(bytes),
        }
    }

    pub(crate) fn finish(self) -> Digest {
        let (algorithm, hex) = match self.state {
            State::Sha256(state) => (Algorithm::Sha256, lower_hex(&state.finalize())),
            State::Sha512(state) => (Algorithm::Sha512, lower_hex(&state.finalize())),
        };
        Digest {
            split: algorithm.name().len(),
            text: format!("{}:{hex}", algorithm.name()),
        }
    }
}

fn lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_and_sha512_match_published_vectors() {
        // FIPS 180-4 / NIST example values for "abc".
        assert_eq!(
            Digest::sha256_of(b"abc").as_str(),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            Digest::compute(Algorithm::Sha512, b"abc").hex(),
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a\
             2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
        );
    }

    #[test]
    fn grammar_and_known_algorithm_lengths() {
        assert!(Digest::parse("sha256:abc").is_err(), "sha256 needs 64 hex");
        let upper = format!("sha256:{}", "A".repeat(64));
        assert!(Digest::parse(&upper).is_err(), "sha256 hex is lowercase");
        assert!(
            Digest::parse("tarsum.v1+sha256:ABCdef01").is_ok(),
            "by the grammar"
        );
        assert!(Digest::parse(":abc").is_err());
        assert!(Digest::parse("sha256:").is_err());
        assert!(Digest::parse("sha256").is_err());
        assert!(Digest::parse("sha 256:ab").is_err());
        let other = Digest::parse("blake3:00ff").unwrap();
        assert_eq!(other.supported_algorithm(), None);
        assert_eq!(other.matches(b""), None);
    }
}
