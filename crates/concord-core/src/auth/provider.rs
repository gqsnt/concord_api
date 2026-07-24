use crate::hooks::BoxFuture;
use crate::http::provider::ProviderHttp;

use super::{AuthError, CredentialMaterial};

/// Recovery behavior selected by one credential provider.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryMode {
    /// A challenge does not permit provider recovery.
    None,
    /// A challenge invalidates current material without refreshing it.
    InvalidateOnly,
    /// A challenge may refresh exact-current material.
    Refresh,
}

/// Why Concord is asking a provider for material.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialReason {
    /// No material is currently stored.
    Missing,
    /// Current material has expired.
    Expired,
    /// Current material is within the proactive refresh skew.
    ExpiringSoon,
    /// The protected server rejected exact-current material.
    Rejected,
}

/// Bounded context supplied to a credential provider.
pub struct CredentialRequest<'a> {
    /// Isolated provider HTTP capability for this operation.
    ///
    /// Its [`ProviderHttp::request_body_limit`] accessor lets a provider check
    /// encoded output against the active runtime ceiling before allocation.
    pub http: ProviderHttp<'a>,
    /// Reason for acquisition or refresh.
    pub reason: CredentialReason,
}

/// Application-supplied or built-in credential provider.
///
/// Calls may overlap and receive sensitive credential material or provider
/// inputs. Implementations own confidentiality in their logging and storage and
/// must bound CPU, memory, retries, maps, queues, spawned tasks, metric
/// cardinality, and external I/O. Provider futures run under the operation
/// deadline and are dropped on cancellation; already completed provider effects
/// are not rolled back. Concord bounds its credential state and provider HTTP
/// bodies, but does not automatically bound custom provider state, synchronous
/// work before a future is returned, or application-created background work.
pub trait CredentialProvider: Send + Sync + 'static {
    /// Material produced by this provider.
    type Material: CredentialMaterial;

    /// Acquires material when none is usable.
    fn acquire<'a>(
        &'a self,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>>;

    /// Refreshes exact-current material after expiration or rejection.
    fn refresh<'a>(
        &'a self,
        _current: &'a Self::Material,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        self.acquire(context)
    }

    /// Selects challenge recovery behavior.
    fn recovery_mode(&self) -> RecoveryMode {
        RecoveryMode::Refresh
    }
}
