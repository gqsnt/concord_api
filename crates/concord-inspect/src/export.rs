//! `ContractDescription` → `concord.contract/3` structural JSON.

use std::collections::BTreeSet;
use std::time::Duration;

use concord_core::contracts::BodyKind;
use concord_core::inspection::{
    AcceptedStatuses, AtomExpression, AuthOperation, AuthPlacement, BoundDimension,
    CONTRACT_FORMAT, ChallengeRecovery, ContractDescription, CostSource, CredentialSourceKind,
    Deprecation, DiagnosticSeverity, Effect, EffectSource, EffectiveAuth, EffectiveDimension,
    EffectiveKeyOperation, EffectivePolicy, EffectiveRepresentation, EffectiveRoute,
    EffectiveTimeout, Emission, EncoderRole, EndpointContract, FamilyMode, FormalDimension,
    HeadFamily, IdSource, InitValue, InitializerExpression, KeyOperation, LiteralText,
    OwnerDescription, PathOperation, PathSegment, PathTemplate, PlacementTarget, PolicyCoverage,
    PolicyOrigin, RateAttachment, RateAttachmentSource, RateDimensionKind, RepresentationSelection,
    RepresentationSource, ResponseAcquisition, RouteReset, Sensitivity, StatusSource,
    SubdomainOperation, TemplatePart, TimeoutSource, ValueDescription, ValueInitializer,
    ValueLiveness, ValuePresence, ValueRef, ValueRole, WindowDescription,
};
use serde_json::{Value, json};

use crate::canonical::to_canonical_string;
use crate::error::{InspectError, InspectErrorKind};

/// The required top-level keys of a `concord.contract/3` document.
pub(crate) const TOP_LEVEL_KEYS: [&str; 8] = [
    "format",
    "client",
    "owners",
    "endpoints",
    "policies",
    "window_sets",
    "dependencies",
    "diagnostics",
];

/// Builds the `concord.contract/3` structural document of `contract`.
///
/// The document is value-free and deterministic; it reads only the built
/// description and performs no network work or user-code calls.
#[must_use]
pub fn export_value(contract: &ContractDescription) -> Value {
    json!({
        "format": CONTRACT_FORMAT,
        "client": client(contract),
        "owners": contract
            .owners()
            .iter()
            .map(|record| owner(contract, record))
            .collect::<Vec<_>>(),
        "endpoints": contract
            .endpoints()
            .iter()
            .map(|endpoint| endpoint_json(contract, endpoint))
            .collect::<Vec<_>>(),
        "policies": contract
            .policies()
            .iter()
            .map(|policy| policy_json(contract, policy))
            .collect::<Vec<_>>(),
        "window_sets": contract
            .window_sets()
            .iter()
            .map(|set| json!({
                "name": set.name(),
                "windows_source_order": windows(set.windows()),
                "windows_canonical": windows(&set.canonical_windows()),
                "used_by": set.used_by(),
                "docs": set.doc_lines(),
            }))
            .collect::<Vec<_>>(),
        "dependencies": contract
            .dependencies()
            .iter()
            .map(|dependency| {
                let mut record = json!({
                    "kind": dependency.kind().label(),
                    "reference": dependency.reference(),
                    "sites": dependency.sites().iter().map(|site| &**site).collect::<Vec<_>>(),
                });
                if dependency.kind() == concord_core::inspection::DependencyKind::RedactedLiteral {
                    record["classification"] = json!("sensitive");
                }
                record
            })
            .collect::<Vec<_>>(),
        "diagnostics": contract
            .diagnostics()
            .iter()
            .map(|diagnostic| json!({
                "code": diagnostic.code(),
                "severity": severity(diagnostic.severity()),
                "owner": diagnostic.owner(),
                "subject": diagnostic.subject(),
                "message": diagnostic.message(),
                "correction": diagnostic.correction_text(),
            }))
            .collect::<Vec<_>>(),
    })
}

/// Renders the canonical `concord.contract/3` text of `contract`:
/// byte-identical across runs for an equal description.
#[must_use]
pub fn export_string(contract: &ContractDescription) -> String {
    to_canonical_string(&export_value(contract))
}

/// Parses and validates a `concord.contract/3` document.
///
/// # Errors
///
/// Returns [`InspectError`] for invalid JSON, a wrong `format`, a missing or
/// mistyped top-level key, any floating-point number, or invalid/duplicate
/// indexed record keys. Uniqueness checks do not establish full schema validity.
pub fn parse_document(text: &str) -> Result<Value, InspectError> {
    let value = serde_json::from_str::<Value>(text)
        .map_err(|_| InspectError::new(InspectErrorKind::InvalidJson, "snapshot"))?;
    validate_document(&value)?;
    Ok(value)
}

