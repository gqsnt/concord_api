//! Human-readable static effective endpoint and policy views (§18.5).

use std::fmt::Write as _;
use std::time::Duration;

use concord_core::inspection::{
    AtomExpression, ContractDescription, CredentialSourceKind, Emission, EndpointContract,
    HeadFamily, IdSource, InitValue, InitializerExpression, KeyOperationKind, LiteralText,
    OwnerKind, PlacementTarget, PolicyCoverage, PolicyOrigin, REDACTED_MARKER, RateDimensionKind,
    RepresentationSource, ResponseAcquisition, TimeoutSource, ValueInitializer, ValueRef,
    WindowDescription,
};

/// Renders the static effective view of one endpoint: identity, method and
/// logical URL template, normalized query/header programs with provenance,
/// auth, quota, output, traversal and timeout.
///
/// The view states symbolic behavior only; it does not claim a plan was
/// executed or that opaque Rust code is equivalent.
#[must_use]
pub fn explain_endpoint(contract: &ContractDescription, endpoint: &EndpointContract) -> String {
    let mut text = String::new();
    let _ = writeln!(text, "{}", endpoint.path());
    let source = match endpoint.identity().source() {
        IdSource::Explicit => "explicit",
        _ => "navigation-derived; changes on renames and moves",
    };
    let _ = writeln!(text, "  id: {} ({source})", endpoint.id());
    let _ = writeln!(
        text,
        "  {} {}",
        endpoint.method(),
        contract.url_template(endpoint)
    );
    for (family, label) in [
        (HeadFamily::Query, "query"),
        (HeadFamily::Header, "headers"),
    ] {
        let operations = endpoint.operations(family);
        if operations.is_empty() {
            continue;
        }
        let _ = writeln!(text, "  {label}:");
        for operation in operations {
            let line = operation_line(contract, endpoint, operation);
            let _ = writeln!(
                text,
                "    {line:<60} [{}]",
                owner_label(contract, operation.origin())
            );
        }
    }
    explain_auth(&mut text, contract, endpoint);
    for policy in endpoint.policies() {
        let labels = policy
            .dimensions()
            .iter()
            .map(|dimension| match dimension.kind() {
                RateDimensionKind::Origin => "origin".to_owned(),
                RateDimensionKind::Endpoint => "endpoint".to_owned(),
                RateDimensionKind::Custom => format!("label \"{}\"", dimension.label()),
            })
            .collect::<Vec<_>>();
        let _ = writeln!(
            text,
            "  quota: {}; key {:<32} [{}]",
            policy.id(),
            labels.join(" + "),
            owner_label(contract, policy.origin())
        );
        let _ = writeln!(
            text,
            "         {}; cost {}; coverage shown separately",
            windows(&policy.canonical_windows()),
            policy.cost()
        );
    }
    let result = endpoint.result();
    let representation = result.representation();
    let provenance = match representation.source() {
        RepresentationSource::Inline => "inline".to_owned(),
        RepresentationSource::Inherited(owner) => {
            format!("{} default", owner_label(contract, owner))
        }
        _ => "unknown".to_owned(),
    };
    let terminal = match (endpoint.pagination().is_some(), result.driver()) {
        (_, ResponseAcquisition::Streaming) => "managed stream acquisition",
        (_, ResponseAcquisition::Discard) => "discarded accepted body",
        _ => "one-response output",
    };
    let _ = writeln!(
        text,
        "  {terminal}: {} via {:<24} [{provenance}]",
        result.output_type(),
        representation.type_name()
    );
    if let Some(fence) = representation.blocking_fence() {
        let _ = writeln!(text, "    inherited default blocked by fence at {fence}");
    }
    if let Some(codes) = endpoint.absent_on() {
        let codes = codes.iter().map(u16::to_string).collect::<Vec<_>>();
        let _ = writeln!(
            text,
            "  absent on {}: head-only None; the body is not polled",
            codes.join(", ")
        );
    }
    if endpoint.terminals().accepted_bytes() {
        let _ = writeln!(text, "  accepted_bytes: explicit decoder bypass available");
    }
    for credential in endpoint.terminals().store_as() {
        let _ = writeln!(text, "  store_as_{credential}: explicit local installation");
    }
    if let Some(pagination) = endpoint.pagination() {
        let init = pagination
            .init()
            .iter()
            .map(|field| {
                let value = match field.value() {
                    InitValue::Value(value) => value.name().to_owned(),
                    InitValue::Literal(text) | InitValue::Constant(text) => text.to_owned(),
                    _ => "?".to_owned(),
                };
                format!("{}: {value}", field.field())
            })
            .collect::<Vec<_>>();
        let cursor = pagination
            .cursor_destinations()
            .iter()
            .map(|value| value.name())
            .collect::<Vec<_>>();
        let _ = writeln!(
            text,
            "  traversal: {}; Init({}), Cursor -> {}",
            pagination.controller(),
            init.join(", "),
            cursor.join(", ")
        );
    }
    let timeout = endpoint.timeout();
    let origin = match timeout.source() {
        TimeoutSource::Declared(owner) => owner_label(contract, owner),
        _ => "root default".to_owned(),
    };
    let scope = if endpoint.pagination().is_some() {
        "for the whole operation/traversal"
    } else {
        "for the whole operation"
    };
    let _ = writeln!(
        text,
        "  timeout: {} {scope:<30} [{origin}]",
        human_duration(timeout.duration())
    );
    text
}

