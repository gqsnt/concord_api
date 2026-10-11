use std::collections::HashMap;

use super::PaginationError;
use super::identity::{CanonicalIdentity, deterministic_digest};
use crate::error::PaginationErrorKind;

/// Default aggregate application-handoff ceiling for one traversal. The
/// trusted builder can select another positive ceiling with
/// `RuntimeConfigBuilder::aggregate_max_sends`; a caller send cap
/// (`CallOptions::cap_sends`) can only narrow the selected ceiling.
pub const DEFAULT_TRAVERSAL_MAX_HANDOFFS: u32 = 2_000;

/// Maximum visible application attempts for one page before aggregate
/// intersection: the initial attempt, at most 16 policy retries and at most one
/// authentication-recovery replay.
pub const MAX_PAGE_ATTEMPTS: u32 = 18;

pub(crate) struct CycleDetector {
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
    pub(crate) fn new(digest: fn(&[u8]) -> u64) -> Self {
        Self {
            buckets: HashMap::new(),
            retained_identity_count: 0,
            retained_identity_bytes: 0,
            digest,
        }
    }

    pub(crate) fn preflight(
        &self,
        identity: &CanonicalIdentity,
        byte_limit: usize,
    ) -> Result<u64, PaginationError> {
        let digest = (self.digest)(identity.as_bytes());
        if self
            .buckets
            .get(&digest)
            .is_some_and(|bucket| bucket.iter().any(|bytes| bytes == identity.as_bytes()))
        {
            return Err(PaginationError::new(PaginationErrorKind::RepeatedRequest));
        }

        let retained_bytes = self
            .retained_identity_bytes
            .checked_add(identity.len())
            .ok_or_else(|| PaginationError::new(PaginationErrorKind::Overflow))?;
        if retained_bytes > byte_limit {
            return Err(PaginationError::new(
                PaginationErrorKind::IdentityStorageCeilingExceeded,
            ));
        }
        Ok(digest)
    }

    pub(crate) fn remaining_identity_bytes(
        &self,
        byte_limit: usize,
    ) -> Result<usize, PaginationError> {
        byte_limit
            .checked_sub(self.retained_identity_bytes)
            .ok_or_else(|| {
                PaginationError::new(PaginationErrorKind::IdentityStorageCeilingExceeded)
            })
    }

    pub(crate) fn retain(
        &mut self,
        identity: CanonicalIdentity,
        digest: u64,
    ) -> Result<(), PaginationError> {
        let retained_bytes = self
            .retained_identity_bytes
            .checked_add(identity.len())
            .ok_or_else(|| PaginationError::new(PaginationErrorKind::Overflow))?;
        let retained_count = self
            .retained_identity_count
            .checked_add(1)
            .ok_or_else(|| PaginationError::new(PaginationErrorKind::Overflow))?;
        self.buckets
            .entry(digest)
            .or_default()
            .push(identity.into_bytes());
        self.retained_identity_count = retained_count;
        self.retained_identity_bytes = retained_bytes;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn check_and_retain(
        &mut self,
        identity: CanonicalIdentity,
        byte_limit: usize,
    ) -> Result<(), PaginationError> {
        let digest = self.preflight(&identity, byte_limit)?;
        self.retain(identity, digest)
    }

    #[cfg(test)]
    pub(crate) fn retained_identity_count(&self) -> usize {
        self.retained_identity_count
    }

    pub(crate) fn retained_identity_bytes(&self) -> usize {
        self.retained_identity_bytes
    }

    #[cfg(test)]
    pub(crate) fn set_retained_identity_bytes(&mut self, bytes: usize) {
        self.retained_identity_bytes = bytes;
    }

    #[cfg(test)]
    pub(crate) fn contains_allocation(&self, pointer: usize) -> bool {
        self.buckets
            .values()
            .flatten()
            .any(|bytes| bytes.as_ptr() as usize == pointer)
    }

    #[cfg(test)]
    pub(crate) fn stored_identity_bytes_sum(&self) -> usize {
        self.buckets.values().flatten().map(Vec::len).sum::<usize>()
    }
}