/// Validates top-level structure, the integer/string-only rule and uniqueness
/// of keys used by snapshot diff indexes. This is not complete schema validation.
///
/// # Errors
///
/// Returns [`InspectError`] describing the first violation.
pub fn validate_document(value: &Value) -> Result<(), InspectError> {
    let object = value.as_object().ok_or(InspectError::new(
        InspectErrorKind::UnsupportedFormat,
        "format",
    ))?;
    if object.get("format").and_then(Value::as_str) != Some(CONTRACT_FORMAT) {
        return Err(InspectError::new(
            InspectErrorKind::UnsupportedFormat,
            "format",
        ));
    }
    for key in &TOP_LEVEL_KEYS[1..] {
        let valid = match object.get(*key) {
            Some(Value::Object(_)) => *key == "client",
            Some(Value::Array(_)) => *key != "client",
            _ => false,
        };
        if !valid {
            return Err(InspectError::new(InspectErrorKind::MissingKey, key));
        }
    }
    if contains_float(value) {
        return Err(InspectError::new(
            InspectErrorKind::NonIntegerNumber,
            "number",
        ));
    }
    for (collection, fields) in [
        ("owners", &["path"][..]),
        ("endpoints", &["path"][..]),
        ("policies", &["id"][..]),
        ("window_sets", &["name"][..]),
        ("dependencies", &["kind", "reference"][..]),
        ("diagnostics", &["code", "owner", "subject"][..]),
    ] {
        validate_keys(
            object[collection].as_array().expect("validated collection"),
            fields,
            collection,
        )?;
    }
    let mut endpoint_ids = BTreeSet::new();
    for endpoint in object["endpoints"].as_array().expect("validated endpoints") {
        if let Some(id) = endpoint.get("id") {
            let id = id
                .get("value")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .ok_or(InspectError::new(
                    InspectErrorKind::InvalidRecordKey,
                    "endpoints.id",
                ))?;
            if !endpoint_ids.insert(id) {
                return Err(InspectError::new(
                    InspectErrorKind::DuplicateKey,
                    "endpoints.id",
                ));
            }
        }
    }
    validate_nested_keys(value)?;
    Ok(())
}

// Validate the exact keys used by diff indexing, including its positional
// fallback. Missing keys remain representable; malformed supplied keys and
// collisions with an explicit `#position` are rejected before any indexing.
fn validate_keys(
    items: &[Value],
    fields: &[&str],
    collection: &'static str,
) -> Result<(), InspectError> {
    let mut keys = BTreeSet::new();
    for (position, item) in items.iter().enumerate() {
        // Some exported nested arrays (for example credential secret
        // references) contain scalar names and are indexed positionally.
        // Match the diff's fallback instead of treating them as records.
        let record = item.as_object();
        let mut parts = Vec::new();
        let mut missing = false;
        for field in fields {
            match record.and_then(|record| record.get(*field)) {
                Some(Value::String(value)) if !value.is_empty() => parts.push(value.as_str()),
                None => missing = true,
                _ => {
                    return Err(InspectError::new(
                        InspectErrorKind::InvalidRecordKey,
                        collection,
                    ));
                }
            }
        }
        let key = if missing {
            format!("#{position}")
        } else {
            parts.join(":")
        };
        if !keys.insert(key) {
            return Err(InspectError::new(
                InspectErrorKind::DuplicateKey,
                collection,
            ));
        }
    }
    Ok(())
}

