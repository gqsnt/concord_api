use std::time::Duration;

use super::*;

pub(super) fn endpoint_method_doc(endpoint: &Endpoint) -> String {
    let docs = &endpoint.docs;
    let endpoint_name = local_type_name(&docs.endpoint_type_path);
    let request_name = local_type_name(&docs.request_type_path);
    let arguments = if docs.required_endpoint_inputs.is_empty() {
        "Required arguments: none.".to_owned()
    } else {
        format!(
            "Required arguments: {}.",
            docs.required_endpoint_inputs
                .iter()
                .map(render_argument)
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    format!(
        "`{} {}`\n\nCreates an inert request for [`{endpoint_name}`]. {arguments} Returns [`{request_name}`].",
        docs.http_method,
        render_route(docs),
    )
}

pub(super) fn endpoint_type_doc(endpoint: &Endpoint) -> String {
    let docs = &endpoint.docs;
    let scope_chain = if docs.scope_chain.is_empty() {
        "the client".to_owned()
    } else {
        format!("scope chain `{}`", docs.scope_chain.join("::"))
    };
    let inputs = render_inputs(docs);
    let request = render_request(docs);
    let response = render_response(docs);
    let identity = if docs.explicit_contract_id {
        format!("Stable contract ID: `{}` (explicit).", docs.contract_id)
    } else {
        format!(
            "Stable contract ID: `{}` (derived from the declaration path; renaming that path changes the ID).",
            docs.contract_id
        )
    };
    format!(
        "Endpoint contract for `{} {}`.\n\n# Summary\n\nRelative route `{}` on base URL `{}`.\n\n# Usage\n\nCall the `{}` direct scope method on {scope_chain}, or construct `{}` and bind it with `{}::request`. Both return `{}`.\n\n# Inputs\n\n{inputs}\n\n# Request\n\n{request}\n\n# Response\n\n{response}\n\n# Identity\n\n{identity}\n\nEndpoint type: `{}`.\n\nRequest type: `{}`.",
        docs.http_method,
        render_route(docs),
        render_route(docs),
        docs.base_url,
        docs.direct_method_name,
        docs.endpoint_type_path,
        docs.client_name,
        docs.request_type_path,
        docs.endpoint_type_path,
        docs.request_type_path,
    )
}

pub(super) fn endpoint_constructor_doc(endpoint: &Endpoint) -> String {
    let docs = &endpoint.docs;
    let scope_inputs = docs
        .scope_inputs
        .iter()
        .filter(|input| input.kind == EndpointDocInputKind::Required)
        .count();
    let endpoint_inputs = docs.required_endpoint_inputs.len();
    format!(
        "Creates an inert `{}` endpoint value. Arguments contain {scope_inputs} required scope input(s) followed by {endpoint_inputs} required endpoint/body input(s).",
        docs.endpoint_type_path
    )
}

pub(super) fn request_wrapper_doc(endpoint: &Endpoint) -> String {
    let docs = &endpoint.docs;
    let endpoint_name = local_type_name(&docs.endpoint_type_path);
    let mut controls = docs
        .endpoint_setters
        .iter()
        .flat_map(|setter| {
            [
                format!("`.{}`", setter.method),
                format!("`.{}()`", setter.helper),
            ]
        })
        .collect::<Vec<_>>();
    controls.extend([
        "`.timeout(...)`".to_owned(),
        "`.disable_timeout()`".to_owned(),
        "`.inherit_timeout()`".to_owned(),
    ]);

    let mut terminals = Vec::new();
    if docs.terminals.direct_await {
        terminals.push(format!("`.await` returns `{}`", docs.response.output_type));
    }
    if docs.terminals.response {
        terminals.push("`.response()` includes successful response metadata".to_owned());
    }
    if docs.terminals.pagination {
        terminals.push("`.paginate(...)` runs bounded sequential pagination".to_owned());
    }
    terminals.extend(
        docs.terminals
            .credential_storage
            .iter()
            .map(|method| format!("`.{method}()` stores the decoded credential")),
    );
    if docs.terminals.raw_response {
        terminals.push(
            "`.raw_response()` selects bounded undecoded bytes when the response mode permits it"
                .to_owned(),
        );
    }

    format!(
        "Inert named request for [`{endpoint_name}`]. Construction and setters perform no request lifecycle work.\n\nControls: {}.\n\nTerminals:\n\n{}",
        controls.join(", "),
        terminals
            .iter()
            .map(|terminal| format!("- {terminal}"))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

pub(super) fn endpoint_identity(endpoint: &Endpoint) -> String {
    endpoint.surface.docs_identity.clone()
}

pub(super) fn render_route(docs: &EndpointDocModel) -> String {
    if docs.route_atoms.is_empty() {
        return "/".to_owned();
    }
    format!(
        "/{}",
        docs.route_atoms
            .iter()
            .map(render_route_atom)
            .collect::<Vec<_>>()
            .join("/")
    )
}

fn render_route_atom(atom: &EndpointDocRouteAtom) -> String {
    match atom {
        EndpointDocRouteAtom::Literal(value) => value.clone(),
        EndpointDocRouteAtom::Value { namespace, name } => {
            let namespace = match namespace {
                EndpointDocNamespace::Vars => "vars",
                EndpointDocNamespace::Scope => "scope",
                EndpointDocNamespace::Endpoint => "ep",
            };
            format!("{{{namespace}.{name}}}")
        }
        EndpointDocRouteAtom::Format(values) => {
            values.iter().map(render_route_atom).collect::<String>()
        }
    }
}

fn render_inputs(docs: &EndpointDocModel) -> String {
    let mut values = Vec::new();
    values.extend(docs.scope_inputs.iter().map(|input| {
        let owner = input.owner.as_deref().unwrap_or("scope");
        format!(
            "- `scope.{}: {}` — {} input from `{owner}`.",
            input.name,
            input.type_name,
            input_kind_name(input.kind)
        )
    }));
    values.extend(docs.required_endpoint_inputs.iter().map(|input| {
        format!(
            "- `ep.{}: {}` — required direct argument.",
            input.name, input.type_name
        )
    }));
    values.extend(docs.endpoint_setters.iter().map(render_setter));
    if values.is_empty() {
        "None.".to_owned()
    } else {
        values.join("\n")
    }
}

fn render_setter(setter: &EndpointDocSetter) -> String {
    format!(
        "- `ep.{}: {}` — {}; set with `.{}(...)`, restore with `.{}()`.",
        setter.name,
        setter.type_name,
        input_kind_name(setter.kind),
        setter.method,
        setter.helper,
    )
}

fn render_request(docs: &EndpointDocModel) -> String {
    let query = render_names("Public query names", &docs.safe_query_names);
    let headers = render_names("Public header names", &docs.safe_header_names);
    let body = docs.body.as_ref().map_or_else(
        || "Body: none.".to_owned(),
        |body| {
            format!(
                "Body argument: `ep.{}`. Specification: `{}`. Input type: `{}`.",
                body.argument, body.specification, body.input_type
            )
        },
    );
    let authentication = if docs.authentication.is_empty() {
        "Authentication: none.".to_owned()
    } else {
        format!(
            "Authentication: {}.",
            docs.authentication
                .iter()
                .map(|placement| { format!("{} ({})", placement.placement, placement.challenge) })
                .collect::<Vec<_>>()
                .join("; ")
        )
    };
    let rate_limits = if docs.rate_limits.is_empty() {
        "Rate limits: none.".to_owned()
    } else {
        format!(
            "Rate limits: {}.",
            docs.rate_limits
                .iter()
                .map(|rule| {
                    let windows = rule
                        .windows
                        .iter()
                        .map(|window| {
                            format!("{}/{}", window.capacity, human_duration(window.duration))
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!(
                        "{} (`{}`, cost {}; {windows})",
                        rule.origin, rule.id, rule.cost
                    )
                })
                .collect::<Vec<_>>()
                .join("; ")
        )
    };
    let timeout = match docs.timeout {
        EndpointDocTimeout::Inherited(duration) => format!(
            "Timeout: inherited {}. Request controls can override, disable, or restore it.",
            human_duration(duration)
        ),
        EndpointDocTimeout::Disabled => {
            "Timeout: disabled. Request controls can set or restore it.".to_owned()
        }
        EndpointDocTimeout::Overridden(duration) => format!(
            "Timeout: endpoint override {}. Request controls can override, disable, or restore it.",
            human_duration(duration)
        ),
    };
    let retry = match docs.native_retry {
        ResolvedRetry::Disabled => String::new(),
        ResolvedRetry::ReqwestDefault => "\n\nNative retry: enabled.".to_owned(),
    };
    format!(
        "{query}\n\n{headers}\n\n{body}\n\n{authentication}\n\n{rate_limits}\n\n{timeout}{retry}"
    )
}

fn render_response(docs: &EndpointDocModel) -> String {
    format!(
        "Specification: `{}`.\n\nOutput type: `{}`.\n\nThe response specification selects buffered, streaming, or discard acquisition. The named request documents the terminals actually available for this build.",
        docs.response.specification, docs.response.output_type
    )
}

fn render_names(label: &str, values: &[String]) -> String {
    if values.is_empty() {
        format!("{label}: none.")
    } else {
        format!(
            "{label}: {}.",
            values
                .iter()
                .map(|value| format!("`{value}`"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

fn render_argument(input: &EndpointDocInput) -> String {
    format!("`{}: {}`", input.name, input.type_name)
}

fn input_kind_name(kind: EndpointDocInputKind) -> &'static str {
    match kind {
        EndpointDocInputKind::Required => "required",
        EndpointDocInputKind::Optional => "optional",
        EndpointDocInputKind::Defaulted => "defaulted",
    }
}

fn local_type_name(path: &str) -> &str {
    path.rsplit("::").next().unwrap_or(path)
}

pub(super) fn human_duration(duration: Duration) -> String {
    let total = duration.as_nanos();
    if total == 0 {
        return "0s".to_owned();
    }
    const NANOS_PER_MICROSECOND: u128 = 1_000;
    const NANOS_PER_MILLISECOND: u128 = 1_000_000;
    const NANOS_PER_SECOND: u128 = 1_000_000_000;
    const NANOS_PER_MINUTE: u128 = 60 * NANOS_PER_SECOND;
    const NANOS_PER_HOUR: u128 = 60 * NANOS_PER_MINUTE;

    if total % NANOS_PER_HOUR == 0 {
        return format!("{}h", total / NANOS_PER_HOUR);
    }
    if total % NANOS_PER_MINUTE == 0 {
        return format!("{}m", total / NANOS_PER_MINUTE);
    }
    if total >= NANOS_PER_SECOND {
        return format_scaled(total, NANOS_PER_SECOND, 9, "s");
    }
    if total >= NANOS_PER_MILLISECOND {
        return format_scaled(total, NANOS_PER_MILLISECOND, 6, "ms");
    }
    if total >= NANOS_PER_MICROSECOND {
        return format_scaled(total, NANOS_PER_MICROSECOND, 3, "µs");
    }
    format!("{total}ns")
}

fn format_scaled(total: u128, unit: u128, width: usize, suffix: &str) -> String {
    let whole = total / unit;
    let remainder = total % unit;
    if remainder == 0 {
        return format!("{whole}{suffix}");
    }
    let fraction = format!("{remainder:0width$}")
        .trim_end_matches('0')
        .to_owned();
    format!("{whole}.{fraction}{suffix}")
}

pub(super) fn value_category_summary(values: &[Value], owner: &str) -> String {
    let names = |predicate: fn(&Value) -> bool| {
        values
            .iter()
            .filter(|value| predicate(value))
            .map(|value| {
                format!(
                    "`{}: {}`",
                    canonical_ident(&value.name),
                    value.ty.to_token_stream()
                )
            })
            .collect::<Vec<_>>()
    };
    let required = names(Value::is_required);
    let optional = names(Value::is_optional);
    let defaulted = names(|value| value.default().is_some());
    let mut categories = Vec::new();
    if !required.is_empty() {
        categories.push(format!("Required {owner} inputs: {}.", required.join(", ")));
    }
    if !optional.is_empty() {
        categories.push(format!("Optional {owner} inputs: {}.", optional.join(", ")));
    }
    if !defaulted.is_empty() {
        categories.push(format!(
            "Defaulted {owner} inputs: {}.",
            defaulted.join(", ")
        ));
    }
    if categories.is_empty() {
        format!("{owner} inputs: none.")
    } else {
        categories.join(" ")
    }
}

pub(super) fn retry_summary(retry: ResolvedRetry) -> &'static str {
    match retry {
        ResolvedRetry::Disabled => "Native retry: disabled.",
        ResolvedRetry::ReqwestDefault => "Native retry: enabled.",
    }
}

pub(super) fn credential_summary(client: &Client) -> String {
    let secrets = client
        .secrets
        .iter()
        .map(|secret| format!("`{}`", canonical_ident(&secret.name)))
        .collect::<Vec<_>>();
    let providers = client
        .credentials
        .iter()
        .filter(|credential| matches!(credential.source, CredentialSource::Custom { .. }))
        .map(|credential| format!("`{}`", canonical_ident(&credential.name)))
        .collect::<Vec<_>>();
    let secret_summary = if secrets.is_empty() {
        "Required protected secrets: none.".to_owned()
    } else {
        format!("Required protected secrets: {}.", secrets.join(", "))
    };
    let provider_summary = if providers.is_empty() {
        "Required credential providers: none.".to_owned()
    } else {
        format!("Required credential providers: {}.", providers.join(", "))
    };
    format!("{secret_summary} {provider_summary}")
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::super::test_support;
    use super::*;
    use crate::semantic;
    use crate::semantic::surface::SurfaceCapabilities;

    #[test]
    fn renders_named_routes_human_durations_and_user_sections() {
        let api = semantic::test_support::resolve_tokens(quote! {
            client DDragonApi {
                base "https://ddragon.example.test/root";
                vars { locale: String = "en_US"; }
                timeout 20s;
            }
            scope version(version: String) {
                path ["cdn", scope.version];
                scope data() {
                    path ["data", vars.locale];
                    GET realm(region: String, compact: Option<bool>) -> Json<Value> {
                        path [fmt[ep.region, ".json"]];
                        query { "compact" = ep.compact; }
                        headers { "X-View" = "public"; }
                        timeout 250ms;
                        rate_limit { 10 / 1500ms; }
                    }
                }
            }
        });
        let endpoint = semantic::test_support::endpoint(&api.client.children, "realm");
        assert_eq!(
            render_route(&endpoint.docs),
            "/cdn/{scope.version}/data/{vars.locale}/{ep.region}.json"
        );
        let docs = endpoint_type_doc(endpoint);
        for expected in [
            "# Summary",
            "# Usage",
            "# Inputs",
            "# Request",
            "# Response",
            "# Identity",
            "Public query names: `compact`",
            "Public header names: `X-View`",
            "Timeout: endpoint override 250ms",
            "10/1.5s",
            "Stable contract ID: `d_dragon_api.version.data.realm`",
        ] {
            assert!(docs.contains(expected), "missing `{expected}` in:\n{docs}");
        }
        for forbidden in [
            "{vars.VALUE}",
            "{scope.VALUE}",
            "{ep.VALUE}",
            "/<fixed base>",
            "normalized public head",
            "facade method",
            "core-owned capability bounds",
            "selected core capability",
            "application native retry",
        ] {
            assert!(
                !docs.contains(forbidden),
                "forbidden `{forbidden}` in:\n{docs}"
            );
        }
    }

    #[test]
    fn request_docs_list_only_feature_selected_and_endpoint_terminals() {
        let without_raw = test_support::api();
        let endpoint = test_support::endpoint(&without_raw);
        let docs = request_wrapper_doc(endpoint);
        assert!(docs.contains("`.response()`"));
        assert!(!docs.contains("`.paginate(...)`"));
        assert!(!docs.contains("`.raw_response()`"));

        let with_raw =
            test_support::api_with_capabilities(SurfaceCapabilities { raw_response: true });
        let docs = request_wrapper_doc(test_support::endpoint(&with_raw));
        assert!(docs.contains("`.raw_response()`"));
    }

    #[test]
    fn emitted_markdown_separates_direct_endpoint_and_request_ownership() {
        let api = semantic::test_support::resolve_tokens_with_capabilities(
            quote! {
                client DocsApi {
                    base "https://docs.example.test";
                    credentials {
                        secret token: String;
                        credential bearer = bearer(secret.token);
                    }
                    timeout 20s;
                }
                POST submit(
                    #[body] body: Json<Input>,
                    optional: Option<bool>,
                ) -> Json<Output> {
                    auth bearer bearer;
                }
                GET stream() -> Stream<OctetStream>;
                GET discard() -> NoContent {
                    timeout off;
                }
                GET pages(cursor: Option<String>) -> Json<Page> {
                    paginate crate::Controller {
                        cursor: bind ep.cursor,
                    }
                }
            },
            SurfaceCapabilities { raw_response: true },
        );
        let json = semantic::test_support::endpoint(&api.client.children, "submit");
        let direct = endpoint_method_doc(json);
        assert!(direct.starts_with("`POST /`"));
        assert!(direct.contains("Creates an inert request for [`Submit`]"));
        assert!(direct.contains("Returns [`SubmitRequest`]"));
        assert!(!direct.contains("# Request"));

        let endpoint = endpoint_type_doc(json);
        assert!(endpoint.contains("Body argument: `ep.body`"));
        assert!(endpoint.contains("Specification: `Json < Input >`"));
        assert!(endpoint.contains("Authentication: Bearer authorization"));
        assert!(endpoint.contains("Timeout: inherited 20s"));

        let request = request_wrapper_doc(json);
        assert!(request.contains("`.optional`"));
        assert!(request.contains("`.response()`"));
        assert!(request.contains("`.raw_response()`"));
        assert!(!request.contains("`.paginate(...)`"));

        let stream = semantic::test_support::endpoint(&api.client.children, "stream");
        let stream_docs = endpoint_type_doc(stream);
        assert!(stream_docs.contains("Specification: `Stream < OctetStream >`"));
        assert!(stream_docs.contains("Body: none."));
        assert!(stream_docs.contains("Authentication: none."));

        let discard = semantic::test_support::endpoint(&api.client.children, "discard");
        assert!(endpoint_type_doc(discard).contains("Specification: `NoContent`"));
        assert!(endpoint_type_doc(discard).contains("Timeout: disabled"));

        let pages = semantic::test_support::endpoint(&api.client.children, "pages");
        assert!(request_wrapper_doc(pages).contains("`.paginate(...)`"));
    }

    #[test]
    fn durations_are_compact_and_exact() {
        assert_eq!(human_duration(Duration::from_secs(20)), "20s");
        assert_eq!(human_duration(Duration::from_millis(250)), "250ms");
        assert_eq!(human_duration(Duration::from_millis(1_500)), "1.5s");
        assert_eq!(human_duration(Duration::from_secs(600)), "10m");
    }
}
