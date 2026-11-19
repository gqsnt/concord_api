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
        /// A JSON body when the caller sets `options`. Otherwise the body is omitted.
        DELETE delete(id: u64, options?: DeleteOptions) {
            path "items/{id}";
            body options.as_ref().map(concord::json).transpose();
        }
        /// Declares a length of zero only when no configuration is supplied.
        PUT configure(id: u64, configuration?: concord::Body) {
            path "items/{id}/configuration";
            headers { "content-length" = configuration.is_none().then_some("0"); }
            body configuration.as_ref();
        }
    }
}

#[derive(Clone, serde::Serialize)]
pub struct DeleteOptions {
    pub grace_seconds: u32,
}

/// Send a delete with any options the caller supplied.
pub async fn remove(store: &Store, id: u64, options: Option<DeleteOptions>) -> store::Result<()> {
    store.delete(id).options(options).await
}
