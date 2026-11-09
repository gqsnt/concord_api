//! Typed REST documents: query envelopes, the selected Account/Contact schema (standard,
//! optional and custom fields), dynamic records, sObject describe and error documents.
//!
//! Field names are the API names Salesforce returns (`Id`, `Name`, `Loyalty_Tier__c`). A field
//! the typed schema does not know stays dynamic in the record's `extra` map; a typed optional
//! field distinguishes "not selected" from `null` ([`Field`]).

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Deserializer};

use crate::ids::{FieldName, RecordId, SObjectName};

/// A dynamic field value (JSON as Salesforce returned it).
pub type Value = serde_json::Value;

/// Fields a typed schema does not model, by API name, kept dynamic.
pub type DynamicFields = BTreeMap<String, Value>;

/// The `attributes` object of every REST record: its type and its URL
/// (`/services/data/v68.0/sobjects/Contact/003...`). The URL is a returned link: validate it
/// with [`crate::OrgContext::record_link`] before use.
#[derive(Clone, Deserialize)]
pub struct Attributes {
    /// The sObject type.
    #[serde(rename = "type")]
    pub sobject: SObjectName,
    /// The record's resource URL, as returned (unvalidated).
    #[serde(default)]
    pub url: Option<String>,
}

impl fmt::Debug for Attributes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Attributes")
            .field("sobject", &self.sobject)
            .field("url", &self.url.as_ref().map(|_| ".."))
            .finish()
    }
}

/// A selected field that may be absent from a response: not selected (absent), selected and
/// `null`, or selected with a value. Absent (`#[serde(default)]`) is [`Field::NotSelected`].
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Field<T> {
    /// The field was not in the response (not selected, or not visible to the user).
    #[default]
    NotSelected,
    /// Selected, and `null`.
    Null,
    /// Selected, with a value.
    Value(T),
}

impl<T> Field<T> {
    /// The value, if any.
    pub fn value(&self) -> Option<&T> {
        match self {
            Field::Value(v) => Some(v),
            _ => None,
        }
    }

    /// Whether the response carried the field (as a value or `null`).
    pub fn is_selected(&self) -> bool {
        !matches!(self, Field::NotSelected)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Field<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match Option::<T>::deserialize(deserializer)? {
            Some(v) => Field::Value(v),
            None => Field::Null,
        })
    }
}

/// An Account of the selected schema. `Id` and `Name` are required; `Industry` and
/// `AnnualRevenue` are optional standard fields; `Customer_Tier__c` is an optional custom field
/// that an org may not have. Every other returned field stays dynamic in `extra`.
#[derive(Clone, Deserialize)]
pub struct Account {
    /// `attributes`.
    pub attributes: Attributes,
    /// `Id`.
    #[serde(rename = "Id")]
    pub id: RecordId,
    /// `Name`.
    #[serde(rename = "Name")]
    pub name: String,
    /// `Industry` (picklist).
    #[serde(rename = "Industry", default)]
    pub industry: Field<String>,
    /// `AnnualRevenue` (currency).
    #[serde(rename = "AnnualRevenue", default)]
    pub annual_revenue: Field<f64>,
    /// `Customer_Tier__c` (custom picklist; optional in the selected schema).
    #[serde(rename = "Customer_Tier__c", default)]
    pub customer_tier: Field<String>,
    /// Fields outside the typed schema.
    #[serde(flatten)]
    pub extra: DynamicFields,
}

/// A Contact of the selected schema: `Id` and `LastName` required; `FirstName`, `Email`,
/// `AccountId` optional; `Loyalty_Tier__c` an optional custom field. `Account.Name` (a
/// relationship path) arrives as a nested object and stays dynamic in `extra["Account"]`.
#[derive(Clone, Deserialize)]
pub struct Contact {
    /// `attributes`.
    pub attributes: Attributes,
    /// `Id`.
    #[serde(rename = "Id")]
    pub id: RecordId,
    /// `FirstName`.
    #[serde(rename = "FirstName", default)]
    pub first_name: Field<String>,
    /// `LastName`.
    #[serde(rename = "LastName")]
    pub last_name: String,
    /// `Email`.
    #[serde(rename = "Email", default)]
    pub email: Field<String>,
    /// `AccountId`.
    #[serde(rename = "AccountId", default)]
    pub account_id: Field<RecordId>,
    /// `Loyalty_Tier__c` (custom; optional in the selected schema).
    #[serde(rename = "Loyalty_Tier__c", default)]
    pub loyalty_tier: Field<String>,
    /// Fields outside the typed schema.
    #[serde(flatten)]
    pub extra: DynamicFields,
}

macro_rules! redacted_record_debug {
    ($name:ident) => {
        impl fmt::Debug for $name {
            /// The type and id only: field values are record contents.
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_struct(stringify!($name))
                    .field("id", &self.id)
                    .finish_non_exhaustive()
            }
        }
    };
}

redacted_record_debug!(Account);
redacted_record_debug!(Contact);

/// A record of any object, every field dynamic.
#[derive(Clone, Deserialize)]
pub struct DynamicRecord {
    /// `attributes`.
    pub attributes: Attributes,
    /// Every other field.
    #[serde(flatten)]
    pub fields: DynamicFields,
}

impl fmt::Debug for DynamicRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DynamicRecord")
            .field("sobject", &self.attributes.sobject)
            .field("fields", &self.fields.len())
            .finish()
    }
}

