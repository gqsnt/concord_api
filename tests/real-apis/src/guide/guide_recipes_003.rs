concord::api! {
pub client Gmail(authorization: concord::Secret) {
        base "https://gmail.googleapis.com";
        headers { "authorization" = authorization; }
        POST batch(request: concord::Body) -> concord::ByteStream {
            path "batch/gmail/v1"; body request; read concord::read::Stream;
        }
    }
}