fn validate_nested_keys(value: &Value) -> Result<(), InspectError> {
    match value {
        Value::Object(object) => {
            for (field, nested) in object {
                if let Value::Array(items) = nested {
                    let indexed = match field.as_str() {
                        "values" => Some(("name", "values")),
                        "secrets" => Some(("name", "secrets")),
                        "credentials" => Some(("name", "credentials")),
                        "rate" => Some(("id", "rate")),
                        "coverage" => Some(("id", "coverage")),
                        _ => None,
                    };
                    if let Some((key, collection)) = indexed {
                        validate_keys(items, &[key], collection)?;
                    }
                }
                validate_nested_keys(nested)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                validate_nested_keys(item)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn contains_float(value: &Value) -> bool {
    match value {
        Value::Number(number) => !(number.is_u64() || number.is_i64()),
        Value::Array(items) => items.iter().any(contains_float),
        Value::Object(map) => map.values().any(contains_float),
        _ => false,
    }
}

fn duration(duration: Duration) -> Value {
    json!({ "secs": duration.as_secs(), "nanos": duration.subsec_nanos() })
}

fn deprecation(deprecation: Option<Deprecation>) -> Value {
    deprecation.map_or(
        Value::Null,
        |deprecation| json!({ "since": deprecation.since_text(), "note": deprecation.note_text() }),
    )
}

fn value_ref(reference: &ValueRef) -> Value {
    json!({ "owner": reference.owner_path(), "name": reference.name() })
}

fn redacted(location: &str) -> Value {
    json!({ "redacted": true, "classification": "sensitive", "location": location })
}

fn atom(expression: &AtomExpression, location: &str) -> Value {
    match expression {
        AtomExpression::Literal { kind, text } => match text {
            LiteralText::Text(text) => {
                json!({ "kind": "literal", "literal_kind": kind.label(), "text": text })
            }
            _ => {
                let mut record = redacted(location);
                record["kind"] = json!("literal");
                record["literal_kind"] = json!(kind.label());
                record
            }
        },
        AtomExpression::Value { value, adapter } => {
            json!({ "kind": "value", "value": value_ref(value), "adapter": adapter })
        }
        AtomExpression::Concat(parts) => json!({
            "kind": "concat",
            "parts": parts.iter().map(|part| atom(part, location)).collect::<Vec<_>>(),
        }),
        _ => json!({ "kind": "unknown" }),
    }
}

fn emission(emission: Option<&Emission>, location: &str) -> Value {
    match emission {
        None => Value::Null,
        Some(Emission::Scalar(scalar)) => {
            json!({ "kind": "scalar", "atom": atom(scalar, location) })
        }
        Some(Emission::Each { value, adapter }) => {
            json!({ "kind": "each", "value": value_ref(value), "adapter": adapter })
        }
        Some(_) => json!({ "kind": "unknown" }),
    }
}

fn key_operation(operation: &KeyOperation, location: &str) -> Value {
    json!({
        "kind": operation.kind().label(),
        "key": operation.key(),
        "site": operation.site().label(),
        "emission": emission(operation.emission(), &format!("{location}.emission")),
    })
}

fn template_part(part: &TemplatePart) -> Value {
    match part {
        TemplatePart::Literal(text) => json!({ "kind": "literal", "text": text }),
        TemplatePart::Hole { value, adapter } => {
            json!({ "kind": "hole", "value": value_ref(value), "adapter": adapter })
        }
        _ => json!({ "kind": "unknown" }),
    }
}

fn segment(segment: &PathSegment) -> Value {
    Value::Array(segment.parts().iter().map(template_part).collect())
}

fn path_template(template: &PathTemplate) -> Value {
    json!({
        "text": template.text(),
        "segments": template.segments().iter().map(segment).collect::<Vec<_>>(),
    })
}

fn family_mode(mode: FamilyMode) -> &'static str {
    mode.label()
}

fn target(target: PlacementTarget) -> Value {
    let (name, scheme) = match target {
        PlacementTarget::Header(name) | PlacementTarget::Query(name) => (Some(name), None),
        PlacementTarget::Authorization { scheme } => (None, scheme),
        _ => (None, None),
    };
    json!({ "kind": target.label(), "name": name, "scheme": scheme })
}

fn placement(placement: &AuthPlacement) -> Value {
    json!({
        "target": target(placement.target()),
        "credential": placement.credential(),
        "challenge": placement.challenge_rule().label(),
    })
}

fn window(window: &WindowDescription) -> Value {
    json!({ "capacity": window.capacity(), "duration": duration(window.duration()) })
}

fn windows(windows: &[WindowDescription]) -> Value {
    Value::Array(windows.iter().map(window).collect())
}

fn representation_selection(selection: Option<RepresentationSelection>) -> Value {
    match selection {
        None => Value::Null,
        Some(RepresentationSelection::Type(type_name)) => {
            json!({ "kind": "type", "type": type_name })
        }
        Some(RepresentationSelection::ExplicitFence) => json!({ "kind": "explicit_fence" }),
        Some(_) => json!({ "kind": "unknown" }),
    }
}

fn attachment(attachment: &RateAttachment) -> Value {
    match attachment.source() {
        RateAttachmentSource::Named { policy, bindings } => json!({
            "kind": "named",
            "policy": policy,
            "bindings": bindings
                .iter()
                .map(|binding| json!({
                    "formal": binding.formal(),
                    "value": value_ref(&binding.value()),
                    "encoding_id": binding.encoding_id(),
                }))
                .collect::<Vec<_>>(),
            "cost": attachment.declared_cost(),
        }),
        RateAttachmentSource::Inline {
            id,
            dimensions,
            windows: inline,
            window_set,
        } => json!({
            "kind": "inline",
            "id": id,
            "dimensions": dimensions.iter().map(bound_dimension).collect::<Vec<_>>(),
            "windows_source_order": windows(inline),
            "window_set": window_set,
            "cost": attachment.declared_cost(),
        }),
        _ => json!({ "kind": "unknown" }),
    }
}

fn bound_dimension(dimension: &BoundDimension) -> Value {
    match dimension {
        BoundDimension::Origin => json!({ "kind": "origin" }),
        BoundDimension::Endpoint => json!({ "kind": "endpoint" }),
        BoundDimension::Custom {
            label,
            value,
            encoder,
            encoding_id,
        } => json!({
            "kind": "custom",
            "label": label,
            "value": value_ref(value),
            "encoder": encoder,
            "encoding_id": encoding_id,
        }),
        _ => json!({ "kind": "unknown" }),
    }
}

fn local(owner: &OwnerDescription) -> Value {
    let local = owner.local();
    let path = owner.path();
    let subdomain = match local.subdomain() {
        None => Value::Null,
        Some(operation) => {
            let (mode, labels): (&str, &[AtomExpression]) = match operation {
                SubdomainOperation::Prepend(labels) => ("prepend", labels),
                SubdomainOperation::Replace(labels) => ("replace", labels),
                _ => ("off", &[]),
            };
            json!({
                "mode": mode,
                "labels": labels
                    .iter()
                    .enumerate()
                    .map(|(index, label)| atom(label, &format!("{path}#subdomain[{index}]")))
                    .collect::<Vec<_>>(),
            })
        }
    };
    let path_clause = match local.path() {
        None => Value::Null,
        Some(PathOperation::Append(template)) => {
            json!({ "mode": "append", "template": path_template(template) })
        }
        Some(PathOperation::Replace(template)) => {
            json!({ "mode": "replace", "template": path_template(template) })
        }
        Some(_) => json!({ "mode": "off", "template": null }),
    };
    let family = |family: HeadFamily, label: &str| {
        json!({
            "mode": family_mode(local.mode(family)),
            "operations": local
                .operations(family)
                .iter()
                .enumerate()
                .map(|(index, operation)| key_operation(operation, &format!("{path}#{label}[{index}]")))
                .collect::<Vec<_>>(),
        })
    };
    let auth = match local.auth() {
        None => Value::Null,
        Some(operation) => {
            let (mode, placements): (&str, &[AuthPlacement]) = match operation {
                AuthOperation::Append(placements) => ("append", placements),
                AuthOperation::Replace(placements) => ("replace", placements),
                _ => ("off", &[]),
            };
            json!({
                "mode": mode,
                "placements": placements.iter().map(placement).collect::<Vec<_>>(),
            })
        }
    };
    json!({
        "subdomain": subdomain,
        "path": path_clause,
        "query": family(HeadFamily::Query, "query"),
        "headers": family(HeadFamily::Header, "headers"),
        "auth": auth,
        "timeout": local.timeout().map(duration),
        "request_default": representation_selection(local.request_default()),
        "response_default": representation_selection(local.response_default()),
        "rate_limits": local.rate_limits().iter().map(attachment).collect::<Vec<_>>(),
    })
}

fn value_json(value: &ValueDescription) -> Value {
    let location = format!("{}#values.{}.initializer", value.owner(), value.name());
    let initializer = match value.initializer() {
        ValueInitializer::None => json!({ "kind": "none" }),
        ValueInitializer::TraitDefault => json!({ "kind": "trait_default" }),
        ValueInitializer::StableValue(expression) => {
            let expression = match expression {
                InitializerExpression::Literal(text) => json!({ "kind": "literal", "text": text }),
                InitializerExpression::Constant(path) => {
                    json!({ "kind": "constant", "path": path })
                }
                _ => {
                    let mut record = redacted(&location);
                    record["kind"] = json!("redacted");
                    record
                }
            };
            json!({ "kind": "stable", "expression": expression })
        }
        _ => json!({ "kind": "unknown" }),
    };
    json!({
        "name": value.name(),
        "payload_type": value.payload_type(),
        "presence": presence(value.presence()),
        "initializer": initializer,
        "sensitivity": match value.sensitivity() {
            Sensitivity::Ordinary => "ordinary",
            Sensitivity::Sensitive => "sensitive",
            _ => "unknown",
        },
        "liveness": match value.liveness() {
            ValueLiveness::Live => "live",
            ValueLiveness::RuntimeProjectionOnly => "runtime_projection_only",
            ValueLiveness::DeprecatedNoOp => "deprecated_no_op",
            _ => "unknown",
        },
        "role": match value.role() {
            ValueRole::Constructor => "constructor",
            ValueRole::BuilderRequired => "builder_required",
            ValueRole::Configurable => "configurable",
            _ => "unknown",
        },
        "controls": value.controls().iter().map(|control| control.label()).collect::<Vec<_>>(),
        "constructor_position": value.constructor_position(),
        "declaration_index": value.declaration_index(),
        "docs": value.docs(),
        "deprecation": deprecation(value.deprecation()),
    })
}

fn presence(presence: ValuePresence) -> &'static str {
    match presence {
        ValuePresence::Required => "required",
        ValuePresence::Optional => "optional",
        _ => "unknown",
    }
}

fn owner(contract: &ContractDescription, owner: &OwnerDescription) -> Value {
    let effective = owner.effective().map_or(Value::Null, |view| {
        let path = owner.path();
        let default = |default: concord_core::inspection::EffectiveDefault| {
            json!({
                "type": default.type_name(),
                "owner": default.owner(),
                "fenced": default.fenced(),
            })
        };
        json!({
            "route": route_json(contract, view.route(), path),
            "query": effective_operations(view.operations(HeadFamily::Query), path, "query"),
            "headers": effective_operations(view.operations(HeadFamily::Header), path, "headers"),
            "auth": auth_json(view.auth()),
            "rate": view.policies().iter().map(effective_policy).collect::<Vec<_>>(),
            "timeout": timeout(view.timeout()),
            "request_default": default(view.request_default()),
            "response_default": default(view.response_default()),
        })
    });
    json!({
        "path": owner.path(),
        "kind": owner.kind().label(),
        "name": owner.name(),
        "parent": owner.parent(),
        "children": owner.children(),
        "docs": owner.docs(),
        "deprecation": deprecation(owner.deprecation()),
        "values": owner.values().iter().map(value_json).collect::<Vec<_>>(),
        "local": local(owner),
        "effective": effective,
    })
}

fn client(contract: &ContractDescription) -> Value {
    let base = contract.base();
    json!({
        "name": contract.name(),
        "path": contract.client().path(),
        "base": {
            "literal": base.literal(),
            "scheme": base.scheme(),
            "host": base.host(),
            "port": base.port(),
            "path_prefix": base.path_prefix(),
            "ip_literal": base.is_ip_literal(),
        },
        "timeout": timeout(contract.root_timeout()),
        "secrets": contract
            .secrets()
            .iter()
            .map(|secret| json!({ "name": secret.name(), "docs": secret.doc_lines() }))
            .collect::<Vec<_>>(),
        "credentials": contract
            .credentials()
            .iter()
            .map(|credential| json!({
                "name": credential.name(),
                "kind": source_kind(credential.kind()),
                "secrets": credential.secrets(),
                "provider_type": credential.provider_type(),
                "source_endpoint": credential.source_endpoint(),
                "resolved_source_endpoint": credential.resolved_source_endpoint(),
                "challenge_recovery": recovery(credential.challenge_recovery()),
                "docs": credential.doc_lines(),
            }))
            .collect::<Vec<_>>(),
    })
}

fn source_kind(kind: CredentialSourceKind) -> &'static str {
    kind.label()
}

fn recovery(recovery: ChallengeRecovery) -> &'static str {
    match recovery {
        ChallengeRecovery::FinalRejectsAppliedMaterial => "final_rejects_applied_material",
        ChallengeRecovery::FinalRejectsInstalledVersion => "final_rejects_installed_version",
        ChallengeRecovery::ProviderRecoveryMode => "provider_recovery_mode",
        _ => "unknown",
    }
}

