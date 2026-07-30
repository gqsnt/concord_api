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
    let default_semantics = render_default_semantics(docs);
    let request = render_request(docs);
    let response = render_response(docs);
    let summary = render_location(docs);
    let identity = if docs.explicit_contract_id {
        format!("Stable contract ID: `{}` (explicit).", docs.contract_id)
    } else {
        format!(
            "Stable contract ID: `{}` (derived from the declaration path; renaming that path changes the ID).",
            docs.contract_id
        )
    };
    format!(
        "Endpoint contract for `{} {}`.\n\n# Summary\n\n{summary}\n\n# Usage\n\nCall the `{}` direct scope method on {scope_chain}, or construct `{}` and bind it with `{}::request`. Both return `{}`.\n\n# Inputs\n\n{inputs}{default_semantics}\n\n# Request\n\n{request}\n\n# Response\n\n{response}\n\n# Identity\n\n{identity}\n\nEndpoint type: `{}`.\n\nRequest type: `{}`.",
        docs.http_method,
        render_route(docs),
        docs.direct_method_name,
        docs.endpoint_type_path,
        docs.client_name,
        docs.request_type_path,
        docs.endpoint_type_path,
        docs.request_type_path,
    )
}

fn render_default_semantics(docs: &EndpointDocModel) -> &'static str {
    let has_default = docs
        .scope_inputs
        .iter()
        .any(|input| input.kind == EndpointDocInputKind::Defaulted)
        || docs
            .endpoint_setters
            .iter()
            .any(|setter| setter.reset.is_some());
    if has_default {
        "\n\nEach declaration default is evaluated when its owning snapshot is constructed. The `= default` shorthand invokes application-owned `Default::default()` at that point; snapshot and request `reset_*` controls restore retained values without re-evaluation."
    } else {
        ""
    }
}

fn render_location(docs: &EndpointDocModel) -> String {
    let mut lines = vec![format!("Base URL: `{}`.", docs.base_url)];
    if let Some(host) = &docs.host {
        lines.push(format!("Host template: `{}`.", render_host(host)));
    }
    lines.push(format!("Path: `{}`.", render_route(docs)));
    lines.join("\n\n")
}

fn render_host(host: &EndpointDocHost) -> String {
    let mut rendered = host
        .label_atoms
        .iter()
        .map(render_route_atom)
        .collect::<Vec<_>>()
        .join(".");
    if !rendered.is_empty() {
        rendered.push('.');
    }
    rendered.push_str(&host.base_host);
    if let Some(port) = host.port {
        rendered.push(':');
        rendered.push_str(&port.to_string());
    }
    rendered
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
        .map(render_request_control)
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
        terminals.push("`.response()` includes accepted response metadata".to_owned());
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
    terminals.push(
        "`.response_bytes()` selects bounded undecoded bytes when the response mode permits it"
            .to_owned(),
    );

    format!(
        "Inert named request for [`{endpoint_name}`]. Construction and value controls perform no request lifecycle work.\n\nControls: {}.\n\nTerminals:\n\n{}",
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
    let controls = render_value_controls(setter, ", ");
    format!(
        "- `ep.{}: {}` — {}; {controls}.",
        setter.name,
        setter.type_name,
        setter_capability_name(setter),
    )
}

fn render_request_control(setter: &EndpointDocSetter) -> String {
    render_value_controls(setter, "; ")
}

fn setter_capability_name(setter: &EndpointDocSetter) -> &'static str {
    match (setter.clear.is_some(), setter.reset.is_some()) {
        (true, true) => "absence-capable and has a declaration default",
        (true, false) => "absence-capable",
        (false, true) => "has a declaration default",
        (false, false) => "plain",
    }
}

fn render_value_controls(setter: &EndpointDocSetter, separator: &str) -> String {
    let mut controls = Vec::new();
    let set = if setter.clear.is_some() {
        format!("set a present value with `.{}(...)`", setter.setter)
    } else if setter.reset.is_some() {
        format!("override with `.{}(...)`", setter.setter)
    } else {
        format!("set with `.{}(...)`", setter.setter)
    };
    controls.push(set);
    if let Some(clear) = &setter.clear {
        controls.push(format!("select absence with `.{clear}()`"));
    }
    if let Some(reset) = &setter.reset {
        controls.push(format!("restore the retained default with `.{reset}()`"));
    }
    controls.join(separator)
}

