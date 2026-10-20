use crate::hooks::BoxFuture;
use crate::http::provider::ProviderHttp;

use super::{AuthError, CredentialMaterial, ProviderOrigin};

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
///
/// It contains the acquisition reason and one isolated, bounded
/// [`ProviderHttp`] capability for this single acquisition or refresh. It does
/// not contain the application client, the application's credential store or
/// any application credential placement.
pub struct CredentialRequest<'a> {
    /// Isolated provider HTTP capability for this acquisition.
    ///
    /// It may reach only the origins returned by
    /// [`CredentialProvider::http_origins`], at most the configured number of
    /// sends, under `min(remaining operation deadline, provider timeout)`.
    /// Its [`ProviderHttp::request_body_limit`] accessor lets a provider check
    /// encoded output against the active runtime ceiling before allocation.
    pub http: ProviderHttp<'a>,
    /// Reason for acquisition or refresh.
    pub reason: CredentialReason,
}

/// Application-supplied or built-in credential provider.
///
/// # Extension contract
///
/// * **Ownership.** The client owns the provider behind an `Arc`; material it
///   returns is moved into the credential store that requested it.
/// * **Concurrency.** Calls for different acquisitions may overlap, but
///   Concord runs at most one acquisition per credential store at a time
///   (single flight): concurrent callers join the current leader instead of
///   starting their own.
/// * **Cancellation.** Each acquisition runs under
///   `min(remaining operation deadline, provider timeout)` (30 seconds by
///   default). Dropping the leader cancels its future and retires the flight
///   so followers re-evaluate; a cancelled follower never cancels the leader.
///   Already completed provider effects are not rolled back. A result that
///   arrives after a later manual `clear`/`replace` superseded the acquisition
///   is dropped, never installed.
/// * **Input sensitivity.** Calls receive sensitive credential material or
///   provider inputs. Implementations own confidentiality in their logging
///   and storage.
/// * **Errors.** Return an [`AuthError`] (a kind, an optional caller-reviewed
///   safe message and an optional retry-after hint). Nothing is installed,
///   the requesting operation fails with a value-free authentication error,
///   and a nonzero retry-after starts a failure cooldown capped by the
///   runtime's `auth_failure_cooldown_cap` (60 seconds by default).
/// * **Resource obligations.** Implementations must bound CPU, memory,
///   retries, maps, queues, spawned tasks, metric cardinality and external
///   I/O. Concord bounds its credential state, waiter count, failure
///   cooldown, provider HTTP sends, bodies and time, but does not
///   automatically bound custom provider state, synchronous work before a
///   future is returned, or application-created background work.
/// * **Negative capabilities.** A provider receives no application client, no
///   application credential store and no application credential placement.
///   Its [`ProviderHttp`] reaches only the exact origins it declares through
///   [`CredentialProvider::http_origins`]; the empty default grants no helper
///   destinations. If a provider owns another HTTP client, Concord cannot
///   sandbox it: that I/O is the application's explicitly trusted code.
#[diagnostic::on_unimplemented(
    message = "[C3-AUTH] `{Self}` is not a credential provider",
    label = "a `custom` credential source requires `CredentialProvider`",
    note = "a custom credential source implements `CredentialProvider` (`Send + Sync + 'static`) and produces `CredentialMaterial`",
    note = "implement `CredentialProvider` for this type, or declare a built-in source such as `api_key(secret.NAME)`, `bearer(secret.NAME)` or `endpoint PATH`"
)]
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

    /// Selects challenge recovery behavior. The default for a custom provider
    /// is [`RecoveryMode::Refresh`].
    fn recovery_mode(&self) -> RecoveryMode {
        RecoveryMode::Refresh
    }

    /// Declares the exact origins this provider may reach through the
    /// provided [`ProviderHttp`] helper.
    ///
    /// Read once when the client is built (or when this provider is installed
    /// by an explicit provider replacement) and validated against the
    /// configured capacity (8 by default, hard maximum 32). Only HTTPS origins
    /// are accepted. The set cannot grow afterwards. The default empty set
    /// grants no helper destinations.
    fn http_origins(&self) -> &[ProviderOrigin] {
        &[]
    }
}
