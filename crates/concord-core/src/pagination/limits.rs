use std::collections::HashMap;

use super::PaginationError;
use super::identity::{CanonicalIdentity, deterministic_digest};
use crate::error::PaginationErrorKind;

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
    ) -> Result<(u64, usize), PaginationError> {
        let digest = (self.digest)(identity.bytes());
        if self
            .buckets
            .get(&digest)
            .is_some_and(|bucket| bucket.iter().any(|bytes| bytes == identity.bytes()))
        {
            return Err(PaginationError::new(PaginationErrorKind::RepeatedRequest));
        }

        let retained_bytes = self
            .retained_identity_bytes
            .checked_add(identity.retained_byte_len())
            .ok_or_else(|| PaginationError::new(PaginationErrorKind::Overflow))?;
        if retained_bytes > byte_limit {
            return Err(PaginationError::new(
                PaginationErrorKind::IdentityStorageCeilingExceeded,
            ));
        }
        Ok((digest, retained_bytes))
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
        retained_bytes: usize,
    ) -> Result<(), PaginationError> {
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
        let (digest, retained_bytes) = self.preflight(&identity, byte_limit)?;
        self.retain(identity, digest, retained_bytes)
    }

    #[cfg(test)]
    pub(crate) fn retained_identity_count(&self) -> usize {
        self.retained_identity_count
    }

    #[cfg(test)]
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
}
