// The data types that the page names and does not print, cut to the fields that its code uses.

#[derive(Debug, serde::Deserialize)]
pub struct Message {
    pub content: Vec<ContentBlock>,
}
#[derive(Debug, serde::Deserialize)]
pub struct ContentBlock {
    pub text: Option<String>,
}
#[derive(Debug, serde::Deserialize)]
pub struct TokenCount {
    pub input_tokens: u64,
}
#[derive(Debug, serde::Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub display_name: String,
}
