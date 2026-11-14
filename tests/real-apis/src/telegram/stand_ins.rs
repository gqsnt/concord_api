// The data types that the page names and does not print, cut to the fields that its code uses.

#[derive(Clone, Debug, serde::Serialize)]
#[serde(untagged)]
pub enum ChatId {
    Id(i64),
    Username(String),
}
impl std::fmt::Display for ChatId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChatId::Id(id) => id.fmt(f),
            ChatId::Username(name) => name.fmt(f),
        }
    }
}
#[derive(Clone, Debug, serde::Serialize)]
pub enum ParseMode {
    #[serde(rename = "HTML")]
    Html,
    MarkdownV2,
}
#[derive(Debug, serde::Deserialize)]
pub struct User {
    pub first_name: String,
}
#[derive(Debug, serde::Deserialize)]
pub struct Message {
    pub message_id: i64,
    pub document: Option<Document>,
}
#[derive(Debug, serde::Deserialize)]
pub struct Document {
    pub file_id: String,
}
#[derive(Debug, serde::Deserialize)]
pub struct File {
    pub file_path: Option<String>,
}
#[derive(Debug, serde::Deserialize)]
pub struct Update {
    pub update_id: i64,
}
#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub enum EditedMessage {
    Message(Message),
    Inline(bool),
}
#[derive(Debug, serde::Deserialize)]
pub struct ChatFullInfo {}
#[derive(Clone, Debug, serde::Serialize)]
pub struct MessageEntity {}
#[derive(Clone, Debug, serde::Serialize)]
pub struct ReplyMarkup {}
#[derive(Clone, Debug, serde::Serialize)]
pub struct InlineKeyboardMarkup {}
