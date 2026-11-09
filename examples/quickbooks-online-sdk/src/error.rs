//! The SDK's errors. No `Display` or `Debug` here prints a token, a client secret, an
//! authorization code, a URL, a provider message or detail, or a response body.

use std::sync::Arc;

use concord::reqwest::StatusCode;
use concord::{Received, ResponseMeta};
use concord_pagination::OffsetError;
use serde::Deserialize;

use crate::{ERROR_BODY_LIMIT, RealmId, RequestId};

/// One `Error` element of a QuickBooks `Fault`.
#[derive(Clone, PartialEq, Eq, Deserialize, concord::Debug)]
pub struct FaultError {
    #[serde(rename = "Message")]
    #[sensitive]
    message: Option<String>,
    #[serde(rename = "Detail")]
    #[sensitive]
    detail: Option<String>,
    code: Option<String>,
    element: Option<String>,
}

impl FaultError {
    /// The error number (`2050`, `6240`, `610`, ...).
    pub fn code(&self) -> Option<&str> {
        self.code.as_deref()
    }

    /// The field the error concerns, when QuickBooks names one.
    pub fn element(&self) -> Option<&str> {
        self.element.as_deref()
    }

    /// The short message. It can quote submitted values, so this SDK never formats it.
    pub fn message_unredacted(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// The detail. It can quote submitted values, so this SDK never formats it.
    pub fn detail_unredacted(&self) -> Option<&str> {
        self.detail.as_deref()
    }
}

/// A QuickBooks `Fault`: its type (`ValidationFault`, `SystemFault`, `AuthenticationFault`,
/// `AuthorizationFault`) and its errors.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Fault {
    #[serde(rename = "Error", default)]
    errors: Vec<FaultError>,
    #[serde(rename = "type")]
    kind: Option<String>,
}

impl Fault {
    /// The fault type.
    pub fn kind(&self) -> Option<&str> {
        self.kind.as_deref()
    }

    /// The errors, in the order QuickBooks listed them.
    pub fn errors(&self) -> &[FaultError] {
        &self.errors
    }

    /// The first error's code.
    pub fn code(&self) -> Option<&str> {
        self.errors.first().and_then(FaultError::code)
    }
}

/// An error document: `{"Fault": {..}, "time": ..}`.
#[derive(Deserialize)]
pub struct FaultResponse {
    #[serde(rename = "Fault")]
    fault: Fault,
}

/// A response QuickBooks answered with a `Fault`: a non-2xx status, or a 2xx whose body is a
/// `Fault` (documented: a 200 may carry one). The status is authoritative whether or not the
/// document could be read.
#[derive(Debug)]
pub struct ProviderError {
    meta: ResponseMeta,
    fault: Result<Fault, concord::Error>,
}

impl ProviderError {
    pub(crate) async fn read<T>(received: Received<T, crate::FaultResponse>) -> Self {
        let (meta, document) = received.failure(ERROR_BODY_LIMIT).await.into_document();
        Self {
            meta,
            fault: document.map(|d| d.fault),
        }
    }

    pub(crate) fn in_success(meta: ResponseMeta, fault: Fault) -> Self {
        Self {
            meta,
            fault: Ok(fault),
        }
    }

    /// The HTTP status (`200` for a fault inside a successful response).
    pub fn status(&self) -> StatusCode {
        self.meta.status()
    }

    /// The fault, or why the body is not one (another media type, a body over
    /// [`ERROR_BODY_LIMIT`], malformed JSON, a broken connection).
    pub fn fault(&self) -> Result<&Fault, &concord::Error> {
        self.fault.as_ref()
    }

    /// The `intuit_tid` response header, which Intuit support asks for.
    pub fn intuit_tid(&self) -> Option<&str> {
        self.meta
            .headers()
            .get("intuit_tid")
            .and_then(|v| v.to_str().ok())
    }

    /// The response head (status, headers, operation); its `Debug` prints header names only.
    pub fn meta(&self) -> &ResponseMeta {
        &self.meta
    }
}

/// What went wrong with authorization. Each case names how to recover.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuthorizationKind {
    /// The token endpoint refused the grant with 400 or 401: the refresh token expired, was
    /// revoked or was replaced by a newer one (OAuth `invalid_grant`), the authorization code is
    /// spent, or the app's credentials are wrong (`invalid_client`). The connection stops asking
    /// for tokens until it is reauthorized ([`Connection::reauthorize`](crate::Connection::reauthorize))
    /// or given a newer refresh token
    /// ([`Connection::replace_refresh_token`](crate::Connection::replace_refresh_token)). Other
    /// realms are unaffected.
    GrantRejected,
    /// The token endpoint gave no usable answer: no response, a 5xx, or a 2xx that could not be
    /// read or violates the token contract. The refresh token may have been rotated; the
    /// connection keeps the latest one it received, and the next call tries again with it.
    GrantUncertain,
    /// The token endpoint refused for another reason (a 429, a 403, ...) without consuming the
    /// grant, or the token request could not be built. Try again later.
    TokenServiceUnavailable,
    /// QuickBooks answered 401 to the API request: on a read, with a token acquired after the
    /// first 401; on a write, at once (a write is never repeated by this SDK). The cached token
    /// is discarded; the next call acquires a new one.
    AccessTokenRejected,
}

