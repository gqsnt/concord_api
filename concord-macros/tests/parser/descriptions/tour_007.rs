/// The new document, and where it lives.
POST create(doc: NewDoc) -> Created {
    body json(doc);
    status {
        201 { body, headers } => Created { doc: body, at: headers.require("location")?.to_owned() },
    }
}
