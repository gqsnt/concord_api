use super::model::{
    MaterialIdentity, ModelError, PlacementIdentity, Slot, SlotIdentity, SourceKind, Version,
    WaiterIdentity,
};
use super::transition::PrepareDecision;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ChallengePolicy {
    Never,
    Unauthorized,
    UnauthorizedOrForbidden,
}

impl ChallengePolicy {
    pub(super) fn classifies(self, status: u16) -> bool {
        match self {
            Self::Never => false,
            Self::Unauthorized => status == 401,
            Self::UnauthorizedOrForbidden => matches!(status, 401 | 403),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PolicySetting {
    Omitted,
    Explicit(ChallengePolicy),
}

pub(super) fn resolve_policy(
    source: SourceKind,
    setting: PolicySetting,
) -> Result<ChallengePolicy, ModelError> {
    match setting {
        PolicySetting::Explicit(policy) => Ok(policy),
        PolicySetting::Omitted => match source {
            SourceKind::RefreshableBearer => Ok(ChallengePolicy::Unauthorized),
            SourceKind::StaticApiKey
            | SourceKind::StaticBearer
            | SourceKind::StaticBasic
            | SourceKind::EndpointBacked => Ok(ChallengePolicy::Never),
            SourceKind::CustomProvider => Err(ModelError::InvalidPolicy),
        },
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Placement {
    pub(super) identity: PlacementIdentity,
    pub(super) slot: SlotIdentity,
    pub(super) source: SourceKind,
    pub(super) setting: PolicySetting,
}

impl Placement {
    pub(super) fn resolved_policy(self) -> Result<ChallengePolicy, ModelError> {
        resolve_policy(self.source, self.setting)
    }

    pub(super) fn explicitly_challenge_enabled(self) -> bool {
        matches!(
            self.setting,
            PolicySetting::Explicit(
                ChallengePolicy::Unauthorized | ChallengePolicy::UnauthorizedOrForbidden
            )
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct AuthPlan {
    pub(super) placements: Vec<Placement>,
}

impl AuthPlan {
    pub(super) fn validate(&self) -> Result<(), ModelError> {
        let mut enabled = 0_u8;
        for placement in &self.placements {
            if placement.resolved_policy()? != ChallengePolicy::Never {
                enabled = enabled.checked_add(1).ok_or(ModelError::Overflow)?;
                if enabled > 1 {
                    return Err(ModelError::InvalidPlan);
                }
            }
        }
        Ok(())
    }

    pub(super) fn attributed(&self, status: u16) -> Result<Option<Placement>, ModelError> {
        self.validate()?;
        Ok(self.placements.iter().copied().find(|placement| {
            placement
                .resolved_policy()
                .is_ok_and(|p| p.classifies(status))
        }))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct AppliedCredential {
    pub(super) placement: PlacementIdentity,
    pub(super) slot: SlotIdentity,
    pub(super) version: Version,
    pub(super) material: MaterialIdentity,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ChallengeAction {
    None,
    StaticRejected,
    EndpointCleared,
    Refresh(PrepareDecision),
}

pub(super) fn apply_attributed_challenge(
    plan: &AuthPlan,
    status: u16,
    applied: AppliedCredential,
    slots: &mut [Slot],
    waiter: WaiterIdentity,
) -> Result<ChallengeAction, ModelError> {
    let Some(placement) = plan.attributed(status)? else {
        return Ok(ChallengeAction::None);
    };
    if placement.identity != applied.placement || placement.slot != applied.slot {
        return Ok(ChallengeAction::None);
    }
    let Some(slot) = slots.iter_mut().find(|slot| slot.identity == applied.slot) else {
        return Ok(ChallengeAction::None);
    };
    if slot.source_kind.is_static() {
        if !placement.explicitly_challenge_enabled() {
            return Ok(ChallengeAction::None);
        }
        return Ok(if slot.reject_exact(applied.version, true) {
            ChallengeAction::StaticRejected
        } else {
            ChallengeAction::None
        });
    }
    match slot.source_kind {
        SourceKind::EndpointBacked => Ok(if slot.reject_exact(applied.version, false) {
            ChallengeAction::EndpointCleared
        } else {
            ChallengeAction::None
        }),
        SourceKind::RefreshableBearer => slot
            .begin_exact_refresh(applied.version, waiter)
            .map(ChallengeAction::Refresh),
        SourceKind::CustomProvider => slot
            .begin_exact_refresh(applied.version, waiter)
            .map(ChallengeAction::Refresh),
        SourceKind::StaticApiKey | SourceKind::StaticBearer | SourceKind::StaticBasic => {
            Ok(ChallengeAction::None)
        }
    }
}