/// What the token endpoint said, shared by every request waiting on the same refresh.
#[derive(Debug)]
pub(crate) struct TokenFailure {
    pub(crate) kind: AuthorizationKind,
    pub(crate) status: Option<StatusCode>,
    pub(crate) oauth_error: Option<String>,
    pub(crate) cause: Option<concord::Error>,
    pub(crate) violation: Option<&'static str>,
    /// A refresh token found in an otherwise unusable 2xx, stored before the failure is shared.
    pub(crate) replacement: Option<crate::RefreshToken>,
}

/// A recoverable authorization failure of one realm's connection.
#[derive(Debug)]
pub struct AuthorizationError {
    realm: RealmId,
    kind: AuthorizationKind,
    token: Option<Arc<TokenFailure>>,
    response: Option<Box<ProviderError>>,
}

impl AuthorizationError {
    pub(crate) fn from_token(realm: RealmId, failure: Arc<TokenFailure>) -> Self {
        Self {
            realm,
            kind: failure.kind,
            token: Some(failure),
            response: None,
        }
    }

    pub(crate) fn access_rejected(realm: RealmId, response: ProviderError) -> Self {
        Self {
            realm,
            kind: AuthorizationKind::AccessTokenRejected,
            token: None,
            response: Some(Box::new(response)),
        }
    }

    /// What failed, and so how to recover.
    pub fn kind(&self) -> AuthorizationKind {
        self.kind
    }

    /// The realm whose connection failed. No other realm's credentials were used or changed.
    pub fn realm(&self) -> &RealmId {
        &self.realm
    }

    /// `true` when only a new authorization (or a newer refresh token) recovers the connection.
    pub fn needs_reauthorization(&self) -> bool {
        self.kind == AuthorizationKind::GrantRejected
    }

    /// The token endpoint's HTTP status, when it answered.
    pub fn token_status(&self) -> Option<StatusCode> {
        self.token.as_ref().and_then(|t| t.status)
    }

    /// The token endpoint's OAuth `error` code (`invalid_grant`, `invalid_client`, ...).
    pub fn oauth_error(&self) -> Option<&str> {
        self.token.as_ref().and_then(|t| t.oauth_error.as_deref())
    }

    /// The Concord error behind an uncertain or unavailable token request.
    pub fn cause(&self) -> Option<&concord::Error> {
        self.token.as_ref().and_then(|t| t.cause.as_ref())
    }

    /// The token-contract rule a 2xx token response broke (`token_type` not `bearer`, ...).
    pub fn violation(&self) -> Option<&'static str> {
        self.token.as_ref().and_then(|t| t.violation)
    }

    /// QuickBooks' 401 response, for [`AuthorizationKind::AccessTokenRejected`].
    pub fn response(&self) -> Option<&ProviderError> {
        self.response.as_deref()
    }
}

/// Why a write's outcome is unknown.
#[derive(Debug)]
#[non_exhaustive]
pub enum UncertainCause {
    /// No response head arrived after the request may have been sent (a timeout, a reset, a
    /// refused connection: `concord::Delivery::Unknown`).
    NoResponse(concord::Error),
    /// A 5xx: the server may have committed the write before failing.
    ServerError(Box<ProviderError>),
    /// A 2xx whose head or body could not be read: the write may have happened.
    UnreadableSuccess(concord::Error),
}

/// A write that may or may not have taken effect. This SDK never repeats it.
///
/// Resolve it explicitly: send the **same** invoice again with the same
/// [`RequestId`] (`create(invoice).set_request_id(id)`), and QuickBooks answers with the original
/// response instead of creating a second invoice; or query for it by `DocNumber` first.
#[derive(Debug)]
pub struct UncertainWrite {
    operation: &'static str,
    realm: RealmId,
    request_id: Option<RequestId>,
    cause: UncertainCause,
}

impl UncertainWrite {
    pub(crate) fn new(
        operation: &'static str,
        realm: RealmId,
        request_id: Option<RequestId>,
        cause: UncertainCause,
    ) -> Self {
        Self {
            operation,
            realm,
            request_id,
            cause,
        }
    }

    /// The declared endpoint (`QuickBooks.invoices.create`).
    pub fn operation(&self) -> &'static str {
        self.operation
    }

    /// The realm the write was sent to.
    pub fn realm(&self) -> &RealmId {
        &self.realm
    }

    /// The request ID the write carried; replay with it, never with a new one.
    pub fn request_id(&self) -> Option<&RequestId> {
        self.request_id.as_ref()
    }

    /// What was observed.
    pub fn cause(&self) -> &UncertainCause {
        &self.cause
    }
}

/// A rejected OAuth redirect: nothing was sent.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CallbackError {
    /// The callback's `state` is not the one the authorization request carried.
    StateMismatch,
    /// The callback names another realm than the connection being reauthorized.
    RealmMismatch,
    /// A required parameter (`code`, `state`, `realmId`) is missing or empty.
    Missing(&'static str),
    /// The user or Intuit refused the authorization (`error=access_denied`, `invalid_scope`).
    Denied(String),
}