fn timeout(timeout: EffectiveTimeout) -> Value {
    let source = match timeout.source() {
        TimeoutSource::Declared(owner) => json!({ "kind": "declared", "owner": owner }),
        TimeoutSource::RootDefault => json!({ "kind": "root_default" }),
        _ => json!({ "kind": "unknown" }),
    };
    json!({ "duration": duration(timeout.duration()), "source": source })
}

fn representation(representation: &EffectiveRepresentation) -> Value {
    let source = match representation.source() {
        RepresentationSource::Inline => json!({ "kind": "inline" }),
        RepresentationSource::Inherited(owner) => json!({ "kind": "inherited", "owner": owner }),
        _ => json!({ "kind": "unknown" }),
    };
    json!({
        "type": representation.type_name(),
        "source": source,
        "blocking_fence": representation.blocking_fence(),
        "shadowed_default": representation
            .shadowed_default()
            .map(|(owner, type_name)| json!({ "owner": owner, "type": type_name })),
    })
}

fn driver(driver: ResponseAcquisition) -> &'static str {
    match driver {
        ResponseAcquisition::Buffered => "buffered",
        ResponseAcquisition::Streaming => "streaming",
        ResponseAcquisition::Discard => "discard",
    }
}

fn body_family(family: Option<BodyKind>) -> Value {
    match family {
        None => Value::Null,
        Some(BodyKind::Empty) => json!("empty"),
        Some(BodyKind::Buffered) => json!("buffered"),
        Some(BodyKind::Stream) => json!("stream"),
        Some(BodyKind::Multipart) => json!("multipart"),
        Some(_) => json!("unknown"),
    }
}

