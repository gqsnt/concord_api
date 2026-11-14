concord::api! { pub client Fixture {
base "https://fixture.example"; read Json;


/// The new document, and where it lives.
POST create(doc: NewDoc) -> Created {
    body json(doc);
    status {
        2xx { body, headers } => Created { doc: body, at: headers.require("location")?.to_owned() },
    }
}


} }