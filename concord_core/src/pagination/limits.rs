use std::collections::HashMap;

use super::PaginationError;
use super::identity::{CanonicalIdentity, deterministic_digest};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PaginationLimits {
    pub(super) max_pages: usize,
    pub(super) max_items: usize,
    pub(super) max_retained_identity_bytes: usize,
}

pub(super) struct CycleDetector {
    buckets: HashMap<u64, Vec<Vec<u8>>>,
    retained_identity_count: usize,
    retained_identity_bytes: usize,
    digest: fn(&[u8]) -> u64,
}

impl Default for CycleDetector {
    fn default() -> Self {
        Self::new(deterministic_digest)
    }
}

impl CycleDetector {
    pub(super) fn new(digest: fn(&[u8]) -> u64) -> Self {
        Self {
            buckets: HashMap::new(),
            retained_identity_count: 0,
            retained_identity_bytes: 0,
            digest,
        }
    }

    pub(super) fn preflight(
        &self,
        identity: &CanonicalIdentity,
        byte_limit: usize,
    ) -> Result<(u64, usize), PaginationError> {
        let digest = (self.digest)(identity.bytes());
        if self
            .buckets
            .get(&digest)
            .is_some_and(|bucket| bucket.iter().any(|bytes| bytes == identity.bytes()))
        {
            return Err(PaginationError::RepeatedIdentity);
        }

        let retained_bytes = self
            .retained_identity_bytes
            .checked_add(identity.retained_byte_len())
            .ok_or(PaginationError::RetainedIdentityByteOverflow)?;
        if retained_bytes > byte_limit {
            return Err(PaginationError::RetainedIdentityByteLimit);
        }
        Ok((digest, retained_bytes))
    }

    pub(super) fn retain(
        &mut self,
        identity: CanonicalIdentity,
        digest: u64,
        retained_bytes: usize,
    ) -> Result<(), PaginationError> {
        let retained_count = self
            .retained_identity_count
            .checked_add(1)
            .ok_or(PaginationError::RetainedIdentityByteOverflow)?;
        self.buckets
            .entry(digest)
            .or_default()
            .push(identity.bytes().to_vec());
        self.retained_identity_count = retained_count;
        self.retained_identity_bytes = retained_bytes;
        Ok(())
    }

    pub(super) fn check_and_retain(
        &mut self,
        identity: CanonicalIdentity,
        byte_limit: usize,
    ) -> Result<(), PaginationError> {
        let (digest, retained_bytes) = self.preflight(&identity, byte_limit)?;
        self.retain(identity, digest, retained_bytes)
    }

    pub(super) fn retained_identity_count(&self) -> usize {
        self.retained_identity_count
    }

    pub(super) fn retained_identity_bytes(&self) -> usize {
        self.retained_identity_bytes
    }

    #[cfg(test)]
    pub(super) fn set_retained_identity_bytes(&mut self, bytes: usize) {
        self.retained_identity_bytes = bytes;
    }
}