fn reset(reset: Option<RouteReset>) -> Value {
    reset.map_or(Value::Null, |reset| {
        json!({ "owner": reset.owner(), "mode": if reset.is_off() { "off" } else { "replace" } })
    })
}

fn effective_operation(operation: &EffectiveKeyOperation, location: &str) -> Value {
    let mut record = key_operation(operation.operation(), location);
    record["origin"] = json!(operation.origin());
    record["sensitive"] = json!(operation.sensitive());
    record["conditional_on"] = operation
        .conditional_on()
        .map_or(Value::Null, |source| value_ref(&source));
    record["superseded_when_present"] = Value::Array(
        operation
            .superseded_when_present()
            .iter()
            .map(value_ref)
            .collect(),
    );
    record
}

fn dimension(dimension: &EffectiveDimension) -> Value {
    json!({
        "label": dimension.label(),
        "kind": match dimension.kind() {
            RateDimensionKind::Origin => "origin",
            RateDimensionKind::Endpoint => "endpoint",
            RateDimensionKind::Custom => "custom",
        },
        "binding": dimension.binding().map_or(Value::Null, |value| value_ref(&value)),
        "encoder_role": match dimension.encoder_role() {
            EncoderRole::BuiltinUtf8 => "builtin_utf8",
            EncoderRole::ValueType => "value_type",
            EncoderRole::Encoder => "encoder",
            _ => "unknown",
        },
        "encoder_type": dimension.encoder_type(),
        "encoding_id": dimension.encoding_id(),
    })
}

