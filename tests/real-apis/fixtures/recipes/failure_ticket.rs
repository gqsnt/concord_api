use concord::{mismatch, Binary, Fail, Json};
use serde::Deserialize;

#[derive(Deserialize)]
struct Ticket {
    upload_url: Option<String>,
    file_id: Option<String>,
}

/// A complete ticket to send a file.
#[derive(Debug)]
pub struct UploadTicket {
    /// Where the bytes go.
    pub url: String,
    /// The ID to pass when completing the upload.
    pub id: String,
}

/// Decodes a ticket and checks the two fields that the next step needs.
pub fn check_ticket(body: concord::bytes::Bytes) -> Result<UploadTicket, Fail<()>> {
    let ticket: Ticket = Json::decode(&body).map_err(Fail::from_read)?;
    match (ticket.upload_url, ticket.file_id) {
        (Some(url), Some(id)) => Ok(UploadTicket { url, id }),
        _ => Err(mismatch("the upload ticket is missing its URL or file id")),
    }
}

concord::api! {
    client Files {
        base "https://files.example.test";
        GET upload_ticket() -> UploadTicket {
            read Binary;
            status { 2xx { body } => check_ticket(body)? }
        }
    }
}
