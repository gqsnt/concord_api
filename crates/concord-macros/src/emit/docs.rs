use super::*;

pub(super) fn endpoint_method_doc(
    client: &Client,
    scopes: &[&Scope],
    endpoint: &Endpoint,
) -> String {
    let identity = endpoint_identity(client, scopes, endpoint);
    let method = method_name(&endpoint.method);
    let route = route_shape(&endpoint.config.path);
    let controls = endpoint
        .parameters
        .iter()
        .filter(|value| !value.is_required())
        .map(|value| canonical_ident(&value.name))
        .collect::<Vec<_>>();
    let control_doc = if controls.is_empty() {
        "This endpoint has no endpoint-specific request setters.".to_owned()
    } else {
        format!(
            "Use the inherent consuming request setters for: {}.",
            controls.join(", ")
        )
    };
    let values = value_category_summary(&endpoint.parameters, "endpoint");
    let configuration = endpoint_configuration_summary(client, endpoint);
    let features = endpoint_feature_summary(client, endpoint);
    let contract_id = contract_identity_doc(endpoint);
    format!(
        "Creates a side-effect-free pending `{method}` request for `{identity}` at `{route}`. {contract_id} {values} {control_doc} {configuration} {} Direct await uses the `{}` response specification. {} {features}",
        request_body_doc(endpoint.body.as_ref()),
        response_name(&endpoint.response),
        response_doc(&endpoint.response),
    )
}

