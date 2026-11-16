use concord::Body;

concord::api! {
    /// The host that receives the bytes of an upload, at the `upload_url` that Slack returns.
    pub client SlackUploads {
        base "https://files.slack.com";        // `upload_url` is `https://files.slack.com/upload/v1/…`
        /// The middle step of an upload. Call it with `concord::transfer::at`.
        POST upload(bytes: Body) { body bytes; }
    }
}