fn policy_origin(origin: PolicyOrigin) -> Value {
    match origin {
        PolicyOrigin::Named(name) => json!({ "kind": "named", "name": name }),
        PolicyOrigin::Inline => json!({ "kind": "inline" }),
        _ => json!({ "kind": "unknown" }),
    }
}

fn cost_source(source: CostSource) -> &'static str {
    source.label()
}

fn effective_policy(policy: &EffectivePolicy) -> Value {
    json!({
        "id": policy.id(),
        "definition": policy_origin(policy.definition()),
        "origin": policy.origin(),
        "dimensions": policy.dimensions().iter().map(dimension).collect::<Vec<_>>(),
        "cost": policy.cost(),
        "cost_source": cost_source(policy.cost_source()),
        "windows_canonical": windows(&policy.canonical_windows()),
        "windows_source_order": windows(policy.windows()),
        "window_set": policy.window_set(),
    })
}

fn init_value(value: InitValue) -> Value {
    match value {
        InitValue::Literal(text) => json!({ "kind": "literal", "text": text }),
        InitValue::Constant(path) => json!({ "kind": "constant", "path": path }),
        InitValue::Value(reference) => json!({ "kind": "value", "value": value_ref(&reference) }),
        _ => json!({ "kind": "unknown" }),
    }
}

fn effect(effect: Effect) -> &'static str {
    match effect {
        Effect::Safe => "safe",
        Effect::Idempotent => "idempotent",
        Effect::NonIdempotent => "non_idempotent",
        _ => "unknown",
    }
}

