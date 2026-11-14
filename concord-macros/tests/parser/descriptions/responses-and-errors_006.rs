/// A document with its version, for the `If-Match` of the next call.
GET get(id: DocId) -> Versioned<Doc> {
    path "{id}";
    status { 200 { body, headers } => Versioned { etag: headers.get("etag").map(String::from), value: body } }
}
