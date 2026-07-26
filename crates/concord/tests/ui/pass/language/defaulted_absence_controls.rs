use concord::api;

const DEFAULT_CLIENT_LABEL: Option<&'static str> = Some("client-default");
const DEFAULT_SCOPE_LABEL: Option<&'static str> = Some("scope-default");
const DEFAULT_ENDPOINT_LABEL: Option<&'static str> = Some("endpoint-default");
const DEFAULT_TAGS: Option<Vec<&'static str>> = Some(Vec::new());

api! {
    client DefaultedAbsenceApi {
        vars {
            client_label: Option<&'static str> = crate::DEFAULT_CLIENT_LABEL;
        }
        base "http://defaulted-absence.example.test";
    }

    scope records(
        scope_label: Option<&'static str> = crate::DEFAULT_SCOPE_LABEL,
    ) {
        GET read(
            endpoint_label: Option<&'static str> = crate::DEFAULT_ENDPOINT_LABEL,
            tags: Option<Vec<&'static str>> = crate::DEFAULT_TAGS,
        ) -> Bytes;
    }
}

fn main() {
    let client = DefaultedAbsenceApi::builder()
        .set_client_label("client-override")
        .clear_client_label()
        .reset_client_label()
        .build()
        .unwrap();

    let _ = client
        .records()
        .set_scope_label("scope-override")
        .clear_scope_label()
        .reset_scope_label();

    let _ = defaulted_absence_api::records::Read::new()
        .set_scope_label("scope-override")
        .clear_scope_label()
        .reset_scope_label()
        .set_endpoint_label("endpoint-override")
        .clear_endpoint_label()
        .reset_endpoint_label()
        .set_tags(vec!["first", "second"])
        .clear_tags()
        .reset_tags();

    let _: defaulted_absence_api::records::ReadRequest = client
        .records()
        .read()
        .set_endpoint_label("endpoint-override")
        .clear_endpoint_label()
        .reset_endpoint_label()
        .set_tags(vec!["first", "second"])
        .clear_tags()
        .reset_tags();
}
