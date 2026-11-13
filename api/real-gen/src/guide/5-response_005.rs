concord::api! { pub client Fixture {
base "https://fixture.example"; read Json;


/// A document with its version, for the `If-Match` of the next call.
GET get(id: DocId) -> Versioned<Doc> {
    path "{id}";
    status { 2xx { body, headers } => Versioned { etag: headers.get("etag").map(String::from), value: body } }
}


} }