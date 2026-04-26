use super::{AuthRetryReason, CredentialId};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthPlan {
    pub mode: AuthModePlan,
    pub requirements: &'static [AuthRequirement],
}

impl AuthPlan {
    pub const NONE: Self = Self {
        mode: AuthModePlan::All,
        requirements: &[],
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthModePlan {
    All,
    Any,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthRequirement {
    pub credential: CredentialRef,
    pub placement: AuthPlacement,
    pub challenge: AuthChallengePolicy,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CredentialRef {
    pub id: CredentialId,
}

pub trait CustomAuthPlacement: Send + Sync + 'static {
    fn name(&self) -> &'static str;
}

#[derive(Clone, Copy)]
pub enum AuthPlacement {
    Bearer,
    Header(&'static str),
    Query(&'static str),
    Basic,
    Certificate,
    Custom(&'static dyn CustomAuthPlacement),
}

impl fmt::Debug for AuthPlacement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bearer => f.write_str("Bearer"),
            Self::Header(name) => f.debug_tuple("Header").field(name).finish(),
            Self::Query(name) => f.debug_tuple("Query").field(name).finish(),
            Self::Basic => f.write_str("Basic"),
            Self::Certificate => f.write_str("Certificate"),
            Self::Custom(placement) => f.debug_tuple("Custom").field(&placement.name()).finish(),
        }
    }
}

impl PartialEq for AuthPlacement {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Bearer, Self::Bearer)
            | (Self::Basic, Self::Basic)
            | (Self::Certificate, Self::Certificate) => true,
            (Self::Header(a), Self::Header(b)) | (Self::Query(a), Self::Query(b)) => a == b,
            (Self::Custom(a), Self::Custom(b)) => std::ptr::addr_eq(*a, *b),
            _ => false,
        }
    }
}

impl Eq for AuthPlacement {}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AuthChallengePolicy {
    #[default]
    Default,
    NeverRefresh,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthDecision {
    Continue,
    RetryAfterRefresh {
        credential: CredentialRef,
        generation: Option<u64>,
        reason: AuthRetryReason,
    },
    Fail,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AuthAttemptSummary {
    pub applied_credentials: usize,
    pub refreshed_credentials: usize,
}
