//! Durable, value-redacted traversal positions.

use std::{fmt, marker::PhantomData};

use super::identity::deterministic_digest;
use super::{PaginationController, PaginationError};

/// Opt-in durable state for a pagination controller.
///
/// State must use a deterministic, process-independent format. Implementations
/// must validate restored state and must not include credentials. State is plain
/// data, not encrypted; errors must never contain state or cursor values.
///
/// # Extension contract
///
/// * **Ownership.** State belongs to one traversal. Saving borrows it without
///   changing the pending request; restoration creates an independent value.
/// * **Concurrency.** Calls are synchronous on a traversal's private controller;
///   independent traversals may encode and restore concurrently.
/// * **Cancellation.** Calls must be prompt, finite and free of external I/O.
/// * **Input sensitivity.** Encodings are plain data. Never include credentials
///   or sensitive inputs; core rejects sensitive cursor destinations.
/// * **Errors.** Restoration validates all bytes and invariants and returns
///   value-free errors. Core discards any restoration diagnostic detail.
/// * **Resource obligations.** Bound synchronous CPU and allocations. Tokens
///   including state and binding overhead must fit 4096 ASCII characters.
/// * **Negative capabilities.** Encoding and restoration grant no authority to
///   send, authenticate or change inputs outside the mapped cursor. Restored
///   requests pass ordinary request preparation and continuation validation.
pub trait ResumableController<P>: PaginationController<P> {
    /// Encodes the complete state for the next request.
    fn save_state(&self) -> Vec<u8>;
    /// Restores and validates a previously encoded state.
    ///
    /// # Errors
    /// Returns a value-free pagination error for invalid state.
    fn restore_state(state: &[u8]) -> Result<Self, PaginationError>;
}

/// Classification of a checkpoint failure.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckpointErrorKind {
    /// The traversal has finished or proved exhaustion.
    Finished,
    /// The controller has not opted into durable state.
    NotResumable,
    /// A cursor destination is sensitive.
    SensitiveCursor,
    /// The encoded token would exceed 4096 characters.
    TooLarge,
    /// The token or restored state is invalid or corrupted.
    Malformed,
    /// The token uses an unknown format version.
    UnsupportedVersion,
    /// The endpoint differs from the checkpoint endpoint.
    EndpointMismatch,
    /// The controller differs from the checkpoint controller.
    ControllerMismatch,
    /// A public non-cursor input differs.
    InputMismatch,
}

/// A value-free checkpoint error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckpointError {
    kind: CheckpointErrorKind,
}
impl CheckpointError {
    /// Constructs a value-free error.
    #[must_use]
    pub const fn new(kind: CheckpointErrorKind) -> Self {
        Self { kind }
    }
    /// Returns the classification.
    #[must_use]
    pub const fn kind(&self) -> CheckpointErrorKind {
        self.kind
    }
}
impl fmt::Display for CheckpointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "checkpoint failed: {:?}", self.kind)
    }
}
impl std::error::Error for CheckpointError {}

/// A portable position of the next page of a lazy traversal.
///
/// Tokens are checksummed plain data. They detect corruption, not forgery.
/// Debug output never includes state, cursor or input values.
#[derive(Clone, PartialEq, Eq)]
pub struct Checkpoint {
    pub(super) endpoint: String,
    pub(super) controller: String,
    pub(super) fingerprint: u64,
    pub(super) state: Vec<u8>,
    pub(super) next_index: u32,
}
impl fmt::Debug for Checkpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Checkpoint")
            .field("next_page_index", &self.next_index)
            .finish_non_exhaustive()
    }
}
impl Checkpoint {
    /// Returns the zero-based index of the page to fetch next.
    #[must_use]
    pub const fn next_page_index(&self) -> u32 {
        self.next_index
    }

    fn bytes(&self) -> Vec<u8> {
        let mut out = vec![1];
        out.extend_from_slice(&self.next_index.to_be_bytes());
        out.extend_from_slice(&self.fingerprint.to_be_bytes());
        for field in [
            self.endpoint.as_bytes(),
            self.controller.as_bytes(),
            &self.state,
        ] {
            out.extend_from_slice(&(field.len() as u32).to_be_bytes());
            out.extend_from_slice(field);
        }
        let checksum = deterministic_digest(&out);
        out.extend_from_slice(&checksum.to_be_bytes());
        out
    }

    pub(super) fn validate_size(self) -> Result<Self, CheckpointError> {
        if self.bytes().len() > 2048 {
            return Err(CheckpointError::new(CheckpointErrorKind::TooLarge));
        }
        Ok(self)
    }

