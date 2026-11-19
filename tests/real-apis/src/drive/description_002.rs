concord::api! {
    /// Sends bytes to an established Drive resumable upload session.
    pub client DriveSessions {
        base "https://www.googleapis.com";

        /// Returns whether Drive completed the upload.
        PUT chunk(content_range: String, bytes: concord::Body) -> bool {
            headers { "content-range" = content_range; }
            body bytes;
            status { 308 => false, 2xx => true }
        }
    }
}

/// Sends one chunk to the returned session URL.
pub async fn send_chunk(
    sessions: &DriveSessions,
    session_url: &str,
    content_range: &str,
    bytes: concord::Body,
) -> drive_sessions::Result<bool> {
    concord::transfer::at_call(sessions.chunk(content_range, bytes), session_url)?.await
}