/// Renders the coverage and possible-sharing view of one policy identity
/// (RATE-02). It never claims that arbitrary tenants or regions share a
/// bucket merely because endpoints appear under one policy.
#[must_use]
pub fn explain_policy(contract: &ContractDescription, policy: &PolicyCoverage) -> String {
    let mut text = String::new();
    let definition = match policy.definition() {
        PolicyOrigin::Named(name) => format!("named policy `{name}`"),
        _ => "inline definition".to_owned(),
    };
    let _ = writeln!(text, "policy {} ({definition})", policy.id());
    let labels = policy
        .dimensions()
        .iter()
        .map(|dimension| match dimension.kind() {
            RateDimensionKind::Custom => format!("\"{}\"", dimension.label()),
            _ => dimension.label().to_owned(),
        })
        .collect::<Vec<_>>();
    let _ = writeln!(text, "  key: [{}]", labels.join(", "));
    let _ = writeln!(text, "  windows: {}", windows(&policy.canonical_windows()));
    if policy.is_unused() {
        let _ = writeln!(text, "  covers: no endpoint (unused)");
        return text;
    }
    let _ = writeln!(text, "  covers:");
    for covered in policy.covered() {
        let _ = writeln!(
            text,
            "    {} ({}) cost {} [attached at {}]",
            covered.id(),
            covered.path(),
            covered.cost(),
            covered.attached_at()
        );
    }
    for group in policy.sharing() {
        let condition = group.condition();
        let mut parts = vec![
            "same limiter service".to_owned(),
            "same policy ID".to_owned(),
        ];
        if condition.same_origin() {
            parts.push("same normalized origin".to_owned());
        }
        if condition.same_endpoint() {
            parts.push("same endpoint".to_owned());
        }
        for label in condition.labels() {
            let bindings = label
                .bindings()
                .iter()
                .map(|binding| contract.display_ref(&binding.value()))
                .collect::<std::collections::BTreeSet<_>>();
            parts.push(format!(
                "same canonical \"{}\" value ({})",
                label.label(),
                bindings.into_iter().collect::<Vec<_>>().join(" | ")
            ));
        }
        let _ = writeln!(
            text,
            "  sharing: {} share a bucket only with {}",
            group.endpoints().join(", "),
            parts.join(", ")
        );
    }
    text
}

fn owner_label(contract: &ContractDescription, path: &str) -> String {
    match contract
        .owner(path)
        .map(|owner| (owner.kind(), owner.name()))
    {
        Some((OwnerKind::Client, _)) => "client".to_owned(),
        Some((OwnerKind::Endpoint, _)) => "endpoint".to_owned(),
        Some((_, name)) => format!("scope {name}"),
        None => path.to_owned(),
    }
}