    /// Encodes this checkpoint as printable ASCII, at most 4096 characters.
    #[must_use]
    pub fn to_token(&self) -> String {
        const HEX: &[u8] = b"0123456789abcdef";
        let bytes = self.bytes();
        let mut out = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            out.push(char::from(HEX[usize::from(byte >> 4)]));
            out.push(char::from(HEX[usize::from(byte & 15)]));
        }
        out
    }

    /// Decodes and validates a portable token.
    ///
    /// # Errors
    /// Returns a value-free error for malformed, oversized or unsupported tokens.
    pub fn from_token(token: &str) -> Result<Self, CheckpointError> {
        let malformed = || CheckpointError::new(CheckpointErrorKind::Malformed);
        if token.len() > 4096 {
            return Err(CheckpointError::new(CheckpointErrorKind::TooLarge));
        }
        if token.len() % 2 != 0 {
            return Err(malformed());
        }
        let digit = |b: u8| match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            _ => None,
        };
        let bytes = token
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                Ok((digit(pair[0]).ok_or_else(malformed)? << 4)
                    | digit(pair[1]).ok_or_else(malformed)?)
            })
            .collect::<Result<Vec<_>, CheckpointError>>()?;
        if bytes.len() < 33 {
            return Err(malformed());
        }
        let (data, checksum) = bytes.split_at(bytes.len() - 8);
        if deterministic_digest(data).to_be_bytes() != checksum {
            return Err(malformed());
        }
        if data[0] != 1 {
            return Err(CheckpointError::new(
                CheckpointErrorKind::UnsupportedVersion,
            ));
        }
        let next_index = u32::from_be_bytes(data[1..5].try_into().map_err(|_| malformed())?);
        let fingerprint = u64::from_be_bytes(data[5..13].try_into().map_err(|_| malformed())?);
        let mut rest = &data[13..];
        let mut field = || -> Result<Vec<u8>, CheckpointError> {
            let len = u32::from_be_bytes(
                rest.get(..4)
                    .ok_or_else(malformed)?
                    .try_into()
                    .map_err(|_| malformed())?,
            ) as usize;
            rest = &rest[4..];
            let value = rest.get(..len).ok_or_else(malformed)?.to_vec();
            rest = &rest[len..];
            Ok(value)
        };
        let endpoint = String::from_utf8(field()?).map_err(|_| malformed())?;
        let controller = String::from_utf8(field()?).map_err(|_| malformed())?;
        let state = field()?;
        if !rest.is_empty() {
            return Err(malformed());
        }
        Ok(Self {
            endpoint,
            controller,
            fingerprint,
            state,
            next_index,
        })
    }
}

/// Type-erased opt-in callbacks used by generated endpoint implementations.
#[doc(hidden)]
pub struct CheckpointCodec<C, P> {
    save: fn(&C) -> Vec<u8>,
    restore: fn(&[u8]) -> Result<C, PaginationError>,
    marker: PhantomData<fn(P)>,
}
impl<C, P> Copy for CheckpointCodec<C, P> {}
impl<C, P> Clone for CheckpointCodec<C, P> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<C, P> Default for CheckpointCodec<C, P>
where
    C: ResumableController<P>,
{
    fn default() -> Self {
        Self::new()
    }
}
impl<C, P> CheckpointCodec<C, P>
where
    C: ResumableController<P>,
{
    /// Selects the controller's opt-in callbacks.
    pub const fn new() -> Self {
        Self {
            save: C::save_state,
            restore: C::restore_state,
            marker: PhantomData,
        }
    }
}
impl<C, P> CheckpointCodec<C, P> {
    pub(super) fn save(&self, controller: &C) -> Vec<u8> {
        (self.save)(controller)
    }
    pub(super) fn restore(&self, state: &[u8]) -> Result<C, PaginationError> {
        (self.restore)(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn checkpoint() -> Checkpoint {
        Checkpoint {
            endpoint: "endpoint".into(),
            controller: "controller".into(),
            fingerprint: 12,
            state: b"private-state".to_vec(),
            next_index: 4,
        }
    }
    #[test]
    fn token_roundtrip_and_redaction() {
        let cp = checkpoint();
        let token = cp.to_token();
        assert_eq!(Checkpoint::from_token(&token).unwrap(), cp);
        assert!(
            token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        );
        assert!(!format!("{cp:?}").contains("private-state"));
        assert_eq!(cp.next_page_index(), 4);
    }
    #[test]
    fn every_single_character_change_fails() {
        let token = checkpoint().to_token();
        for i in 0..token.len() {
            for byte in b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ-_" {
                if token.as_bytes()[i] == *byte {
                    continue;
                }
                let mut altered = token.as_bytes().to_vec();
                altered[i] = *byte;
                assert!(Checkpoint::from_token(std::str::from_utf8(&altered).unwrap()).is_err());
            }
        }
    }
    #[test]
    fn malformed_sizes_versions_and_trailing_data() {
        for token in ["", "0", "zz", "00"] {
            assert!(Checkpoint::from_token(token).is_err());
        }
        let mut cp = checkpoint();
        cp.state = vec![0; 2048];
        assert_eq!(
            cp.validate_size().unwrap_err().kind(),
            CheckpointErrorKind::TooLarge
        );
        let mut bytes = checkpoint().bytes();
        bytes[0] = 2;
        let len = bytes.len() - 8;
        let digest = deterministic_digest(&bytes[..len]).to_be_bytes();
        bytes[len..].copy_from_slice(&digest);
        let token = bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            Checkpoint::from_token(&token).unwrap_err().kind(),
            CheckpointErrorKind::UnsupportedVersion
        );
    }
}