fn render_request(docs: &EndpointDocModel) -> String {
    let query = render_names("Public query names", &docs.safe_query_names);
    let headers = render_names("Public header names", &docs.safe_header_names);
    let body = docs.body.as_ref().map_or_else(
        || "Body: none.".to_owned(),
        |body| {
            let mut rendered = format!(
                "Body argument: `ep.{}`. Specification: `{}`. Input type: `{}`.",
                body.argument, body.specification, body.input_type
            );
            if docs.terminals.pagination {
                rendered.push_str(
                    " Pagination requires `PageRequestBodySpec`: every page clones the original \
                     page-stable input and prepares a fresh managed body.",
                );
            }
            rendered
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
            "Rate limits (applicable rules are listed in inherited order):\n{}",
            docs.rate_limits
                .iter()
                .map(|rule| {
                    let identity = if rule.explicit_id {
                        "explicit stable rule ID"
                    } else {
                        "derived deterministic rule ID"
                    };
                    let windows = rule
                        .windows
                        .iter()
                        .map(|window| {
                            format!("{}/{}", window.capacity, human_duration(window.duration))
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    let dimensions = if rule.dimensions.is_empty() {
                        "declaration-global".to_owned()
                    } else {
                        rule.dimensions
                            .iter()
                            .map(|dimension| format!("`{dimension}`"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    format!(
                        "- {} ({identity} `{}`)\n  - Bucketed by: {dimensions}\n  - Cost: {}\n  - Windows: {windows}",
                        rule.origin, rule.id, rule.cost,
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
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
    format!("{query}\n\n{headers}\n\n{body}\n\n{authentication}\n\n{rate_limits}\n\n{timeout}")
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
    use proc_macro2::TokenStream;
    use quote::quote;

    use super::super::test_support;
    use super::*;
    use crate::semantic;

    fn emitted_endpoint_docs(tokens: TokenStream, endpoint_name: &str) -> String {
        let api = semantic::test_support::resolve_tokens(tokens);
        endpoint_type_doc(semantic::test_support::endpoint(
            &api.client.children,
            endpoint_name,
        ))
    }

    fn riot_host_api() -> Api {
        semantic::test_support::resolve_tokens(quote! {
            client RiotApi {
                base "https://riotgames.com/";
                vars {
                    platform: String;
                    regional: String;
                }
            }
            scope platform() {
                host [vars.platform, "api"];
                path ["lol", "league", "v4"];
                GET challenger() -> Bytes;
            }
            scope regional() {
                host [vars.regional, "api"];
                path ["riot", "account", "v1"];
                GET account() -> Bytes;
            }
        })
    }

    #[test]
    fn static_host_prefix() {
        let docs = emitted_endpoint_docs(
            quote! {
                client HostApi { base "https://example.test:8443/root"; }
                GET read() -> Bytes {
                    host ["api"];
                    path ["items"];
                }
            },
            "read",
        );

        assert!(docs.contains("Base URL: `https://example.test:8443/root`."));
        assert!(docs.contains("Host template: `api.example.test:8443`."));
        assert!(docs.contains("Path: `/items`."));
    }

    #[test]
    fn ip_bases_without_host_shaping_render_their_authorities() {
        let ipv4 = emitted_endpoint_docs(
            quote! {
                client Ipv4Api { base "http://127.0.0.1:8080/root"; }
                GET read() -> Bytes;
            },
            "read",
        );
        assert!(ipv4.contains("Base URL: `http://127.0.0.1:8080/root`."));
        assert!(!ipv4.contains("Host template:"));

        let ipv6 = emitted_endpoint_docs(
            quote! {
                client Ipv6Api { base "http://[2001:db8::1]:8080/root"; }
                GET read() -> Bytes;
            },
            "read",
        );
        assert!(ipv6.contains("Base URL: `http://[2001:db8::1]:8080/root`."));
        assert!(!ipv6.contains("Host template:"));
    }

    #[test]
    fn impossible_ip_host_shaping_never_reaches_documentation_generation() {
        for declaration in [
            quote! {
                client Ipv4Api {
                    base "http://127.0.0.1:8080/";
                    host ["api"];
                }
                GET read() -> Bytes;
            },
            quote! {
                client Ipv6Api {
                    base "http://[2001:db8::1]:8080/";
                }
                GET read() -> Bytes {
                    host ["api"];
                }
            },
        ] {
            let error = semantic::test_support::try_resolve_tokens(declaration)
                .err()
                .expect("IP host shaping must fail before documentation IR");
            assert!(
                error
                    .to_string()
                    .contains("host-prefix shaping requires a DNS-domain fixed base")
            );
        }
    }

    #[test]
    fn client_value_in_host() {
        let docs = emitted_endpoint_docs(
            quote! {
                client HostApi {
                    base "https://example.test";
                    vars { locale: String; }
                }
                GET localized() -> Bytes {
                    host [vars.locale];
                }
            },
            "localized",
        );

        assert!(docs.contains("Host template: `{vars.locale}.example.test`."));
    }

    #[test]
    fn scope_value_in_host() {
        let docs = emitted_endpoint_docs(
            quote! {
                client HostApi { base "https://example.test"; }
                scope platform(platform: String) {
                    GET scoped() -> Bytes {
                        host [scope.platform];
                    }
                }
            },
            "scoped",
        );

        assert!(docs.contains("Host template: `{scope.platform}.example.test`."));
    }

    #[test]
    fn formatted_host_atom() {
        let docs = emitted_endpoint_docs(
            quote! {
                client HostApi { base "https://example.test"; }
                GET formatted(value: String) -> Bytes {
                    host [fmt["edge-", ep.value]];
                }
            },
            "formatted",
        );

        assert!(docs.contains("Host template: `edge-{ep.value}.example.test`."));
    }

    #[test]
    fn host_inheritance() {
        let docs = emitted_endpoint_docs(
            quote! {
                client HostApi {
                    base "https://example.test";
                    host ["gateway"];
                }
                scope inherited() {
                    GET read() -> Bytes;
                }
            },
            "read",
        );

        assert!(docs.contains("Host template: `gateway.example.test`."));
    }

    #[test]
    fn host_off() {
        let docs = emitted_endpoint_docs(
            quote! {
                client HostApi {
                    base "https://example.test/root";
                    host ["gateway"];
                }
                scope base_host() {
                    host off;
                    GET read() -> Bytes;
                }
            },
            "read",
        );

        assert!(docs.contains("Base URL: `https://example.test/root`."));
        assert!(docs.contains("Path: `/`."));
        assert!(!docs.contains("Host template:"));
    }

    #[test]
    fn sibling_scopes_have_distinct_hosts() {
        let api = semantic::test_support::resolve_tokens(quote! {
            client HostApi { base "https://example.test"; }
            scope platform() {
                host ["platform"];
                GET platform_status() -> Bytes;
            }
            scope regional() {
                host ["regional"];
                GET regional_status() -> Bytes;
            }
        });
        let platform = endpoint_type_doc(semantic::test_support::endpoint(
            &api.client.children,
            "platform_status",
        ));
        let regional = endpoint_type_doc(semantic::test_support::endpoint(
            &api.client.children,
            "regional_status",
        ));

        assert!(platform.contains("Host template: `platform.example.test`."));
        assert!(!platform.contains("regional.example.test"));
        assert!(regional.contains("Host template: `regional.example.test`."));
        assert!(!regional.contains("platform.example.test"));
    }

    #[test]
    fn riot_platform_endpoint_host() {
        let api = riot_host_api();
        let docs = endpoint_type_doc(semantic::test_support::endpoint(
            &api.client.children,
            "challenger",
        ));

        assert!(docs.contains("Base URL: `https://riotgames.com/`."));
        assert!(docs.contains("Host template: `{vars.platform}.api.riotgames.com`."));
        assert!(docs.contains("Path: `/lol/league/v4`."));
    }

    #[test]
    fn riot_regional_endpoint_host() {
        let api = riot_host_api();
        let docs = endpoint_type_doc(semantic::test_support::endpoint(
            &api.client.children,
            "account",
        ));

        assert!(docs.contains("Base URL: `https://riotgames.com/`."));
        assert!(docs.contains("Host template: `{vars.regional}.api.riotgames.com`."));
        assert!(docs.contains("Path: `/riot/account/v1`."));
    }

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
                        rate_limit {
                            by [endpoint, ep.region];
                            10 / 1500ms;
                        }
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
            "Bucketed by: `endpoint`, `ep.region`",
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
    fn key_values_do_not_appear_in_generated_docs() {
        let api = semantic::test_support::resolve_tokens(quote! {
            client RateDocsApi {
                base "https://example.test";
                vars { tenant: String; }
                rate_limit { 100 / 1m; }
            }
            scope accounts(account: String) {
                GET read(resource: String) -> Bytes {
                    rate_limit {
                        id "service.account.read";
                        by [host, endpoint, vars.tenant, scope.account, ep.resource];
                        10 / 1s;
                    }
                }
            }
        });
        let endpoint = semantic::test_support::endpoint(&api.client.children, "read");
        let docs = endpoint_type_doc(endpoint);

        assert!(docs.contains("applicable rules are listed in inherited order"));
        assert!(docs.contains("Bucketed by: declaration-global"));
        assert!(docs.contains(
            "Bucketed by: `host`, `endpoint`, `vars.tenant`, `scope.account`, `ep.resource`"
        ));
        assert!(docs.contains("derived deterministic rule ID"));
        assert!(docs.contains("explicit stable rule ID `service.account.read`"));
    }

    #[test]
    fn named_rule_docs_report_definition_attachment_and_logical_dimensions() {
        let api = semantic::test_support::resolve_tokens(quote! {
            client RateDocsApi {
                base "https://example.test";
                rate_limits {
                    rule account_reads {
                        by [host, account];
                        10 / 1m;
                    }
                }
            }
            scope accounts(account: String) {
                rate_limit use account_reads {
                    account = scope.account;
                    cost 2;
                }
                GET read() -> Bytes;
            }
        });
        let endpoint = semantic::test_support::endpoint(&api.client.children, "read");
        let docs = endpoint_type_doc(endpoint);

        assert!(docs.contains("rate-limit profile `account_reads` attached at scope accounts"));
        assert!(
            docs.contains("derived deterministic rule ID `rate_docs_api.rate_limit.account_reads`")
        );
        assert!(docs.contains("Bucketed by: `host`, `account`"));
        assert!(docs.contains("Cost: 2"));
        assert!(!docs.contains("Bucketed by: `host`, `scope.account`"));
    }

    #[test]
    fn value_controls_describe_optional_and_defaulted_semantics_exactly() {
        let api = semantic::test_support::resolve_tokens(quote! {
            client ControlDocsApi {
                base "https://example.test";
            }
            GET read(
                cursor: Option<String>,
                limit: u32 = default,
                label: Option<String> = None,
                values: Option<Vec<String>> = None,
            ) -> Bytes;
        });
        let endpoint = semantic::test_support::endpoint(&api.client.children, "read");
        let endpoint_docs = endpoint_type_doc(endpoint);
        assert!(endpoint_docs.contains(
            "absence-capable; set a present value with `.set_cursor(...)`, select absence with `.clear_cursor()`"
        ));
        assert!(endpoint_docs.contains(
            "has a declaration default; override with `.set_limit(...)`, restore the retained default with `.reset_limit()`"
        ));
        assert!(
            endpoint_docs.contains(
                "The `= default` shorthand invokes application-owned `Default::default()`"
            )
        );
        assert!(endpoint_docs.contains(
            "`ep.label: Option < String >` — absence-capable and has a declaration default; set a present value with `.set_label(...)`, select absence with `.clear_label()`, restore the retained default with `.reset_label()`"
        ));
        assert!(endpoint_docs.contains(
            "`ep.values: Option < Vec < String > >` — absence-capable and has a declaration default; set a present value with `.set_values(...)`, select absence with `.clear_values()`, restore the retained default with `.reset_values()`"
        ));
        assert!(!endpoint_docs.contains("Some(value)"));

        let request_docs = request_wrapper_doc(endpoint);
        assert!(request_docs.contains(
            "set a present value with `.set_cursor(...)`; select absence with `.clear_cursor()`"
        ));
        assert!(request_docs.contains(
            "override with `.set_limit(...)`; restore the retained default with `.reset_limit()`"
        ));
        assert!(request_docs.contains(
            "set a present value with `.set_label(...)`; select absence with `.clear_label()`; restore the retained default with `.reset_label()`"
        ));
        assert!(request_docs.contains(
            "set a present value with `.set_values(...)`; select absence with `.clear_values()`; restore the retained default with `.reset_values()`"
        ));
        assert!(!request_docs.contains("Some(value)"));
    }

    #[test]
    fn request_docs_list_endpoint_terminals() {
        let api = test_support::api();
        let endpoint = test_support::endpoint(&api);
        let docs = request_wrapper_doc(endpoint);
        assert!(docs.contains("`.response()`"));
        assert!(docs.contains("`.response_bytes()`"));
        assert!(!docs.contains("`.paginate(...)`"));
    }

    #[test]
    fn emitted_markdown_separates_direct_endpoint_and_request_ownership() {
        let api = semantic::test_support::resolve_tokens(quote! {
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
            POST pages(
                cursor: Option<String>,
                #[body] filter: Json<Filter>,
            ) -> Json<Page> {
                paginate crate::Controller {
                    cursor: bind ep.cursor,
                }
            }
        });
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
        assert!(request.contains("`.set_optional(...)`"));
        assert!(request.contains("`.response()`"));
        assert!(request.contains("`.response_bytes()`"));
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
        assert!(endpoint_type_doc(pages).contains("`PageRequestBodySpec`"));
        assert!(endpoint_type_doc(pages).contains("page-stable input"));
    }

    #[test]
    fn durations_are_compact_and_exact() {
        assert_eq!(human_duration(Duration::from_secs(20)), "20s");
        assert_eq!(human_duration(Duration::from_millis(250)), "250ms");
        assert_eq!(human_duration(Duration::from_millis(1_500)), "1.5s");
        assert_eq!(human_duration(Duration::from_secs(600)), "10m");
    }
}
