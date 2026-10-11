//! Source-aware challenge recovery capabilities (AUTH-01, RETRY-02).
//!
//! Generated documentation must not advertise automatic authentication
//! recovery for a credential source that cannot perform it. The static
//! helpers here let the macro and inspection tools describe, from the source
//! kind and endpoint effect alone, what an attributed challenge can do.

use crate::endpoint::Effect;
use crate::inspection::CredentialSourceKind;

/// What an attributed authentication challenge can do for one credential
/// source kind.
///
/// This is a static capability, not a runtime promise: a provider's runtime
/// [`RecoveryMode`](crate::contracts::RecoveryMode) may still decline refresh,
/// and an application replay additionally requires every effect, body, cap,
/// deadline and rate-admission gate of the managed executor.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ChallengeRecovery {
    /// Static API-key, bearer or Basic material. A challenge is final for the
    /// call and rejects exactly the applied material for future calls until
    /// the application replaces it. Nothing refreshes and nothing is replayed.
    FinalRejectsAppliedMaterial,
    /// Endpoint-backed material. A challenge is final for the call and
    /// rejects exactly the installed version applied to that attempt. The
    /// source (login) endpoint is **never** invoked implicitly, and pending
    /// explicit installation intents remain eligible.
    FinalRejectsInstalledVersion,
    /// A custom provider. The provider may invalidate or refresh under its
    /// declared recovery mode; at most one authentication recovery replay per
    /// direct request or page may follow when every executor gate permits.
    ProviderRecoveryMode,
}

impl CredentialSourceKind {
    /// Returns the static challenge capability of this source kind.
    #[must_use]
    pub const fn challenge_recovery(self) -> ChallengeRecovery {
        match self {
            Self::ApiKey | Self::Bearer | Self::Basic => {
                ChallengeRecovery::FinalRejectsAppliedMaterial
            }
            Self::Endpoint => ChallengeRecovery::FinalRejectsInstalledVersion,
            Self::CustomProvider => ChallengeRecovery::ProviderRecoveryMode,
        }
    }

    /// Returns whether an attributed challenge on an endpoint with `effect`
    /// can ever lead to an automatic authentication recovery replay.
    ///
    /// Only provider sources can recover, and a non-idempotent operation that
    /// received a response is never resubmitted automatically: a 401 alone is
    /// not proof that the write had no effect. A `true` result is still
    /// conditional on the provider's runtime recovery mode, a replayable
    /// body, remaining send/replay caps and deadline, and new rate admission.
    #[must_use]
    pub const fn may_replay_after_challenge(self, effect: Effect) -> bool {
        matches!(self, Self::CustomProvider) && !matches!(effect, Effect::NonIdempotent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_providers_on_safe_or_idempotent_endpoints_may_replay() {
        for kind in [
            CredentialSourceKind::ApiKey,
            CredentialSourceKind::Bearer,
            CredentialSourceKind::Basic,
        ] {
            assert_eq!(
                kind.challenge_recovery(),
                ChallengeRecovery::FinalRejectsAppliedMaterial
            );
            for effect in [Effect::Safe, Effect::Idempotent, Effect::NonIdempotent] {
                assert!(!kind.may_replay_after_challenge(effect));
            }
        }
        assert_eq!(
            CredentialSourceKind::Endpoint.challenge_recovery(),
            ChallengeRecovery::FinalRejectsInstalledVersion
        );
        assert!(!CredentialSourceKind::Endpoint.may_replay_after_challenge(Effect::Safe));
        let provider = CredentialSourceKind::CustomProvider;
        assert_eq!(
            provider.challenge_recovery(),
            ChallengeRecovery::ProviderRecoveryMode
        );
        assert!(provider.may_replay_after_challenge(Effect::Safe));
        assert!(provider.may_replay_after_challenge(Effect::Idempotent));
        assert!(!provider.may_replay_after_challenge(Effect::NonIdempotent));
    }
}