fn endpoint_json(contract: &ContractDescription, endpoint: &EndpointContract) -> Value {
    let path = endpoint.path();
    let accepted = match endpoint.accepted() {
        AcceptedStatuses::Success => json!({ "kind": "success" }),
        AcceptedStatuses::Explicit(ranges) => json!({
            "kind": "explicit",
            "ranges": ranges.iter().map(|range| json!([range.start(), range.end()])).collect::<Vec<_>>(),
        }),
        _ => json!({ "kind": "unknown" }),
    };
    let body = endpoint.body().map_or(Value::Null, |body| {
        json!({
            "name": body.name(),
            "input_type": body.input_type(),
            "presence": presence(body.presence()),
            "representation": representation(body.representation()),
            "description": {
                "family": body_family(body.description().family()),
                "media_hint": body.description().media_hint(),
            },
            "page_capable": body.page_capable(),
            "constructor_position": body.constructor_position(),
            "declaration_index": body.declaration_index(),
            "docs": body.docs(),
        })
    });
    json!({
        "path": path,
        "name": endpoint.name(),
        "owner": endpoint.owner(),
        "id": {
            "value": endpoint.id(),
            "source": match endpoint.identity().source() {
                IdSource::Explicit => "explicit",
                IdSource::Derived => "derived",
                _ => "unknown",
            },
            "derivation_inputs": endpoint
                .identity()
                .derivation_inputs()
                .iter()
                .map(|input| &**input)
                .collect::<Vec<_>>(),
        },
        "method": endpoint.method(),
        "effect": {
            "value": effect(endpoint.effect()),
            "source": match endpoint.effect_source() {
                EffectSource::MethodDefault => "method_default",
                EffectSource::Explicit => "explicit",
                _ => "unknown",
            },
        },
        "status": {
            "accepted": accepted,
            "source": match endpoint.status_source() {
                StatusSource::DefaultSuccess => "default_success",
                StatusSource::Explicit => "explicit",
                _ => "unknown",
            },
            "absent_on": endpoint.absent_on(),
        },
        "timeout": timeout(endpoint.timeout()),
        "result": {
            "output_type": endpoint.result().output_type(),
            "representation": representation(endpoint.result().representation()),
            "driver": driver(endpoint.result().driver()),
            "accept": endpoint.result().accept(),
        },
        "body": body,
        "route": route_json(contract, endpoint.route(), path),
        "query": effective_operations(endpoint.operations(HeadFamily::Query), path, "query"),
        "headers": effective_operations(endpoint.operations(HeadFamily::Header), path, "headers"),
        "auth": auth_json(endpoint.auth()),
        "rate": endpoint.policies().iter().map(effective_policy).collect::<Vec<_>>(),
        "pagination": endpoint.pagination().map_or(Value::Null, |pagination| json!({
            "controller": pagination.controller(),
            "init_type": pagination.init_type(),
            "init": pagination
                .init()
                .iter()
                .map(|field| json!({ "field": field.field(), "value": init_value(field.value()) }))
                .collect::<Vec<_>>(),
            "cursor": pagination.cursor_destinations().iter().map(value_ref).collect::<Vec<_>>(),
        })),
        "terminals": {
            "response": endpoint.terminals().response(),
            "accepted_bytes": endpoint.terminals().accepted_bytes(),
            "pagination": endpoint.terminals().pagination(),
            "store_as": endpoint.terminals().store_as(),
        },
    })
}

fn route_json(contract: &ContractDescription, route: &EffectiveRoute, path: &str) -> Value {
    json!({
        "url_template": contract.route_template(route),
        "labels": route
            .labels()
            .iter()
            .enumerate()
            .map(|(index, label)| json!({
                "atom": atom(label.atom(), &format!("{path}#route.labels[{index}]")),
                "origin": label.origin(),
            }))
            .collect::<Vec<_>>(),
        "segments": route
            .segments()
            .iter()
            .map(|route_segment| json!({
                "parts": segment(route_segment.segment()),
                "origin": route_segment.origin(),
            }))
            .collect::<Vec<_>>(),
        "subdomain_reset": reset(route.subdomain_reset()),
        "path_reset": reset(route.path_reset()),
    })
}

fn effective_operations(operations: &[EffectiveKeyOperation], path: &str, label: &str) -> Value {
    Value::Array(
        operations
            .iter()
            .enumerate()
            .map(|(index, operation)| {
                effective_operation(operation, &format!("{path}#effective.{label}[{index}]"))
            })
            .collect(),
    )
}

