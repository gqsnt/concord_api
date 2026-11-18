use concord::body::WriteError;
use concord::Body;

concord::api! {
    pub client Store(token: concord::Secret) {
        base "https://store.example.com";
        headers { "authorization" = "Bearer {token}"; }

        /// No body, and a length all the same: the server refuses a `POST` without one.
        POST archive(id: u64) {
            path "items/{id}/archive";
            headers { "content-length" = "0"; }
        }
        /// A body that is present and holds no bytes.
        PUT touch(id: u64) {
            path "items/{id}/touched";
            body concord::Body::from(Vec::new());
        }
        /// A JSON body when the caller sets `options`, and no bytes when it does not.
        DELETE delete(id: u64, options?: DeleteOptions) {
            path "items/{id}";
            body optional_json(options);
        }
    }
}

#[derive(Clone, serde::Serialize)]
pub struct DeleteOptions {
    pub grace_seconds: u32,
}

/// The JSON of the value, or a body of no bytes and no content type for `None`.
pub fn optional_json<T: serde::Serialize>(value: &Option<T>) -> Result<Body, WriteError> {
    match value {
        Some(value) => concord::json(value),
        None => Ok(Body::from(Vec::new())),
    }
}
