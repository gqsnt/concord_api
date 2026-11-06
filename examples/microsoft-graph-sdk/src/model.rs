//! Selected Graph properties and lossless sparse delta changes.
use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer};
use serde_json::Value;

/// A delta property distinguishes an unreported value from an explicit clearing to null.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Field<T> {
    /// The property was absent; keep the locally stored value.
    #[default]
    Missing,
    /// The property was explicitly null.
    Null,
    /// A reported value, including an empty string or false.
    Value(T),
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Field<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match Option::<T>::deserialize(d)? {
            Some(v) => Self::Value(v),
            None => Self::Null,
        })
    }
}

/// A user change; absence and null remain distinct even on incremental responses.
/// Changes may repeat and arrive in any order. Applications own merging and publication.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UserDelta {
    /// Directory identifier; key changes by this value, never by display name.
    pub id: String,
    /// Selected display name.
    #[serde(default)]
    pub display_name: Field<String>,
    /// Selected given name.
    #[serde(default)]
    pub given_name: Field<String>,
    /// Selected surname.
    #[serde(default)]
    pub surname: Field<String>,
    /// Selected mail address; null is a valid value.
    #[serde(default)]
    pub mail: Field<String>,
    /// Selected sign-in name.
    #[serde(default)]
    pub user_principal_name: Field<String>,
    /// Selected account-enabled flag.
    #[serde(default)]
    pub account_enabled: Field<bool>,
    /// Removal marker; both `changed` (soft delete) and `deleted` remove the local user record.
    #[serde(rename = "@removed")]
    pub removed: Option<Removed>,
    /// Other properties and annotations, unchanged, including unknown object types.
    #[serde(flatten)]
    pub additional: BTreeMap<String, Value>,
}

/// The provider's removal reason (unknown future reasons remain available).
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Removed {
    /// Typically `changed` or `deleted`; not inferred from any other property.
    pub reason: String,
}

/// A group summary; omitted and limited properties are optional.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    /// Directory identifier.
    pub id: String,
    /// Display name, if visible and selected.
    pub display_name: Option<String>,
    /// Mail address, if present.
    pub mail: Option<String>,
    /// Whether mail is enabled, when returned.
    pub mail_enabled: Option<bool>,
    /// Whether security is enabled, when returned.
    pub security_enabled: Option<bool>,
    /// Group types, when returned.
    pub group_types: Option<Vec<String>>,
    /// Other returned properties, without discarding unknown fields.
    #[serde(flatten)]
    pub additional: BTreeMap<String, Value>,
}

/// A direct member. The type and id survive permission-limited responses.
/// Unknown types and id/type-only objects remain ordinary members; classify with `kind()`.
/// Missing profile fields alone do not establish a permission level.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryObject {
    /// Directory identifier.
    pub id: String,
    /// Exact provider type, including any unknown future type.
    #[serde(rename = "@odata.type")]
    pub object_type: String,
    /// Visible/selected name; may be absent or null.
    pub display_name: Option<String>,
    /// Visible/selected mail; may be absent or null.
    pub mail: Option<String>,
    /// All remaining properties, including explicit nulls.
    #[serde(flatten)]
    pub additional: BTreeMap<String, Value>,
}

/// Known directory object kinds. Unrecognized types are retained verbatim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectKind<'a> {
    /// User object.
    User,
    /// Group object (not traversed).
    Group,
    /// Device object.
    Device,
    /// Organizational contact.
    OrganizationalContact,
    /// Service principal; the v1.0 endpoint has a documented omission of these objects.
    ServicePrincipal,
    /// An unrecognized provider type string.
    Other(&'a str),
}

impl DirectoryObject {
    /// Classifies the exact `@odata.type`; never guesses from profile fields.
    pub fn kind(&self) -> ObjectKind<'_> {
        match self.object_type.as_str() {
            "#microsoft.graph.user" => ObjectKind::User,
            "#microsoft.graph.group" => ObjectKind::Group,
            "#microsoft.graph.device" => ObjectKind::Device,
            "#microsoft.graph.orgContact" => ObjectKind::OrganizationalContact,
            "#microsoft.graph.servicePrincipal" => ObjectKind::ServicePrincipal,
            other => ObjectKind::Other(other),
        }
    }
}

macro_rules! fields {
    ($name:ident, $doc:literal, $($variant:ident => $wire:literal),+ $(,)?) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name {
            $(#[doc = concat!("Selects `", $wire, "`.")] $variant),+
        }
        impl $name {
            pub(crate) fn wire(self) -> &'static str {
                match self { $(Self::$variant => $wire),+ }
            }
        }
    }
}
fields!(UserField, "Supported selections for users delta.",
    Id => "id", DisplayName => "displayName", GivenName => "givenName",
    Surname => "surname", Mail => "mail", UserPrincipalName => "userPrincipalName",
    AccountEnabled => "accountEnabled");
fields!(GroupField, "Supported selections for group summaries.",
    Id => "id", DisplayName => "displayName", Mail => "mail", MailEnabled => "mailEnabled",
    SecurityEnabled => "securityEnabled", GroupTypes => "groupTypes");
fields!(MemberField, "Common properties selectable across direct member types.",
    Id => "id", DisplayName => "displayName");

#[derive(Deserialize)]
pub(crate) struct Envelope<T> {
    pub value: Vec<T>,
    #[serde(rename = "@odata.nextLink")]
    pub next: Option<String>,
    #[serde(rename = "@odata.deltaLink")]
    pub delta: Option<String>,
}
