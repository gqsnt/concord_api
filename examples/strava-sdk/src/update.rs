//! Partial edits of an owned activity (`PUT /activities/{id}`, `UpdatableActivity`).
use crate::Activity;
use serde::Serialize;
use std::fmt;

/// The fields a caller means to change. Only fields set here are sent; every other field of
/// the activity is left as it is. Debug names the set fields and withholds their values.
#[derive(Clone, Default, Serialize)]
pub struct ActivityUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sport_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gear_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trainer: Option<bool>,
    // The reference marks the top-level `commute` as legacy: commute is set through tags, and
    // the two must not be combined. The SDK only ever sends the tag.
    #[serde(skip_serializing_if = "Option::is_none")]
    tags: Option<Tags>,
}
#[derive(Clone, Serialize)]
struct Tags {
    #[serde(rename = "Commute")]
    commute: bool,
}
/// Strava's documented value of `gear_id` that removes the activity's gear.
const NO_GEAR: &str = "none";

impl ActivityUpdate {
    /// An update that changes nothing yet.
    pub fn new() -> Self {
        Self::default()
    }
    /// New activity title.
    pub fn name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }
    /// New case-sensitive sport name (Strava's `SportType`, sent as given; `type` is never sent).
    pub fn sport_type(mut self, sport: String) -> Self {
        self.sport_type = Some(sport);
        self
    }
    /// New description.
    pub fn description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }
    /// Associate the athlete's gear with this identifier.
    pub fn gear_id(mut self, gear: String) -> Self {
        self.gear_id = Some(gear);
        self
    }
    /// Remove the activity's gear (Strava's `gear_id` value `none`).
    pub fn clear_gear(self) -> Self {
        self.gear_id(NO_GEAR.to_owned())
    }
    /// Mark or unmark the activity as a commute (sent as the `Commute` tag).
    pub fn commute(mut self, commute: bool) -> Self {
        self.tags = Some(Tags { commute });
        self
    }
    /// Mark or unmark the activity as recorded on a training machine.
    pub fn trainer(mut self, trainer: bool) -> Self {
        self.trainer = Some(trainer);
        self
    }
    /// Whether `activity` shows every field this update sets, for example after re-reading an
    /// activity whose update was uncertain. A `false` can also mean a later edit or a provider
    /// normalization; the SDK never resends an update on the caller's behalf.
    pub fn is_reflected_in(&self, activity: &Activity) -> bool {
        let gear = self.gear_id.as_deref().map(|g| (g != NO_GEAR).then_some(g));
        self.name.as_ref().is_none_or(|n| *n == activity.name)
            && self
                .sport_type
                .as_ref()
                .is_none_or(|s| *s == activity.sport_type)
            && self
                .description
                .as_ref()
                .is_none_or(|d| activity.description.as_ref() == Some(d))
            && gear.is_none_or(|g| activity.gear_id.as_deref() == g)
            && self.trainer.is_none_or(|t| activity.trainer == Some(t))
            && self
                .tags
                .as_ref()
                .is_none_or(|t| activity.commute == Some(t.commute))
    }
}
impl fmt::Debug for ActivityUpdate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let set = [
            ("name", self.name.is_some()),
            ("sport_type", self.sport_type.is_some()),
            ("description", self.description.is_some()),
            ("gear_id", self.gear_id.is_some()),
            ("trainer", self.trainer.is_some()),
            ("commute", self.tags.is_some()),
        ];
        f.write_str("ActivityUpdate")?;
        f.debug_list()
            .entries(set.iter().filter(|(_, on)| *on).map(|(name, _)| name))
            .finish()
    }
}
