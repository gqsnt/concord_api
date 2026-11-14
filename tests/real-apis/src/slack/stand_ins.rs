// The data types that the page names and does not print, cut to the fields that its code uses.

#[derive(Debug, serde::Deserialize)]
pub struct AuthIdentity {
    pub team_id: String,
    pub user_id: String,
}
#[derive(Debug, serde::Deserialize)]
pub struct Conversation {
    pub id: String,
}
#[derive(Debug, serde::Deserialize)]
pub struct ChannelInfo {
    pub channel: Conversation,
}
#[derive(Debug, serde::Deserialize)]
pub struct Message {
    pub ts: String,
}
#[derive(Debug, serde::Deserialize)]
pub struct MessageReceipt {
    pub channel: String,
    pub ts: String,
}
#[derive(Debug, serde::Deserialize)]
pub struct User {
    pub id: String,
}
#[derive(Debug, serde::Deserialize)]
pub struct UserInfo {
    pub user: User,
}
#[derive(Debug, serde::Deserialize)]
pub struct UploadTicket {
    pub upload_url: String,
    pub file_id: String,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct UploadedFile {
    pub id: String,
    pub title: Option<String>,
}
#[derive(Debug, serde::Deserialize)]
pub struct CompletedFiles {}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Block {}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Attachment {}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Modal {}
#[derive(Debug, serde::Deserialize)]
pub struct OpenedView {}
