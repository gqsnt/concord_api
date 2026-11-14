// The data types that the page names and does not print, cut to the fields that its code uses.

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectMeta {
    pub name: String,
    pub uid: String,
    pub resource_version: Option<String>,
    pub deletion_timestamp: Option<String>,
}
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListMeta {
    #[serde(rename = "continue")]
    pub continue_token: Option<String>,
    pub resource_version: Option<String>,
}
#[derive(Clone, Debug, serde::Deserialize)]
pub struct Head {
    pub metadata: ObjectMeta,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Pod {
    pub metadata: ObjectMeta,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Deployment {
    pub metadata: ObjectMeta,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Scale {
    pub metadata: ObjectMeta,
    pub spec: ScaleSpec,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ScaleSpec {
    pub replicas: i32,
}
#[derive(Clone, Debug, serde::Deserialize)]
pub struct DynamicObject {
    pub metadata: ObjectMeta,
}
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preconditions {
    pub uid: Option<String>,
    pub resource_version: Option<String>,
}
impl Preconditions {
    pub fn uid(uid: &str) -> Self {
        Preconditions { uid: Some(uid.to_owned()), resource_version: None }
    }
}
#[derive(Clone, Debug, serde::Deserialize)]
pub struct Status {
    pub code: Option<u16>,
    pub reason: Option<String>,
    pub message: Option<String>,
    pub details: Option<StatusDetails>,
}
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusDetails {
    pub retry_after_seconds: Option<u32>,
}
#[derive(Clone, Debug, serde::Serialize)]
pub enum Propagation {
    Orphan,
    Background,
    Foreground,
}
#[derive(Clone, Debug, serde::Serialize)]
pub enum VersionMatch {
    Exact,
    NotOlderThan,
}
#[derive(Clone, Debug, serde::Serialize)]
pub enum FieldValidation {
    Ignore,
    Warn,
    Strict,
}