fn atom_text(contract: &ContractDescription, atom: &AtomExpression) -> String {
    match atom {
        AtomExpression::Literal { text, .. } => match text {
            LiteralText::Text(text) => format!("\"{text}\""),
            _ => REDACTED_MARKER.to_owned(),
        },
        AtomExpression::Value { value, adapter } => match adapter {
            Some(adapter) => format!("{} via {adapter}", contract.display_ref(value)),
            None => contract.display_ref(value),
        },
        AtomExpression::Concat(parts) => format!(
            "concat[{}]",
            parts
                .iter()
                .map(|part| atom_text(contract, part))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        _ => "?".to_owned(),
    }
}

fn initialized(contract: &ContractDescription, value: &ValueRef) -> Option<String> {
    let description = contract.owner(value.owner_path()?)?.value(value.name())?;
    match description.initializer() {
        ValueInitializer::StableValue(InitializerExpression::Literal(text))
        | ValueInitializer::StableValue(InitializerExpression::Constant(text)) => {
            Some(format!("initialized {text}"))
        }
        ValueInitializer::StableValue(_) => Some(format!("initialized {REDACTED_MARKER}")),
        ValueInitializer::TraitDefault => Some("initialized by Default".to_owned()),
        _ => None,
    }
}

fn operation_line(
    contract: &ContractDescription,
    endpoint: &EndpointContract,
    operation: &concord_core::inspection::EffectiveKeyOperation,
) -> String {
    let op = operation.operation();
    let prefix = match op.kind() {
        KeyOperationKind::Replace => "replace ",
        KeyOperationKind::WhenPresent => "when_present ",
        KeyOperationKind::Append => "append ",
        _ => "",
    };
    let mut notes = Vec::new();
    let source = match op.emission() {
        Some(Emission::Scalar(atom)) => atom_text(contract, atom),
        Some(Emission::Each { value, adapter }) => match adapter {
            Some(adapter) => format!("each {} via {adapter}", contract.display_ref(value)),
            None => format!("each {}", contract.display_ref(value)),
        },
        _ => String::new(),
    };
    let source_value = op.emission().and_then(Emission::presence_source).copied();
    match (operation.conditional_on(), op.emission()) {
        (Some(_), _) => notes.push("omitted when absent".to_owned()),
        (None, Some(Emission::Each { .. })) => {
            notes.push("required; empty emits no values".to_owned());
        }
        (None, _) => {
            if let Some(initialized) = source_value.and_then(|value| initialized(contract, &value))
            {
                notes.push(initialized);
            } else {
                notes.push("required".to_owned());
            }
        }
    }
    if let (Some(value), Some(pagination)) = (source_value, endpoint.pagination()) {
        if pagination.cursor_destinations().contains(&value) {
            notes.push("page-controlled".to_owned());
        }
    }
    if !operation.superseded_when_present().is_empty() {
        let refs = operation
            .superseded_when_present()
            .iter()
            .map(|value| contract.display_ref(value))
            .collect::<Vec<_>>();
        notes.push(format!("fallback until {} is present", refs.join(" or ")));
    }
    if operation.sensitive() {
        notes.push("sensitive".to_owned());
    }
    format!("{prefix}{} <- {source}, {}", op.key(), notes.join("; "))
}

fn explain_auth(text: &mut String, contract: &ContractDescription, endpoint: &EndpointContract) {
    let auth = endpoint.auth();
    if auth.placements().is_empty() && auth.displaced().is_empty() {
        return;
    }
    for placement in auth.placements() {
        let target = match placement.placement().target() {
            PlacementTarget::Bearer => "bearer".to_owned(),
            PlacementTarget::Basic => "basic".to_owned(),
            PlacementTarget::Authorization { scheme } => {
                format!("authorization {}", scheme.unwrap_or("(credential scheme)"))
            }
            PlacementTarget::Header(name) => format!("header \"{name}\""),
            PlacementTarget::Query(name) => format!("query \"{name}\""),
            _ => "unknown".to_owned(),
        };
        let displaced = auth
            .displaced()
            .iter()
            .map(|displaced| {
                format!(
                    "{} {} placement {}",
                    owner_label(contract, displaced.origin()),
                    displaced.placement().credential(),
                    match displaced.mode() {
                        concord_core::inspection::AuthResetMode::Off => "removed",
                        _ => "replaced",
                    }
                )
            })
            .collect::<Vec<_>>();
        let mut line = format!("{target} {}", placement.placement().credential());
        if !displaced.is_empty() {
            let _ = write!(line, "; {}", displaced.join("; "));
        }
        let _ = writeln!(
            text,
            "  auth: {line:<58} [{}]",
            owner_label(contract, placement.origin())
        );
        let source = match placement.source_kind() {
            CredentialSourceKind::Endpoint => "endpoint-backed source",
            CredentialSourceKind::CustomProvider => "provider source",
            _ => "static source",
        };
        let recovery = if placement.automatic_recovery_available() {
            "a challenge may trigger one provider recovery replay when every gate permits"
        } else if placement.source_kind() == CredentialSourceKind::Endpoint {
            "a challenge does not auto-login"
        } else {
            "a challenge is final for the call"
        };
        let _ = writeln!(text, "        {source}; {recovery}");
    }
    if auth.placements().is_empty() {
        let _ = writeln!(
            text,
            "  auth: none; inherited authentication removed at {}",
            auth.displaced()
                .iter()
                .map(|displaced| displaced.displaced_by())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}

fn human_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if duration.subsec_nanos() != 0 {
        format!("{} milliseconds", duration.as_millis())
    } else if seconds == 1 {
        "1 second".to_owned()
    } else {
        format!("{seconds} seconds")
    }
}

fn short_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if duration.subsec_nanos() != 0 {
        format!("{}ms", duration.as_millis())
    } else if seconds % 3_600 == 0 {
        format!("{}h", seconds / 3_600)
    } else if seconds % 60 == 0 {
        format!("{}m", seconds / 60)
    } else {
        format!("{seconds}s")
    }
}

fn windows(windows: &[WindowDescription]) -> String {
    windows
        .iter()
        .map(|window| {
            format!(
                "{}/{}",
                window.capacity(),
                short_duration(window.duration())
            )
        })
        .collect::<Vec<_>>()
        .join(" AND ")
}