fn auth_json(auth: &EffectiveAuth) -> Value {
    json!({
        "placements": auth
            .placements()
            .iter()
            .map(|effective| {
                let mut record = placement(&effective.placement());
                record["origin"] = json!(effective.origin());
                record["source_kind"] = json!(source_kind(effective.source_kind()));
                record["recovery"] = json!(recovery(effective.recovery()));
                record["automatic_recovery"] = json!(effective.automatic_recovery_available());
                record
            })
            .collect::<Vec<_>>(),
        "displaced": auth
            .displaced()
            .iter()
            .map(|displaced| {
                let mut record = placement(&displaced.placement());
                record["origin"] = json!(displaced.origin());
                record["displaced_by"] = json!(displaced.displaced_by());
                record["mode"] = json!(displaced.mode().label());
                record
            })
            .collect::<Vec<_>>(),
        "automatic_recovery_available": auth.automatic_recovery_available(),
    })
}

fn formal_dimension(dimension: &FormalDimension) -> Value {
    match dimension {
        FormalDimension::Origin => json!({ "kind": "origin" }),
        FormalDimension::Endpoint => json!({ "kind": "endpoint" }),
        FormalDimension::Custom {
            label,
            formal,
            encoder,
        } => json!({ "kind": "custom", "label": label, "formal": formal, "encoder": encoder }),
        _ => json!({ "kind": "unknown" }),
    }
}

fn policy_json(contract: &ContractDescription, policy: &PolicyCoverage) -> Value {
    let definition = match policy.definition() {
        PolicyOrigin::Named(name) => contract
            .policy_definitions()
            .iter()
            .find(|definition| definition.name() == name)
            .map_or_else(
                || json!({ "kind": "named", "name": name }),
                |definition| {
                    json!({
                        "kind": "named",
                        "name": name,
                        "formals": definition.formals(),
                        "dimensions": definition.dimensions().iter().map(formal_dimension).collect::<Vec<_>>(),
                        "declared_cost": definition.declared_cost(),
                        "inline_windows": windows(definition.inline_windows()),
                        "window_set": definition.window_set(),
                        "docs": definition.doc_lines(),
                    })
                },
            ),
        _ => json!({ "kind": "inline" }),
    };
    json!({
        "id": policy.id(),
        "definition": definition,
        "schema": {
            "dimensions": policy.dimensions().iter().map(dimension).collect::<Vec<_>>(),
            "windows_canonical": windows(&policy.canonical_windows()),
            "windows_source_order": windows(policy.windows()),
            "window_set": policy.window_set(),
        },
        "attachments": policy
            .attachments()
            .iter()
            .map(|site| json!({
                "owner": site.owner(),
                "attachment": attachment(site.attachment()),
            }))
            .collect::<Vec<_>>(),
        "coverage": policy
            .covered()
            .iter()
            .map(|covered| json!({
                "id": covered.id(),
                "path": covered.path(),
                "cost": covered.cost(),
                "attached_at": covered.attached_at(),
            }))
            .collect::<Vec<_>>(),
        "sharing": policy
            .sharing()
            .iter()
            .map(|group| {
                let condition = group.condition();
                json!({
                    "endpoints": group
                        .endpoints()
                        .iter()
                        .map(|path| json!({ "id": endpoint_id(contract, path), "path": path }))
                        .collect::<Vec<_>>(),
                    "condition": {
                        "same_limiter": condition.same_limiter(),
                        "same_policy_id": condition.same_policy_id(),
                        "same_origin": condition.same_origin(),
                        "same_endpoint": condition.same_endpoint(),
                        "labels": condition
                            .labels()
                            .iter()
                            .map(|label| json!({
                                "label": label.label(),
                                "bindings": label
                                    .bindings()
                                    .iter()
                                    .map(|binding| json!({
                                        "endpoint": binding.endpoint(),
                                        "endpoint_id": endpoint_id(contract, binding.endpoint()),
                                        "value": value_ref(&binding.value()),
                                    }))
                                    .collect::<Vec<_>>(),
                            }))
                            .collect::<Vec<_>>(),
                    },
                })
            })
            .collect::<Vec<_>>(),
        "unused": policy.is_unused(),
    })
}

fn endpoint_id(contract: &ContractDescription, path: &str) -> Value {
    contract
        .endpoint_by_path(path)
        .map_or(Value::Null, |endpoint| json!(endpoint.id()))
}

fn severity(severity: DiagnosticSeverity) -> &'static str {
    severity.label()
}