/// A customer page that cannot be followed safely.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PaginationError {
    /// `startPosition` differs from the requested `STARTPOSITION`.
    StartPositionMismatch,
    /// The page breaks the offset rule (more customers than `MAXRESULTS`, an overflow).
    Offset(OffsetError),
}

/// Errors of this SDK.
#[derive(Debug)]
#[non_exhaustive]
#[derive(concord::derive::Error)]
#[display(with = display_error)]
pub enum Error {
    /// A request that could not be built (nothing sent), a transport failure of a read, or a 2xx
    /// read whose head or body was refused (`Metadata`, `Decode`, `Limit`).
    Concord(#[source] concord::Error),
    /// QuickBooks answered with a `Fault` (a 5xx on a write is [`Error::Uncertain`] instead,
    /// a 401 is [`Error::Authorization`]).
    #[from(skip)]
    Provider(Box<ProviderError>),
    /// A recoverable authorization failure of one realm.
    #[from(skip)]
    Authorization(Box<AuthorizationError>),
    /// A write whose outcome is unknown; see [`UncertainWrite`].
    #[from(skip)]
    Uncertain(Box<UncertainWrite>),
    /// A rejected OAuth redirect.
    #[from(skip)]
    Callback(CallbackError),
    /// A client ID, secret or token that is not a valid credential (empty, controls, ...).
    #[from(skip)]
    Credential(#[source] concord_auth::CredentialError),
    /// An SDK input outside its documented range; nothing was sent.
    InvalidInput {
        /// Which input.
        what: &'static str,
    },
    /// A customer page that cannot be followed.
    #[from(skip)]
    Pagination(PaginationError),
}

impl Error {
    /// The HTTP status of a provider error (or of the 5xx behind an uncertain write).
    pub fn status(&self) -> Option<StatusCode> {
        match self {
            Self::Provider(p) => Some(p.status()),
            Self::Uncertain(w) => match &w.cause {
                UncertainCause::ServerError(p) => Some(p.status()),
                _ => None,
            },
            Self::Concord(e) => e.status(),
            _ => None,
        }
    }

    /// The provider error, if this is one.
    pub fn provider(&self) -> Option<&ProviderError> {
        match self {
            Self::Provider(p) => Some(p),
            _ => None,
        }
    }

    /// The authorization failure, if this is one.
    pub fn authorization(&self) -> Option<&AuthorizationError> {
        match self {
            Self::Authorization(a) => Some(a),
            _ => None,
        }
    }

    /// The uncertain write, if this is one.
    pub fn uncertain(&self) -> Option<&UncertainWrite> {
        match self {
            Self::Uncertain(w) => Some(w),
            _ => None,
        }
    }
}

fn display_error(value: &Error, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match value {
        Error::Concord(e) => std::fmt::Display::fmt(e, f),
        Error::Provider(p) => {
            write!(f, "QuickBooks answered {} with a fault", p.status())?;
            if let Ok(fault) = p.fault()
                && let Some(kind) = fault.kind()
            {
                write!(f, " ({kind}")?;
                if let Some(code) = fault.code() {
                    write!(f, " {code}")?;
                }
                f.write_str(")")?;
            }
            Ok(())
        }
        Error::Authorization(a) => {
            write!(f, "authorization failed for realm {}: ", a.realm)?;
            f.write_str(match a.kind {
                AuthorizationKind::GrantRejected => {
                    "the token endpoint rejected the grant; reauthorize this realm"
                }
                AuthorizationKind::GrantUncertain => {
                    "the token refresh had no usable answer; the next call tries again"
                }
                AuthorizationKind::TokenServiceUnavailable => {
                    "the token endpoint is unavailable; try again later"
                }
                AuthorizationKind::AccessTokenRejected => {
                    "QuickBooks rejected the access token (401)"
                }
            })
        }
        Error::Uncertain(w) => {
            write!(f, "the outcome of `{}` is unknown: ", w.operation)?;
            f.write_str(match w.cause {
                UncertainCause::NoResponse(_) => "no response arrived",
                UncertainCause::ServerError(_) => "the server failed with a 5xx",
                UncertainCause::UnreadableSuccess(_) => "the success response was unreadable",
            })?;
            f.write_str("; replay it with the same request ID, never with a new one")
        }
        Error::Callback(c) => f.write_str(match c {
            CallbackError::StateMismatch => "the OAuth callback state does not match",
            CallbackError::RealmMismatch => {
                "the OAuth callback is for another realm than this connection"
            }
            CallbackError::Missing(_) => "the OAuth callback lacks a required parameter",
            CallbackError::Denied(_) => "the authorization was refused",
        }),
        Error::Credential(e) => write!(f, "invalid credential: {e}"),
        Error::InvalidInput { what } => write!(f, "invalid {what}"),
        Error::Pagination(kind) => match kind {
            PaginationError::StartPositionMismatch => {
                f.write_str("a customer page reports another start position")
            }
            PaginationError::Offset(e) => write!(f, "a customer page cannot be followed: {e}"),
        },
    }
}
