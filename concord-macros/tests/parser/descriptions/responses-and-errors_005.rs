/// Not changed since `etag`: `None`.
GET get_if_changed(id: DocId, etag?: String) -> Option<Doc> {
    path "{id}";
    headers { "if-none-match" = etag; }
    status { 304 => None }
}
