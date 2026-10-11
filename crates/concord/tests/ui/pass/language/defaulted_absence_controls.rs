// VALUE-03: initialized optional values (constant initializers, §3.1) have
// the full control set on the builder, scope handles, unbound intents and
// requests: `set_x` (payload), `replace_x(Option<T>)` (the v2 exact
// `set_x(Option<T>)` form), `omit_x` and `reset_x`. Every value is live
// through a keyed emission (VALUE-06).
use concord::{Binary, Bytes, api};

const DEFAULT_CLIENT_LABEL: &str = "client-default";
const DEFAULT_SCOPE_LABEL: &str = "scope-default";
const DEFAULT_ENDPOINT_LABEL: &str = "endpoint-default";
const DEFAULT_TAGS: Vec<&str> = Vec::new();

api! {
    client DefaultedAbsenceApi {
        timeout 30s;
        values {
            client_label?: &'static str = crate::DEFAULT_CLIENT_LABEL;
        }
        base "http://defaulted-absence.example.test";
        headers { "x-client-label" = client_label; }

        scope records(
            scope_label?: &'static str = crate::DEFAULT_SCOPE_LABEL,
        ) {
            headers { "x-scope-label" = scope_label; }

            GET read(
                endpoint_label?: &'static str = crate::DEFAULT_ENDPOINT_LABEL,
                tags?: Vec<&'static str> = crate::DEFAULT_TAGS,
            ) -> Bytes via Binary {
                query {
                    "label" = endpoint_label;
                    "tag" = each tags;
                }
            }
        }
    }
}

fn main() {
    let client = DefaultedAbsenceApi::builder()
        .set_client_label("client-override")
        .replace_client_label(Some("client-override"))
        .omit_client_label()
        .reset_client_label()
        .build()
        .unwrap();

    let _ = client
        .records()
        .replace_scope_label(Some("scope-override"))
        .omit_scope_label()
        .reset_scope_label();

    let _ = DefaultedAbsenceApi::unbound()
        .records()
        .set_scope_label("scope-override")
        .omit_scope_label()
        .reset_scope_label()
        .read()
        .replace_endpoint_label(Some("endpoint-override"))
        .omit_endpoint_label()
        .reset_endpoint_label()
        .replace_tags(Some(vec!["first", "second"]))
        .omit_tags()
        .reset_tags();

    let _: defaulted_absence_api::records::ReadRequest = client
        .records()
        .read()
        .set_endpoint_label("endpoint-override")
        .omit_endpoint_label()
        .reset_endpoint_label()
        .set_tags(vec!["first", "second"])
        .omit_tags()
        .reset_tags();
}