/// The REST query envelope (`GET query?q=` and `GET query/{locator}`).
///
/// `totalSize` is metadata only: it never selects or synthesizes a locator.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResponse<T> {
    /// The total number of records the query matches.
    pub total_size: u64,
    /// Whether this is the last batch.
    pub done: bool,
    /// The next batch's URL (`/services/data/v68.0/query/{locator}`), present while `done` is
    /// false. A returned link: the SDK validates it before use.
    #[serde(default)]
    pub next_records_url: Option<String>,
    /// The batch's records.
    pub records: Vec<T>,
}

impl<T> fmt::Debug for QueryResponse<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QueryResponse")
            .field("total_size", &self.total_size)
            .field("done", &self.done)
            .field("next", &self.next_records_url.as_ref().map(|_| ".."))
            .field("records", &self.records.len())
            .finish()
    }
}

/// The sObject Describe document, reduced to what a schema-aware export needs; every other
/// property stays dynamic in `extra`.
#[derive(Clone, Debug, Deserialize)]
pub struct SObjectDescribe {
    /// The object's API name.
    pub name: SObjectName,
    /// The label.
    #[serde(default)]
    pub label: Option<String>,
    /// Whether the object is custom.
    #[serde(default)]
    pub custom: bool,
    /// Whether the user can query it.
    #[serde(default = "yes")]
    pub queryable: bool,
    /// The record id prefix (`001`).
    #[serde(rename = "keyPrefix", default)]
    pub key_prefix: Option<String>,
    /// The fields.
    pub fields: Vec<FieldDescribe>,
    /// Other properties (urls, childRelationships, ...).
    #[serde(flatten)]
    pub extra: DynamicFields,
}

fn yes() -> bool {
    true
}

impl SObjectDescribe {
    /// The field with this API name (case-insensitive, as Salesforce API names are).
    pub fn field(&self, name: &FieldName) -> Option<&FieldDescribe> {
        self.fields
            .iter()
            .find(|f| f.name.as_str().eq_ignore_ascii_case(name.as_str()))
    }
}

/// One field of a describe document.
#[derive(Clone, Debug, Deserialize)]
pub struct FieldDescribe {
    /// The API name.
    pub name: FieldName,
    /// The field type (`id`, `string`, `picklist`, `reference`, `currency`, ...), kept as text:
    /// Salesforce adds types over versions.
    #[serde(rename = "type")]
    pub field_type: String,
    /// The label.
    #[serde(default)]
    pub label: Option<String>,
    /// Whether the field can be null.
    #[serde(default)]
    pub nillable: bool,
    /// Whether it is a custom field.
    #[serde(default)]
    pub custom: bool,
    /// The maximum length for text fields.
    #[serde(default)]
    pub length: Option<u32>,
    /// For a reference: the relationship name (`Account` for `AccountId`).
    #[serde(rename = "relationshipName", default)]
    pub relationship_name: Option<String>,
    /// Other properties.
    #[serde(flatten)]
    pub extra: DynamicFields,
}

/// One entry of a Salesforce error document (`[{"message": .., "errorCode": .., "fields": ..}]`).
///
/// `Debug` shows the code and the fields; the message is withheld (it can quote record values or
/// ids) and read deliberately with [`message_unredacted`](Self::message_unredacted).
#[derive(Clone, Deserialize, PartialEq, Eq)]
pub struct ApiErrorDoc {
    /// The error code (`INVALID_QUERY_LOCATOR`, `NOT_FOUND`, `INVALID_SESSION_ID`, ...).
    #[serde(rename = "errorCode")]
    pub error_code: String,
    #[serde(default)]
    message: String,
    /// The fields concerned, if any.
    #[serde(default)]
    pub fields: Vec<String>,
}

impl ApiErrorDoc {
    /// The human-readable message, **unredacted**.
    pub fn message_unredacted(&self) -> &str {
        &self.message
    }
}

impl fmt::Debug for ApiErrorDoc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiErrorDoc")
            .field("error_code", &self.error_code)
            .field("fields", &self.fields)
            .field("message", &"<withheld>")
            .finish()
    }
}

/// A Salesforce error body: an array of [`ApiErrorDoc`] (the documented form), or one object.
#[derive(Deserialize)]
#[serde(untagged)]
pub enum ApiErrorBody {
    /// The documented array.
    Many(Vec<ApiErrorDoc>),
    /// One object.
    One(ApiErrorDoc),
}

impl ApiErrorBody {
    pub(crate) fn into_vec(self) -> Vec<ApiErrorDoc> {
        match self {
            ApiErrorBody::Many(v) => v,
            ApiErrorBody::One(e) => vec![e],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_fields_distinguish_absent_and_null() {
        let json = r#"{"attributes":{"type":"Account","url":"/services/data/v68.0/sobjects/Account/001R0000005hDFYIA2"},
            "Id":"001R0000005hDFYIA2","Name":"Acme","Industry":null,"Rating__x":"Hot"}"#;
        let a: Account = serde_json::from_str(json).unwrap();
        assert_eq!(a.industry, Field::Null);
        assert_eq!(a.customer_tier, Field::NotSelected);
        assert_eq!(a.extra["Rating__x"], "Hot");
        assert!(!format!("{a:?}").contains("Acme"));
    }

    #[test]
    fn error_docs_hide_messages() {
        let body: ApiErrorBody = serde_json::from_str(
            r#"[{"message":"id value of incorrect type: 001900K0001pPuOAAU","errorCode":"MALFORMED_ID","fields":["Id"]}]"#,
        )
        .unwrap();
        let docs = body.into_vec();
        assert_eq!(docs[0].error_code, "MALFORMED_ID");
        assert!(!format!("{docs:?}").contains("001900K0001pPuOAAU"));
    }
}