pub(super) fn endpoint_type_doc(client: &Client, scopes: &[&Scope], endpoint: &Endpoint) -> String {
    let identity = endpoint_identity(client, scopes, endpoint);
    let mut public_path = canonical_ident(&client.surface.support_name);
    for module in &endpoint.surface.module_path {
        public_path.push_str("::");
        public_path.push_str(&canonical_ident(module));
    }
    public_path.push_str("::");
    public_path.push_str(&canonical_ident(&endpoint.surface.endpoint_type_name));
    let method = method_name(&endpoint.method);
    let route = route_shape(&endpoint.config.path);
    let endpoint_values = value_category_summary(&endpoint.parameters, "endpoint");
    let scope_values = if scopes.is_empty() {
        "No enclosing scope values are stored.".to_owned()
    } else {
        scopes
            .iter()
            .map(|scope| value_category_summary(&scope.parameters, "enclosing scope"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let configuration = endpoint_configuration_summary(client, endpoint);
    let features = endpoint_feature_summary(client, endpoint);
    let contract_id = contract_identity_doc(endpoint);
    format!(
        "Explicit `{method}` endpoint snapshot for `{identity}` at route `{route}`. Its predictable public path is `{public_path}` and it executes through the same client request path as its facade method. {contract_id} {scope_values} {endpoint_values} {configuration} {} Direct await uses the `{}` response specification. {} {features}",
        request_body_doc(endpoint.body.as_ref()),
        response_name(&endpoint.response),
        response_doc(&endpoint.response),
    )
}

pub(super) fn endpoint_identity(client: &Client, scopes: &[&Scope], endpoint: &Endpoint) -> String {
    let _ = (client, scopes);
    endpoint.surface.docs_identity.clone()
}

pub(super) fn contract_identity_doc(endpoint: &Endpoint) -> String {
    if endpoint.contract_id.explicit {
        format!(
            "Its explicit stable contract ID is `{}`.",
            endpoint.contract_id.value
        )
    } else {
        format!(
            "Its deterministic source-path contract ID is `{}`; renaming the client, scope, or endpoint changes this default.",
            endpoint.contract_id.value
        )
    }
}

pub(super) fn route_shape(path: &[ResolvedExpr]) -> String {
    let values = path
        .iter()
        .map(|value| match value {
            ResolvedExpr::String(value) => value.value(),
            ResolvedExpr::Integer(value) => value.to_string(),
            ResolvedExpr::Value(value) => match value.reference {
                ValueReference::Client(_) => "{vars.VALUE}".to_owned(),
                ValueReference::Scope { .. } => "{scope.VALUE}".to_owned(),
                ValueReference::Endpoint(_) => "{ep.VALUE}".to_owned(),
            },
            ResolvedExpr::Format(_) => "{fmt[...]}".to_owned(),
        })
        .collect::<Vec<_>>();
    if values.is_empty() {
        "<fixed base path>".to_owned()
    } else {
        format!("/<fixed base>/{}", values.join("/"))
    }
}

pub(super) fn value_category_summary(values: &[Value], owner: &str) -> String {
    let names = |predicate: fn(&Value) -> bool| {
        values
            .iter()
            .filter(|value| predicate(value))
            .map(|value| format!("`{}`", canonical_ident(&value.name)))
            .collect::<Vec<_>>()
    };
    let required = names(Value::is_required);
    let optional = names(Value::is_optional);
    let defaulted = names(|value| value.default().is_some());
    let mut categories = Vec::new();
    if !required.is_empty() {
        categories.push(format!("Required {owner} values: {}.", required.join(", ")));
    }
    if !optional.is_empty() {
        categories.push(format!("Optional {owner} values: {}.", optional.join(", ")));
    }
    if !defaulted.is_empty() {
        categories.push(format!(
            "Defaulted {owner} values: {}; default expressions are intentionally omitted.",
            defaulted.join(", ")
        ));
    }
    if categories.is_empty() {
        format!("No {owner} values are declared.")
    } else {
        categories.join(" ")
    }
}

pub(super) fn retry_summary(retry: &ResolvedRetry) -> &'static str {
    match retry {
        ResolvedRetry::Disabled => "Application native retry is disabled.",
        ResolvedRetry::ReqwestDefault => {
            "Reqwest default native retry is explicitly selected; an HTTP builder retry method may replace it."
        }
    }
}

pub(super) fn endpoint_configuration_summary(client: &Client, endpoint: &Endpoint) -> String {
    let query_count = endpoint.config.query.len();
    let header_count = endpoint.config.headers.len();
    let timeout = endpoint.config.timeout.map_or_else(
        || "No inherited operation deadline is active.".to_owned(),
        |duration| {
            format!(
                "The inherited operation deadline is {} seconds and {} nanoseconds; inherent request setters may override, disable, or restore it.",
                duration.as_secs(),
                duration.subsec_nanos()
            )
        },
    );
    let authentication = if endpoint.config.auth.is_empty() {
        "The effective authentication plan is empty.".to_owned()
    } else {
        let placements = endpoint
            .config
            .auth
            .iter()
            .map(|placement| {
                let category = match placement.kind {
                    ResolvedAuthPlacement::Bearer => "bearer",
                    ResolvedAuthPlacement::Basic => "basic",
                    ResolvedAuthPlacement::Header(_) => "custom header",
                    ResolvedAuthPlacement::Query(_) => "private native query",
                };
                let challenge = match placement.challenge {
                    ResolvedChallengePolicy::Unauthorized => "unauthorized",
                    ResolvedChallengePolicy::UnauthorizedOrForbidden => "unauthorized-or-forbidden",
                    ResolvedChallengePolicy::Never => "never",
                };
                format!("{category} ({challenge})")
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "The effective authentication plan has {} placement(s), in declaration order: {placements}. Credential values are absent from generated documentation.",
            endpoint.config.auth.len()
        )
    };
    let rate_limits = if endpoint.config.rate_limits.is_empty() {
        "The effective rate-limit plan is empty.".to_owned()
    } else {
        let rules = endpoint
            .config
            .rate_limits
            .iter()
            .map(|rule| {
                let windows = rule
                    .windows
                    .iter()
                    .map(|window| format!("{} / {}s", window.capacity, window.duration.as_secs()))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{} (cost {}; windows: {windows})", rule.origin, rule.cost)
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "The effective rate-limit plan has {} rule(s), in exact client-to-scope-to-endpoint composition order: {rules}. Each normal generated rule uses a declaration-scoped global bucket; request values and key parts are absent from generated documentation. Advanced explicit keys are a separate runtime/custom boundary.",
            endpoint.config.rate_limits.len()
        )
    };
    format!(
        "The normalized public head contains {query_count} public query declaration(s) and {header_count} public header declaration(s). {authentication} {rate_limits} {timeout} {}",
        retry_summary(&client.retry)
    )
}

pub(super) fn endpoint_feature_summary(_client: &Client, _endpoint: &Endpoint) -> String {
    "Request, response, and provider feature availability is owned by `concord` type and trait exports.".to_owned()
}

pub(super) fn credential_summary(client: &Client) -> String {
    if client.credentials.is_empty() {
        return "No credential sources are declared.".to_owned();
    }
    let sources = client
        .credentials
        .iter()
        .map(|credential| match credential.source {
            CredentialSource::ApiKey { .. } => "static API key (replace)",
            CredentialSource::Bearer { .. } => "static bearer (replace)",
            CredentialSource::Basic { .. } => "atomic Basic pair (replace)",
            CredentialSource::Endpoint { .. } => "endpoint-backed (set/clear)",
            CredentialSource::Custom { .. } => "custom provider (clear/replace provider)",
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "It declares {} protected secret input(s) and {} credential source(s), in order: {sources}. Source-specific handles expose only their documented capabilities.",
        client.secrets.len(),
        client.credentials.len()
    )
}

pub(super) fn method_name(method: &ResolvedMethod) -> String {
    match method {
        ResolvedMethod::Get => "GET".to_owned(),
        ResolvedMethod::Post => "POST".to_owned(),
        ResolvedMethod::Put => "PUT".to_owned(),
        ResolvedMethod::Patch => "PATCH".to_owned(),
        ResolvedMethod::Delete => "DELETE".to_owned(),
        ResolvedMethod::Head => "HEAD".to_owned(),
        ResolvedMethod::Options => "OPTIONS".to_owned(),
        ResolvedMethod::Trace => "TRACE".to_owned(),
        ResolvedMethod::Connect => "CONNECT".to_owned(),
        ResolvedMethod::Custom(value) => value.value(),
    }
}

pub(super) fn request_body_doc(body: Option<&RequestBody>) -> String {
    match body {
        None => "The request is bodyless.".to_owned(),
        Some(body) => format!(
            "The declared `{}` `#[body]` argument is the input of request specification `{}`. Concord obtains structural metadata before the hook and prepares the managed body only after hook approval.",
            canonical_ident(&body.name),
            body.ty.to_token_stream()
        ),
    }
}

pub(super) fn response_doc(_response: &Type) -> &'static str {
    "Direct await returns the response specification output and `.response()` selects universal successful metadata. Acquisition mode, media validation, limits, and raw-response eligibility are enforced by core-owned capability bounds. The `raw_response()` terminal additionally requires the façade `dangerous-raw-response` feature; when the selected core capability permits it, that terminal returns complete bounded successful bytes with reduced secrecy guarantees."
}

pub(super) fn response_name(response: &Type) -> String {
    response.to_token_stream().to_string()
}

#[cfg(test)]
mod tests {
    use super::super::test_support;
    use super::*;

    #[test]
    fn documents_public_identity_route_and_capabilities_from_the_model() {
        let api = test_support::api();
        let endpoint = test_support::endpoint(&api);
        let docs = endpoint_type_doc(&api.client, &[], endpoint);

        assert!(docs.contains("ArtifactApi::fetch_item"));
        assert!(docs.contains("deterministic source-path contract ID"));
        assert!(docs.contains("authentication plan has 1 placement(s)"));
    }
}
