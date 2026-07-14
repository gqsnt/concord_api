//! Explicitly enabled deterministic-development observation support.
//!
//! This module is unstable, is absent unless `dangerous-dev-tools` is
//! selected, and is not a runtime-engine or generated-integration API. The
//! deterministic executor is test infrastructure only: Reqwest remains the
//! sole executor in ordinary builds and generated clients never depend on it.

pub use crate::development_executor::{
    CapturedBodyCategory, CapturedNativeRequest, DeterministicBodyGate, DeterministicExecutionKind,
    DeterministicExecutorInstallationError, DeterministicFakeCredential,
    DeterministicNativeExecutor, RequestBodyTerminalObservation, ScriptedNativeResponse,
    ScriptedResponseBodyStep, SyntheticExecutionFailure, UnsafeCredentialPlacementExpectations,
    UnsafeDeterministicFakeBody, UnsafeRequestBodyExpectations, configure_application_executor,
    configure_provider_executor, install_application_executor, install_provider_executor,
};

/// Authentication lifecycle observations used to verify deterministic
/// challenge ordering without exposing credential storage or execution types.
pub use crate::auth::{CredentialGenerationSnapshot, CredentialLifecycleEvent};

pub fn observe_credential_provider_state<Cx, P>(
    state: &crate::auth::CredentialProviderState<Cx, P>,
    observer: std::sync::Arc<dyn Fn(CredentialLifecycleEvent) + Send + Sync>,
) where
    Cx: crate::client::ClientContext,
    P: crate::auth::CredentialProvider<Cx>,
{
    state.install_lifecycle_observer(observer);
}

/// Opaque identity for one cached credential generation.
///
/// The value can only be compared for equality; generation numbers and cache
/// contents remain private.
/// Observe whether a provider still owns the same cached generation.
pub async fn credential_generation_snapshot<Cx, P>(
    state: &crate::auth::CredentialProviderState<Cx, P>,
) -> Option<CredentialGenerationSnapshot>
where
    Cx: crate::client::ClientContext,
    P: crate::auth::CredentialProvider<Cx>,
{
    state.generation_snapshot().await
}

#[cfg(test)]
mod tests {
    use super::{CredentialGenerationSnapshot, CredentialLifecycleEvent};

    #[test]
    fn generation_identity_is_comparable_but_diagnostics_are_opaque() {
        let first = CredentialGenerationSnapshot::from_generation(8_675_309);
        let same = CredentialGenerationSnapshot::from_generation(8_675_309);
        let replacement = CredentialGenerationSnapshot::from_generation(8_675_310);

        assert_eq!(first, same);
        assert_ne!(first, replacement);

        let event = CredentialLifecycleEvent::GenerationInvalidated {
            requested: Some(first.clone()),
            current: Some(same),
            applied: true,
        };
        for diagnostic in [
            format!("{first:?}"),
            first.to_string(),
            format!("{event:?}"),
        ] {
            assert!(!diagnostic.contains("8675309"), "{diagnostic}");
            assert!(!diagnostic.contains("8675310"), "{diagnostic}");
        }
        assert_eq!(
            format!("{first:?}"),
            "CredentialGenerationSnapshot(<opaque>)"
        );
        assert_eq!(first.to_string(), "<opaque credential generation>");
    }

    #[test]
    fn development_surface_stays_observation_only() {
        let source = include_str!("__development.rs");
        for forbidden in [
            concat!("Credential", "Slot"),
            concat!("AuthApplication", "Request"),
            concat!("AuthApplied", "Credential"),
            concat!("AuthRejection", "Action"),
            concat!("Auth", "Requirement"),
            concat!("Dyn", "Body"),
            concat!("Limited", "Body"),
            concat!("Transport", "Error"),
            concat!("Response", "Entity"),
            concat!("Request", "Entity"),
        ] {
            assert!(
                !source.contains(forbidden),
                "development surface exposed {forbidden}"
            );
        }
    }
}
