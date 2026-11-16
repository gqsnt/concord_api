concord::api! {
pub client SharePoint(site: concord::Base, authorization: concord::Secret) {
        base site;
        headers { "authorization" = authorization; }
        POST batch(request: concord::Body) -> concord::ByteStream {
            path "_api/$batch"; body request; read concord::read::Stream;
        }
    }
}
