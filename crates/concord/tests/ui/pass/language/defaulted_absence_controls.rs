use concord::api;

const DEFAULT_CLIENT_LABEL: &str = "client-default";
const DEFAULT_SCOPE_LABEL: &str = "scope-default";
const DEFAULT_ENDPOINT_LABEL: &str = "endpoint-default";
const DEFAULT_TAGS: Vec<&str> = Vec::new();

api! {
    client DefaultedAbsenceApi {
        timeout off;
        values {
            client_label?: &'static str = crate::DEFAULT_CLIENT_LABEL;
        }
        base "http://defaulted-absence.example.test";


    scope records(
        scope_label?: &'static str = crate::DEFAULT_SCOPE_LABEL,
    ) {
        GET read(
            endpoint_label?: &'static str = crate::DEFAULT_ENDPOINT_LABEL,
            tags?: Vec<&'static str> = crate::DEFAULT_TAGS,
        ) -> Bytes using Bytes;
    }
}}

fn main() {
    let client = DefaultedAbsenceApi::builder()
        .set_client_label(Some("client-override"))
        .clear_client_label()
        .reset_client_label()
        .build()
        .unwrap();

    let _ = client
        .records()
        .set_scope_label(Some("scope-override"))
        .clear_scope_label()
        .reset_scope_label();

    let _ = DefaultedAbsenceApi::unbound()
        .records()
        .set_scope_label(Some("scope-override"))
        .clear_scope_label()
        .reset_scope_label()
        .read()
        .set_endpoint_label(Some("endpoint-override"))
        .clear_endpoint_label()
        .reset_endpoint_label()
        .set_tags(Some(vec!["first", "second"]))
        .clear_tags()
        .reset_tags();

    let _: defaulted_absence_api::records::ReadRequest = client
        .records()
        .read()
        .set_endpoint_label(Some("endpoint-override"))
        .clear_endpoint_label()
        .reset_endpoint_label()
        .set_tags(Some(vec!["first", "second"]))
        .clear_tags()
        .reset_tags();
}
