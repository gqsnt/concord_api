use std::time::Duration;

use quote::ToTokens;
use syn::Type;

use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
struct EndpointDocModel {
    client_name: String,
    scope_chain: Vec<String>,
    direct_method_name: String,
    endpoint_type_path: String,
    request_type_path: String,
    http_method: String,
    base_url: String,
    host: Option<EndpointDocHost>,
    route_atoms: Vec<EndpointDocRouteAtom>,
    scope_inputs: Vec<EndpointDocInput>,
    required_endpoint_inputs: Vec<EndpointDocInput>,
    endpoint_setters: Vec<EndpointDocSetter>,
    safe_query_names: Vec<String>,
    safe_header_names: Vec<String>,
    body: Option<EndpointDocBody>,
    response: EndpointDocResponse,
    authentication: Vec<EndpointDocAuthPlacement>,
    rate_limits: Vec<EndpointDocRateLimit>,
    timeout: EndpointDocTimeout,
    terminals: EndpointDocTerminals,
    contract_id: String,
    explicit_contract_id: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EndpointDocHost {
    label_atoms: Vec<EndpointDocRouteAtom>,
    base_host: String,
    port: Option<u16>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum EndpointDocRouteAtom {
    Literal(String),
    Value {
        namespace: EndpointDocNamespace,
        name: String,
    },
    Concat(Vec<EndpointDocRouteAtom>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum EndpointDocNamespace {
    Client,
    Scope(String),
    Endpoint,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EndpointDocInput {
    owner: Option<String>,
    name: String,
    type_name: String,
    kind: EndpointDocInputKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EndpointDocInputKind {
    Required,
    Optional,
    Defaulted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EndpointDocSetter {
    name: String,
    setter: String,
    with: Option<String>,
    clear: Option<String>,
    reset: Option<String>,
    type_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EndpointDocBody {
    argument: String,
    specification: String,
    input_type: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EndpointDocResponse {
    specification: String,
    output_type: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EndpointDocAuthPlacement {
    placement: String,
    challenge: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EndpointDocRateLimit {
    id: String,
    explicit_id: bool,
    origin: String,
    dimensions: Vec<String>,
    cost: u32,
    windows: Vec<EndpointDocRateLimitWindow>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EndpointDocRateLimitWindow {
    capacity: u32,
    duration: Duration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EndpointDocTimeout {
    Inherited(Duration),
    Disabled,
    Overridden(Duration),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EndpointDocTerminals {
    direct_await: bool,
    response: bool,
    accepted_bytes: bool,
    pagination: bool,
    credential_storage: Vec<String>,
}

fn projected_endpoint_docs(
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
) -> EndpointDocModel {
    let local_timeout = endpoint
        .local_attachments
        .iter()
        .any(|attachment| attachment.family == AttachmentFamily::Timeout);
    let timeout = match (local_timeout, endpoint.config.timeout) {
        (true, Some(duration)) => EndpointDocTimeout::Overridden(duration),
        (true, None) | (false, None) => EndpointDocTimeout::Disabled,
        (false, Some(duration)) => EndpointDocTimeout::Inherited(duration),
    };
    let surface = &endpoint.surface;
    let mut endpoint_type_path = canonical_ident(&client.surface.support_name);
    for module in &surface.module_path {
        endpoint_type_path.push_str("::");
        endpoint_type_path.push_str(&canonical_ident(module));
    }
    let mut request_type_path = endpoint_type_path.clone();
    endpoint_type_path.push_str("::");
    endpoint_type_path.push_str(&canonical_ident(&surface.endpoint_type_name));
    request_type_path.push_str("::");
    request_type_path.push_str(&canonical_ident(&surface.request_type_name));

    let body = endpoint.body.as_ref().map(|body| EndpointDocBody {
        argument: canonical_ident(&body.name),
        input_type: type_name(&body.input_type),
        specification: type_name(&body.spec_type),
    });
    let required_endpoint_inputs = surface
        .required_arguments
        .iter()
        .map(|argument| match argument {
            RequiredArgument::Parameter(index) => doc_input(None, &endpoint.parameters[*index]),
            RequiredArgument::Body => {
                let body = body.as_ref().expect("body documentation");
                EndpointDocInput {
                    owner: None,
                    name: body.argument.clone(),
                    type_name: body.input_type.clone(),
                    kind: EndpointDocInputKind::Required,
                }
            }
        })
        .collect();
    let endpoint_setters = surface
        .setters
        .iter()
        .map(|setter| {
            let value = &endpoint.parameters[setter.parameter];
            EndpointDocSetter {
                name: canonical_ident(&value.name),
                setter: canonical_ident(&setter.setter),
                with: setter.with.as_ref().map(canonical_ident),
                clear: setter.clear.as_ref().map(canonical_ident),
                reset: setter.reset.as_ref().map(canonical_ident),
                type_name: type_name(&value.effective_type),
            }
        })
        .collect();
    let scope_inputs = scopes
        .iter()
        .flat_map(|scope| {
            let owner = canonical_ident(&scope.name);
            scope
                .parameters
                .iter()
                .map(move |value| doc_input(Some(owner.clone()), value))
        })
        .collect();
    let route_atoms = endpoint
        .config
        .path
        .iter()
        .map(|atom| route_atom(atom, &client.values, scopes, &endpoint.parameters))
        .collect();
    let host = if endpoint.config.host.is_empty() {
        None
    } else {
        let base = url::Url::parse(&client.base.value())
            .expect("validated base URL for endpoint documentation");
        Some(EndpointDocHost {
            label_atoms: endpoint
                .config
                .host
                .iter()
                .map(|atom| route_atom(atom, &client.values, scopes, &endpoint.parameters))
                .collect(),
            base_host: base
                .host_str()
                .expect("validated base host for endpoint documentation")
                .to_owned(),
            port: base.port(),
        })
    };
    let authentication = endpoint
        .config
        .auth
        .iter()
        .map(|auth| EndpointDocAuthPlacement {
            placement: match &auth.kind {
                ResolvedAuthPlacement::Bearer => "Bearer authorization".to_owned(),
                ResolvedAuthPlacement::Basic => "Basic authorization".to_owned(),
                ResolvedAuthPlacement::Authorization(None) => {
                    "exact Authorization value".to_owned()
                }
                ResolvedAuthPlacement::Authorization(Some(scheme)) => {
                    format!("{} Authorization scheme", scheme.value())
                }
                ResolvedAuthPlacement::Header(name) => {
                    format!("private header {}", name.value())
                }
                ResolvedAuthPlacement::Query(name) => {
                    format!("private query {}", name.value())
                }
            },
            challenge: match auth.challenge {
                ResolvedChallengePolicy::Unauthorized => "recover once after 401".to_owned(),
                ResolvedChallengePolicy::UnauthorizedOrForbidden => {
                    "recover once after 401 or 403".to_owned()
                }
                ResolvedChallengePolicy::Never => "no automatic recovery".to_owned(),
            },
        })
        .collect();
    let rate_limits = endpoint
        .config
        .rate_limits
        .iter()
        .map(|rule| EndpointDocRateLimit {
            id: rule.id.clone(),
            explicit_id: rule.explicit_id,
            origin: rule.origin.clone(),
            dimensions: rule
                .dimensions
                .iter()
                .map(|dimension| match dimension {
                    NormalizedRateLimitDimension::Host { .. } => "host".to_owned(),
                    NormalizedRateLimitDimension::Endpoint { .. } => "endpoint".to_owned(),
                    NormalizedRateLimitDimension::Value { public_name, .. } => public_name.clone(),
                })
                .collect(),
            cost: rule.cost,
            windows: rule
                .windows
                .iter()
                .map(|window| EndpointDocRateLimitWindow {
                    capacity: window.capacity,
                    duration: window.duration,
                })
                .collect(),
        })
        .collect();

    EndpointDocModel {
        client_name: canonical_ident(&client.name),
        scope_chain: scopes
            .iter()
            .map(|scope| canonical_ident(&scope.name))
            .collect(),
        direct_method_name: canonical_ident(&surface.method_name),
        endpoint_type_path,
        request_type_path,
        http_method: resolved_method_name(&endpoint.method),
        base_url: client.base.value(),
        host,
        route_atoms,
        scope_inputs,
        required_endpoint_inputs,
        endpoint_setters,
        safe_query_names: endpoint
            .config
            .query
            .iter()
            .map(|entry| entry.key.value())
            .collect(),
        safe_header_names: endpoint
            .config
            .headers
            .iter()
            .map(|entry| entry.name.value())
            .collect(),
        body,
        response: EndpointDocResponse {
            output_type: type_name(&endpoint.output),
            specification: type_name(&endpoint.response_spec),
        },
        authentication,
        rate_limits,
        timeout,
        terminals: EndpointDocTerminals {
            direct_await: true,
            response: surface.terminals.response,
            accepted_bytes: surface.terminals.accepted_bytes,
            pagination: surface.terminals.pagination,
            credential_storage: surface
                .terminals
                .credential_storage
                .iter()
                .map(|terminal| canonical_ident(&terminal.method))
                .collect(),
        },
        contract_id: endpoint.contract_id.value.clone(),
        explicit_contract_id: endpoint.contract_id.explicit,
    }
}

fn route_atom(
    value: &ResolvedExpr,
    client_values: &[Value],
    scopes: &[&Scope],
    endpoint_values: &[Value],
) -> EndpointDocRouteAtom {
    match value {
        ResolvedExpr::String(value) => EndpointDocRouteAtom::Literal(value.value()),
        ResolvedExpr::Integer(value) => EndpointDocRouteAtom::Literal(value.to_string()),
        ResolvedExpr::Value(value) => match value.reference {
            ValueReference::Client(index) => EndpointDocRouteAtom::Value {
                namespace: EndpointDocNamespace::Client,
                name: canonical_ident(
                    &client_values
                        .get(index)
                        .expect("resolved client documentation value")
                        .name,
                ),
            },
            ValueReference::Scope {
                scope_id,
                parameter,
            } => {
                let scope = scopes
                    .iter()
                    .find(|scope| scope.id == scope_id)
                    .expect("resolved scope documentation frame");
                EndpointDocRouteAtom::Value {
                    namespace: EndpointDocNamespace::Scope(canonical_ident(&scope.name)),
                    name: canonical_ident(
                        &scope
                            .parameters
                            .get(parameter)
                            .expect("resolved scope documentation value")
                            .name,
                    ),
                }
            }
            ValueReference::Endpoint(index) => EndpointDocRouteAtom::Value {
                namespace: EndpointDocNamespace::Endpoint,
                name: canonical_ident(
                    &endpoint_values
                        .get(index)
                        .expect("resolved endpoint documentation value")
                        .name,
                ),
            },
        },
        ResolvedExpr::Concat(values) => EndpointDocRouteAtom::Concat(
            values
                .iter()
                .map(|value| route_atom(value, client_values, scopes, endpoint_values))
                .collect(),
        ),
    }
}

fn doc_input(owner: Option<String>, value: &Value) -> EndpointDocInput {
    EndpointDocInput {
        owner,
        name: canonical_ident(&value.name),
        type_name: type_name(&value.payload_type),
        kind: input_kind(value),
    }
}

fn input_kind(value: &Value) -> EndpointDocInputKind {
    if value.is_required() {
        EndpointDocInputKind::Required
    } else if value.is_optional() && !value.has_default() {
        EndpointDocInputKind::Optional
    } else {
        EndpointDocInputKind::Defaulted
    }
}

fn type_name(value: &Type) -> String {
    value.to_token_stream().to_string()
}

fn resolved_method_name(method: &ResolvedMethod) -> String {
    match method {
        ResolvedMethod::Get => "GET".to_owned(),
        ResolvedMethod::Post => "POST".to_owned(),
        ResolvedMethod::Put => "PUT".to_owned(),
        ResolvedMethod::Patch => "PATCH".to_owned(),
        ResolvedMethod::Delete => "DELETE".to_owned(),
        ResolvedMethod::Head => "HEAD".to_owned(),
        ResolvedMethod::Options => "OPTIONS".to_owned(),
        ResolvedMethod::Custom(value) => value.value(),
    }
}

pub(super) fn endpoint_method_doc(
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
) -> String {
    let docs = projected_endpoint_docs(client, scopes, endpoint);
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
        render_route(&docs),
    )
}

pub(super) fn endpoint_type_doc(client: &Client, scopes: &[&Scope], endpoint: &Endpoint) -> String {
    let docs = projected_endpoint_docs(client, scopes, endpoint);
    let scope_chain = if docs.scope_chain.is_empty() {
        "the client".to_owned()
    } else {
        format!("scope chain `{}`", docs.scope_chain.join("::"))
    };
    let inputs = render_inputs(&docs);
    let default_semantics = render_default_semantics(&docs);
    let request = render_request(&docs);
    let response = render_response(&docs);
    let summary = render_location(&docs);
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
        render_route(&docs),
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
        "\n\nEach declaration default is evaluated when its owning snapshot is constructed. `#[default]` invokes application-owned `Default::default()` for the written payload type at that point; snapshot and request `reset_*` controls restore retained values without re-evaluation."
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

pub(super) fn request_wrapper_doc(
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
) -> String {
    let docs = projected_endpoint_docs(client, scopes, endpoint);
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
    if docs.terminals.accepted_bytes {
        terminals.push(
            "`.accepted_bytes()` selects complete bounded accepted bytes while preserving authentication challenge and status semantics"
                .to_owned(),
        );
    }

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

pub(super) fn endpoint_route(client: &Client, scopes: &[&Scope], endpoint: &Endpoint) -> String {
    render_route(&projected_endpoint_docs(client, scopes, endpoint))
}

fn render_route(docs: &EndpointDocModel) -> String {
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
                EndpointDocNamespace::Client => "client",
                EndpointDocNamespace::Scope(owner) => owner,
                EndpointDocNamespace::Endpoint => "endpoint",
            };
            format!("{{{namespace}.{name}}}")
        }
        EndpointDocRouteAtom::Concat(values) => {
            values.iter().map(render_route_atom).collect::<String>()
        }
    }
}

fn render_inputs(docs: &EndpointDocModel) -> String {
    let mut values = Vec::new();
    values.extend(docs.scope_inputs.iter().map(|input| {
        let owner = input.owner.as_deref().unwrap_or("scope");
        format!(
            "- `{owner}.{}: {}` — {} input owned by scope `{owner}`.",
            input.name,
            input.type_name,
            input_kind_name(input.kind)
        )
    }));
    values.extend(docs.required_endpoint_inputs.iter().map(|input| {
        format!(
            "- `endpoint.{}: {}` — required direct argument.",
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
        "- `endpoint.{}: {}` — {}; {controls}.",
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
    let set = format!(
        "replace the exact effective value with `.{}(...)`",
        setter.setter
    );
    controls.push(set);
    if let Some(with) = &setter.with {
        controls.push(format!("select a present payload with `.{with}(...)`"));
    }
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
                "Body argument: `endpoint.{}`. Specification: `{}`. Input type: `{}`.",
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
            "Rate limits (applicable policies are listed in inherited order):\n{}",
            docs.rate_limits
                .iter()
                .map(|rule| {
                    let identity = if rule.explicit_id {
                        "explicit stable policy ID"
                    } else {
                        "derived deterministic policy ID"
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
                    value.effective_type.to_token_stream()
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

    fn endpoint_context<'a>(
        client: &'a Client,
        endpoint_name: &str,
    ) -> (Vec<&'a Scope>, &'a Endpoint) {
        fn find<'a>(
            children: &'a [Child],
            endpoint_name: &str,
            scopes: &mut Vec<&'a Scope>,
        ) -> Option<(Vec<&'a Scope>, &'a Endpoint)> {
            for child in children {
                match child {
                    Child::Endpoint(endpoint) if endpoint.name == endpoint_name => {
                        return Some((scopes.clone(), endpoint));
                    }
                    Child::Scope(scope) => {
                        scopes.push(scope);
                        if let Some(found) = find(&scope.children, endpoint_name, scopes) {
                            return Some(found);
                        }
                        scopes.pop();
                    }
                    Child::Endpoint(_) => {}
                }
            }
            None
        }

        find(&client.children, endpoint_name, &mut Vec::new())
            .unwrap_or_else(|| panic!("endpoint `{endpoint_name}`"))
    }

    fn endpoint_type_docs(api: &Api, endpoint_name: &str) -> String {
        let (scopes, endpoint) = endpoint_context(&api.client, endpoint_name);
        endpoint_type_doc(&api.client, &scopes, endpoint)
    }

    fn endpoint_request_docs(api: &Api, endpoint_name: &str) -> String {
        let (scopes, endpoint) = endpoint_context(&api.client, endpoint_name);
        request_wrapper_doc(&api.client, &scopes, endpoint)
    }

    fn endpoint_method_docs(api: &Api, endpoint_name: &str) -> String {
        let (scopes, endpoint) = endpoint_context(&api.client, endpoint_name);
        endpoint_method_doc(&api.client, &scopes, endpoint)
    }

    fn endpoint_route_docs(api: &Api, endpoint_name: &str) -> String {
        let (scopes, endpoint) = endpoint_context(&api.client, endpoint_name);
        endpoint_route(&api.client, &scopes, endpoint)
    }

    fn emitted_endpoint_docs(tokens: TokenStream, endpoint_name: &str) -> String {
        let api = semantic::test_support::resolve_tokens(tokens);
        endpoint_type_docs(&api, endpoint_name)
    }

    fn riot_host_api() -> Api {
        semantic::test_support::resolve_tokens(quote! {
            client RiotApi {
                timeout off;
                base "https://riotgames.com/";
                values {
                    platform: String;
                    regional: String;
                }

            scope platform() {
                subdomain [client.platform, "api"];
                path ["lol", "league", "v4"];
                GET challenger() -> Bytes using Bytes;
            }
            scope regional() {
                subdomain [client.regional, "api"];
                path ["riot", "account", "v1"];
                GET account() -> Bytes using Bytes;
            }
        }})
    }

    #[test]
    fn static_host_prefix() {
        let docs = emitted_endpoint_docs(
            quote! {
                client HostApi {
                    timeout off; base "https://example.test:8443/root";
                GET read() -> Bytes using Bytes {
                    subdomain ["api"];
                    path ["items"];
                }
            }},
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
                client Ipv4Api {
                    timeout off; base "http://127.0.0.1:8080/root";
                GET read() -> Bytes using Bytes;
            }},
            "read",
        );
        assert!(ipv4.contains("Base URL: `http://127.0.0.1:8080/root`."));
        assert!(!ipv4.contains("Host template:"));

        let ipv6 = emitted_endpoint_docs(
            quote! {
                client Ipv6Api {
                    timeout off; base "http://[2001:db8::1]:8080/root";
                GET read() -> Bytes using Bytes;
            }},
            "read",
        );
        assert!(ipv6.contains("Base URL: `http://[2001:db8::1]:8080/root`."));
        assert!(!ipv6.contains("Host template:"));
    }

    #[test]
    fn impossible_ip_subdomain_shaping_never_reaches_documentation_generation() {
        for declaration in [
            quote! {
                client Ipv4Api {
                    timeout off;
                    base "http://127.0.0.1:8080/";
                    subdomain ["api"];

                GET read() -> Bytes using Bytes;
            }},
            quote! {
                client Ipv6Api {
                    timeout off;
                    base "http://[2001:db8::1]:8080/";

                GET read() -> Bytes using Bytes {
                    subdomain ["api"];
                }
            }},
        ] {
            let error = semantic::test_support::try_resolve_tokens(declaration)
                .err()
                .expect("IP host shaping must fail before documentation IR");
            assert!(
                error
                    .to_string()
                    .contains("subdomain shaping requires a DNS-domain fixed base")
            );
        }
    }

    #[test]
    fn client_value_in_host() {
        let docs = emitted_endpoint_docs(
            quote! {
                client HostApi {
                    timeout off;
                    base "https://example.test";
                    values { locale: String; }

                GET localized() -> Bytes using Bytes {
                    subdomain [client.locale];
                }
            }},
            "localized",
        );

        assert!(docs.contains("Host template: `{client.locale}.example.test`."));
    }

    #[test]
    fn scope_value_in_host() {
        let docs = emitted_endpoint_docs(
            quote! {
                client HostApi {
                    timeout off; base "https://example.test";
                scope platform(platform: String) {
                    GET scoped() -> Bytes using Bytes {
                        subdomain [platform.platform];
                    }
                }
            }},
            "scoped",
        );

        assert!(docs.contains("Host template: `{platform.platform}.example.test`."));
    }

    #[test]
    fn formatted_host_atom() {
        let docs = emitted_endpoint_docs(
            quote! {
                client HostApi {
                    timeout off; base "https://example.test";
                GET formatted(value: String) -> Bytes using Bytes {
                    subdomain [concat["edge-", endpoint.value]];
                }
            }},
            "formatted",
        );

        assert!(docs.contains("Host template: `edge-{endpoint.value}.example.test`."));
    }

    #[test]
    fn host_inheritance() {
        let docs = emitted_endpoint_docs(
            quote! {
                client HostApi {
                    timeout off;
                    base "https://example.test";
                    subdomain ["gateway"];

                scope inherited() {
                    GET read() -> Bytes using Bytes;
                }
            }},
            "read",
        );

        assert!(docs.contains("Host template: `gateway.example.test`."));
    }

    #[test]
    fn host_off() {
        let docs = emitted_endpoint_docs(
            quote! {
                client HostApi {
                    timeout off;
                    base "https://example.test/root";
                    subdomain ["gateway"];

                scope base_host() {
                    subdomain off;
                    GET read() -> Bytes using Bytes;
                }
            }},
            "read",
        );

        assert!(docs.contains("Base URL: `https://example.test/root`."));
        assert!(docs.contains("Path: `/`."));
        assert!(!docs.contains("Host template:"));
    }

    #[test]
    fn sibling_scopes_have_distinct_hosts() {
        let api = semantic::test_support::resolve_tokens(quote! {
            client HostApi {
                timeout off; base "https://example.test";
            scope platform() {
                subdomain ["platform"];
                GET platform_status() -> Bytes using Bytes;
            }
            scope regional() {
                subdomain ["regional"];
                GET regional_status() -> Bytes using Bytes;
            }
        }});
        let platform = endpoint_type_docs(&api, "platform_status");
        let regional = endpoint_type_docs(&api, "regional_status");

        assert!(platform.contains("Host template: `platform.example.test`."));
        assert!(!platform.contains("regional.example.test"));
        assert!(regional.contains("Host template: `regional.example.test`."));
        assert!(!regional.contains("platform.example.test"));
    }

    #[test]
    fn riot_platform_endpoint_host() {
        let api = riot_host_api();
        let docs = endpoint_type_docs(&api, "challenger");

        assert!(docs.contains("Base URL: `https://riotgames.com/`."));
        assert!(docs.contains("Host template: `{client.platform}.api.riotgames.com`."));
        assert!(docs.contains("Path: `/lol/league/v4`."));
    }

    #[test]
    fn riot_regional_endpoint_host() {
        let api = riot_host_api();
        let docs = endpoint_type_docs(&api, "account");

        assert!(docs.contains("Base URL: `https://riotgames.com/`."));
        assert!(docs.contains("Host template: `{client.regional}.api.riotgames.com`."));
        assert!(docs.contains("Path: `/riot/account/v1`."));
    }

    #[test]
    fn renders_named_routes_human_durations_and_user_sections() {
        let api = semantic::test_support::resolve_tokens(quote! {
            client DDragonApi {
                base "https://ddragon.example.test/root";
                values { locale: String = "en_US"; }
                timeout 20s;

            scope version(version: String) {
                path ["cdn", version.version];
                scope data() {
                    path ["data", client.locale];
                    GET realm(region: String, compact?: bool) -> Value using Json<Value> {
                        path [concat[endpoint.region, ".json"]];
                        query { "compact" = endpoint.compact; }
                        headers { "X-View" = "public"; }
                        timeout 250ms;
                        rate_limit {
                            key [endpoint, endpoint.region];
                            allow 10 per 1500ms;
                        }
                    }
                }
            }
        }});
        assert_eq!(
            endpoint_route_docs(&api, "realm"),
            "/cdn/{version.version}/data/{client.locale}/{endpoint.region}.json"
        );
        let docs = endpoint_type_docs(&api, "realm");
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
            "Bucketed by: `endpoint`, `endpoint.region`",
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
                timeout off;
                base "https://example.test";
                values { tenant: String; }
                rate_limit { allow 100 per 1m; }

            scope accounts(account: String) {
                GET read(resource: String) -> Bytes using Bytes {
                    rate_limit {
                        id "service.account.read";
                        key [host, endpoint, client.tenant, accounts.account, endpoint.resource];
                        allow 10 per 1s;
                    }
                }
            }
        }});
        let docs = endpoint_type_docs(&api, "read");

        assert!(docs.contains("applicable policies are listed in inherited order"));
        assert!(docs.contains("Bucketed by: declaration-global"));
        assert!(docs.contains(
            "Bucketed by: `host`, `endpoint`, `client.tenant`, `accounts.account`, `endpoint.resource`"
        ));
        assert!(docs.contains("derived deterministic policy ID"));
        assert!(docs.contains("explicit stable policy ID `service.account.read`"));
    }

    #[test]
    fn named_rule_docs_report_definition_attachment_and_logical_dimensions() {
        let api = semantic::test_support::resolve_tokens(quote! {
            client RateDocsApi {
                timeout off;
                base "https://example.test";
                rate_limits {
                    account_reads(account) {
                        key [host, account];
                        allow 10 per 1m;
                    }
                }

            scope accounts(account: String) {
                rate_limit account_reads(account = accounts.account) { cost 2; }
                GET read() -> Bytes using Bytes;
            }
        }});
        let docs = endpoint_type_docs(&api, "read");

        assert!(docs.contains("rate-limit profile `account_reads` attached at scope accounts"));
        assert!(
            docs.contains(
                "derived deterministic policy ID `rate_docs_api.rate_limit.account_reads`"
            )
        );
        assert!(docs.contains("Bucketed by: `host`, `account`"));
        assert!(docs.contains("Cost: 2"));
        assert!(!docs.contains("Bucketed by: `host`, `accounts.account`"));
    }

    #[test]
    fn value_controls_describe_optional_and_defaulted_semantics_exactly() {
        let api = semantic::test_support::resolve_tokens(quote! {
            client ControlDocsApi {
                timeout off;
                base "https://example.test";

            GET read(
                cursor?: String,
                #[default]
                limit: u32,
                label?: Option<String> = None,
                values?: Option<Vec<String>> = None,
            ) -> Bytes using Bytes;
        }});
        let endpoint_docs = endpoint_type_docs(&api, "read");
        assert!(endpoint_docs.contains(
            "absence-capable; replace the exact effective value with `.set_cursor(...)`, select a present payload with `.with_cursor(...)`, select absence with `.clear_cursor()`"
        ));
        assert!(endpoint_docs.contains(
            "has a declaration default; replace the exact effective value with `.set_limit(...)`, restore the retained default with `.reset_limit()`"
        ));
        assert!(
            endpoint_docs.contains("`#[default]` invokes application-owned `Default::default()`")
        );
        assert!(endpoint_docs.contains(
            "`endpoint.label: :: std :: option :: Option < Option < String > >` — absence-capable and has a declaration default; replace the exact effective value with `.set_label(...)`, select a present payload with `.with_label(...)`, select absence with `.clear_label()`, restore the retained default with `.reset_label()`"
        ));
        assert!(endpoint_docs.contains(
            "`endpoint.values: :: std :: option :: Option < Option < Vec < String > > >` — absence-capable and has a declaration default; replace the exact effective value with `.set_values(...)`, select a present payload with `.with_values(...)`, select absence with `.clear_values()`, restore the retained default with `.reset_values()`"
        ));
        assert!(!endpoint_docs.contains("Some(value)"));

        let request_docs = endpoint_request_docs(&api, "read");
        assert!(request_docs.contains(
            "replace the exact effective value with `.set_cursor(...)`; select a present payload with `.with_cursor(...)`; select absence with `.clear_cursor()`"
        ));
        assert!(request_docs.contains(
            "replace the exact effective value with `.set_limit(...)`; restore the retained default with `.reset_limit()`"
        ));
        assert!(request_docs.contains(
            "replace the exact effective value with `.set_label(...)`; select a present payload with `.with_label(...)`; select absence with `.clear_label()`; restore the retained default with `.reset_label()`"
        ));
        assert!(request_docs.contains(
            "replace the exact effective value with `.set_values(...)`; select a present payload with `.with_values(...)`; select absence with `.clear_values()`; restore the retained default with `.reset_values()`"
        ));
        assert!(!request_docs.contains("Some(value)"));
    }

    #[test]
    fn request_docs_list_endpoint_terminals() {
        let api = test_support::api();
        let docs = endpoint_request_docs(&api, "fetch_item");
        assert!(docs.contains("`.response()`"));
        assert!(docs.contains("`.accepted_bytes()`"));
        assert!(!docs.contains("`.paginate(...)`"));
    }

    #[test]
    fn emitted_markdown_separates_direct_endpoint_and_request_ownership() {
        let api = semantic::test_support::resolve_tokens(quote! {
            client DocsApi {
                base "https://docs.example.test";
                credentials {
                    secret token;
                    credential bearer = bearer(secret.token);
                }
                timeout 20s;

            POST submit(
                body body: Input using Json<Input>,
                optional?: bool,
            ) -> Output using Json<Output> {
                auth { bearer bearer; }
                expose accepted_bytes;
            }
            GET stream() -> ::concord::response::ByteStream using Stream<OctetStream>;
            GET discard() -> () using NoContent {
                timeout off;
            }
            POST pages(
                cursor?: String,
                body filter: Filter using Json<Filter>,
            ) -> Page using Json<Page> {
                paginate with crate::Controller {
                    cursor -> endpoint.cursor;
                }
            }
        }});
        let direct = endpoint_method_docs(&api, "submit");
        assert!(direct.starts_with("`POST /`"));
        assert!(direct.contains("Creates an inert request for [`Submit`]"));
        assert!(direct.contains("Returns [`SubmitRequest`]"));
        assert!(!direct.contains("# Request"));

        let endpoint = endpoint_type_docs(&api, "submit");
        assert!(endpoint.contains("Body argument: `endpoint.body`"));
        assert!(endpoint.contains("Specification: `Json < Input >`"));
        assert!(endpoint.contains("Authentication: Bearer authorization"));
        assert!(endpoint.contains("Timeout: inherited 20s"));

        let request = endpoint_request_docs(&api, "submit");
        assert!(request.contains("`.set_optional(...)`"));
        assert!(request.contains("`.response()`"));
        assert!(request.contains("`.accepted_bytes()`"));
        assert!(!request.contains("`.paginate(...)`"));

        let stream_docs = endpoint_type_docs(&api, "stream");
        assert!(stream_docs.contains("Specification: `Stream < OctetStream >`"));
        assert!(stream_docs.contains("Body: none."));
        assert!(stream_docs.contains("Authentication: none."));

        assert!(endpoint_type_docs(&api, "discard").contains("Specification: `NoContent`"));
        assert!(endpoint_type_docs(&api, "discard").contains("Timeout: disabled"));

        assert!(endpoint_request_docs(&api, "pages").contains("`.paginate(...)`"));
        assert!(endpoint_type_docs(&api, "pages").contains("`PageRequestBodySpec`"));
        assert!(endpoint_type_docs(&api, "pages").contains("page-stable input"));
    }

    #[test]
    fn durations_are_compact_and_exact() {
        assert_eq!(human_duration(Duration::from_secs(20)), "20s");
        assert_eq!(human_duration(Duration::from_millis(250)), "250ms");
        assert_eq!(human_duration(Duration::from_millis(1_500)), "1.5s");
        assert_eq!(human_duration(Duration::from_secs(600)), "10m");
    }
}
