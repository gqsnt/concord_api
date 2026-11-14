// The data types that the page names and does not print, cut to the fields that its code uses.

#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Refresh {
    True,
    False,
    WaitFor,
}
#[derive(Debug, serde::Deserialize)]
pub struct GetReply {
    #[serde(rename = "_id")]
    pub id: String,
    pub found: bool,
}
#[derive(Debug, serde::Deserialize)]
pub struct WriteReply {
    #[serde(rename = "_id")]
    pub id: String,
    pub result: String,
}
#[derive(Debug, serde::Deserialize)]
pub struct IndexAck {
    pub acknowledged: bool,
}
#[derive(Debug, serde::Deserialize)]
pub struct CountReply {
    pub count: u64,
}
#[derive(Debug, serde::Deserialize)]
pub struct SearchPage {
    pub timed_out: bool,
    #[serde(rename = "_shards")]
    pub shards: ShardStats,
    pub hits: Hits,
    pub pit_id: Option<String>,
}
#[derive(Debug, serde::Deserialize)]
pub struct ShardStats {
    pub failed: u32,
}
#[derive(Debug, serde::Deserialize)]
pub struct Hits {
    pub hits: Vec<Hit>,
}
#[derive(Debug, serde::Deserialize)]
pub struct Hit {
    #[serde(rename = "_id")]
    pub id: String,
    #[serde(default)]
    pub sort: Vec<serde_json::Value>,
}
#[derive(Debug, serde::Deserialize)]
pub struct OpenedPit {
    pub id: String,
}
#[derive(Debug, serde::Deserialize)]
pub struct ClosedPit {
    pub succeeded: bool,
}
#[derive(Debug, serde::Deserialize)]
pub struct BulkReply {
    pub errors: bool,
    pub items: Vec<std::collections::BTreeMap<String, BulkItem>>,
}
#[derive(Debug, serde::Deserialize)]
pub struct BulkItem {
    pub status: u16,
    pub error: Option<EsCause>,
}
