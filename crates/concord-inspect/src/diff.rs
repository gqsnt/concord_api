//! Semantic, axis-classified comparison of two `concord.contract/3` documents.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use serde_json::{Map, Value, json};

use crate::canonical::to_canonical_string;
use crate::error::InspectError;
use crate::export::{TOP_LEVEL_KEYS, validate_document};

/// The format name of the structural diff report produced by
/// [`DiffReport::to_json`].
pub const DIFF_FORMAT: &str = "concord.contract-diff/3";

/// One semantic change axis (§18.3). A change can belong to several axes.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Axis {
    /// Generated constructors, methods, arity, output types or terminals.
    CallerApi,
    /// Route, host, query, header, method, body media or negotiation.
    Wire,
    /// Outer presence, initializers and retained defaults.
    PresenceAndDefaults,
    /// Credential sources, placements, challenges and recovery.
    AuthAuthority,
    /// Effect, status, timeout, cost, windows and observation policy.
    RuntimePolicy,
    /// Endpoint identity and rate schema (IDs, labels, encodings, windows).
    IdentityAndRateSchema,
    /// Which endpoints a policy covers and which can share buckets.
    SharingCoverage,
    /// Drivers, pagination, accepted bytes and other terminal capabilities.
    Capability,
    /// Declaration sites and inheritance provenance (fences, origins).
    Provenance,
    /// Author documentation, deprecation and diagnostics.
    Documentation,
    /// Opaque Rust types, constants, adapters and redacted literals.
    OpaqueDependency,
    /// A difference the schema cannot classify; never treated as unchanged.
    Unknown,
}

impl Axis {
    /// Every axis in report order.
    pub const ALL: [Self; 12] = [
        Self::CallerApi,
        Self::Wire,
        Self::PresenceAndDefaults,
        Self::AuthAuthority,
        Self::RuntimePolicy,
        Self::IdentityAndRateSchema,
        Self::SharingCoverage,
        Self::Capability,
        Self::Provenance,
        Self::Documentation,
        Self::OpaqueDependency,
        Self::Unknown,
    ];

    /// Returns the stable axis label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::CallerApi => "CallerApi",
            Self::Wire => "Wire",
            Self::PresenceAndDefaults => "PresenceAndDefaults",
            Self::AuthAuthority => "AuthAuthority",
            Self::RuntimePolicy => "RuntimePolicy",
            Self::IdentityAndRateSchema => "IdentityAndRateSchema",
            Self::SharingCoverage => "SharingCoverage",
            Self::Capability => "Capability",
            Self::Provenance => "Provenance",
            Self::Documentation => "Documentation",
            Self::OpaqueDependency => "OpaqueDependency",
            Self::Unknown => "Unknown",
        }
    }
}

/// One endpoint affected by a change.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AffectedEndpoint {
    id: String,
    path: String,
}

impl AffectedEndpoint {
    /// Returns the operational endpoint ID.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the endpoint source path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    fn json(&self) -> Value {
        json!({ "id": self.id, "path": self.path })
    }
}

/// One structural change record.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    collection: &'static str,
    record: String,
    field: String,
    axes: Vec<Axis>,
    endpoints: Vec<AffectedEndpoint>,
    before: Option<Value>,
    after: Option<Value>,
    before_origin: Option<String>,
    after_origin: Option<String>,
}

impl Change {
    /// Returns the top-level collection (`client`, `owners`, `endpoints`, …).
    #[must_use]
    pub const fn collection(&self) -> &'static str {
        self.collection
    }

    /// Returns the record's stable key (source path, policy ID, …).
    #[must_use]
    pub fn record(&self) -> &str {
        &self.record
    }

    /// Returns the changed field path inside the record (empty for an added
    /// or removed record).
    #[must_use]
    pub fn field(&self) -> &str {
        &self.field
    }

    /// Returns the change's axes.
    #[must_use]
    pub fn axes(&self) -> &[Axis] {
        &self.axes
    }

    /// Returns the affected endpoints.
    #[must_use]
    pub fn endpoints(&self) -> &[AffectedEndpoint] {
        &self.endpoints
    }

    /// Returns the structural value before the change.
    #[must_use]
    pub const fn before(&self) -> Option<&Value> {
        self.before.as_ref()
    }

    /// Returns the structural value after the change.
    #[must_use]
    pub const fn after(&self) -> Option<&Value> {
        self.after.as_ref()
    }

    /// Returns the declaration origin of the changed fact before the change.
    #[must_use]
    pub fn before_origin(&self) -> Option<&str> {
        self.before_origin.as_deref()
    }

    /// Returns the declaration origin of the changed fact after the change.
    #[must_use]
    pub fn after_origin(&self) -> Option<&str> {
        self.after_origin.as_deref()
    }

    fn json(&self) -> Value {
        json!({
            "collection": self.collection,
            "record": self.record,
            "field": self.field,
            "axes": self.axes.iter().map(|axis| axis.label()).collect::<Vec<_>>(),
            "endpoints": self.endpoints.iter().map(AffectedEndpoint::json).collect::<Vec<_>>(),
            "before": self.before,
            "after": self.after,
            "before_origin": self.before_origin,
            "after_origin": self.after_origin,
        })
    }
}

/// A comparison that cannot be decided from the structural documents.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Unresolved {
    location: String,
    axis: Axis,
    reason: &'static str,
}

impl Unresolved {
    /// Returns the structural location.
    #[must_use]
    pub fn location(&self) -> &str {
        &self.location
    }

    /// Returns the axis (always [`Axis::OpaqueDependency`] today).
    #[must_use]
    pub const fn axis(&self) -> Axis {
        self.axis
    }

    /// Returns why equality could not be established.
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        self.reason
    }
}

/// The overall outcome of a comparison.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Verdict {
    /// Every represented structural fact is equal and nothing is unresolved.
    StructurallyEqual,
    /// Represented facts are equal but some comparisons (redacted literals)
    /// cannot be decided; this is never reported as equality.
    StructurallyEqualWithUnresolved,
    /// At least one structural change.
    Different,
}

impl Verdict {
    /// Returns the stable verdict label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::StructurallyEqual => "structurally-equal",
            Self::StructurallyEqualWithUnresolved => "structurally-equal-with-unresolved",
            Self::Different => "different",
        }
    }
}

/// The result of comparing two `concord.contract/3` documents.
#[derive(Clone, Debug, PartialEq)]
pub struct DiffReport {
    changes: Vec<Change>,
    unresolved: Vec<Unresolved>,
}

impl DiffReport {
    /// Returns structural changes in deterministic order.
    #[must_use]
    pub fn changes(&self) -> &[Change] {
        &self.changes
    }

    /// Returns unresolved comparisons.
    #[must_use]
    pub fn unresolved(&self) -> &[Unresolved] {
        &self.unresolved
    }

    /// Returns whether no structural change exists (unresolved comparisons
    /// may remain; see [`Self::verdict`]).
    #[must_use]
    pub fn is_structurally_equal(&self) -> bool {
        self.changes.is_empty()
    }

    /// Returns the overall verdict.
    #[must_use]
    pub fn verdict(&self) -> Verdict {
        match (self.changes.is_empty(), self.unresolved.is_empty()) {
            (false, _) => Verdict::Different,
            (true, true) => Verdict::StructurallyEqual,
            (true, false) => Verdict::StructurallyEqualWithUnresolved,
        }
    }

    /// Returns every axis touched by at least one change.
    #[must_use]
    pub fn axes(&self) -> BTreeSet<Axis> {
        self.changes
            .iter()
            .flat_map(|change| change.axes.iter().copied())
            .collect()
    }

    /// Returns whether any change belongs to `axis`.
    #[must_use]
    pub fn has_axis(&self, axis: Axis) -> bool {
        self.changes
            .iter()
            .any(|change| change.axes.contains(&axis))
    }

    /// Returns every affected endpoint, sorted and deduplicated.
    #[must_use]
    pub fn changed_endpoints(&self) -> Vec<AffectedEndpoint> {
        self.changes
            .iter()
            .flat_map(|change| change.endpoints.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// Returns the changes that belong to `axis`.
    #[must_use]
    pub fn changes_on(&self, axis: Axis) -> Vec<&Change> {
        self.changes
            .iter()
            .filter(|change| change.axes.contains(&axis))
            .collect()
    }

    /// Renders the machine-readable report document.
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "format": DIFF_FORMAT,
            "contract_format": crate::CONTRACT_FORMAT,
            "verdict": self.verdict().label(),
            "axes": self.axes().iter().map(|axis| axis.label()).collect::<Vec<_>>(),
            "changed_endpoints": self
                .changed_endpoints()
                .iter()
                .map(AffectedEndpoint::json)
                .collect::<Vec<_>>(),
            "changes": self.changes.iter().map(Change::json).collect::<Vec<_>>(),
            "unresolved": self
                .unresolved
                .iter()
                .map(|unresolved| json!({
                    "location": unresolved.location,
                    "axis": unresolved.axis.label(),
                    "reason": unresolved.reason,
                }))
                .collect::<Vec<_>>(),
        })
    }

    /// Renders the canonical JSON report text.
    #[must_use]
    pub fn to_json_string(&self) -> String {
        to_canonical_string(&self.to_json())
    }

    /// Renders a human-readable report.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut text = String::new();
        let _ = writeln!(
            text,
            "concord contract diff ({}): {} structural change(s), {} unresolved; verdict {}",
            crate::CONTRACT_FORMAT,
            self.changes.len(),
            self.unresolved.len(),
            self.verdict().label()
        );
        let axes = self.axes();
        if !axes.is_empty() {
            let labels = axes.iter().map(|axis| axis.label()).collect::<Vec<_>>();
            let _ = writeln!(text, "axes: {}", labels.join(", "));
        }
        let endpoints = self.changed_endpoints();
        if !endpoints.is_empty() {
            let _ = writeln!(text, "changed endpoints:");
            for endpoint in &endpoints {
                let _ = writeln!(text, "  {} ({})", endpoint.id, endpoint.path);
            }
        }
        if !self.changes.is_empty() {
            let _ = writeln!(text, "changes:");
        }
        for change in &self.changes {
            let axes = change
                .axes
                .iter()
                .map(|axis| axis.label())
                .collect::<Vec<_>>();
            let field = if change.field.is_empty() {
                "<record>"
            } else {
                &change.field
            };
            let _ = writeln!(
                text,
                "  [{}] {} `{}` {}",
                axes.join(", "),
                change.collection,
                change.record,
                field
            );
            let _ = writeln!(
                text,
                "    before: {}{}",
                render(change.before.as_ref()),
                origin_suffix(change.before_origin.as_deref())
            );
            let _ = writeln!(
                text,
                "    after:  {}{}",
                render(change.after.as_ref()),
                origin_suffix(change.after_origin.as_deref())
            );
            if !change.endpoints.is_empty() {
                let ids = change
                    .endpoints
                    .iter()
                    .map(|endpoint| endpoint.id.as_str())
                    .collect::<Vec<_>>();
                let _ = writeln!(text, "    endpoints: {}", ids.join(", "));
            }
        }
        if !self.unresolved.is_empty() {
            let _ = writeln!(text, "unresolved:");
        }
        for unresolved in &self.unresolved {
            let _ = writeln!(
                text,
                "  [{}] {}: {}",
                unresolved.axis.label(),
                unresolved.location,
                unresolved.reason
            );
        }
        text
    }
}

fn render(value: Option<&Value>) -> String {
    value.map_or_else(|| "<absent>".to_owned(), Value::to_string)
}

fn origin_suffix(origin: Option<&str>) -> String {
    origin.map_or_else(String::new, |origin| format!("  [origin {origin}]"))
}

// ---------------------------------------------------------------------------
// Comparison
// ---------------------------------------------------------------------------

struct Document<'a> {
    root: &'a Map<String, Value>,
    endpoints: Vec<(String, String)>,
}

/// Owner source-path renames implied by paired endpoint moves: each moved
/// endpoint path pair plus every renamed ancestor prefix pair. A value
/// reference whose owner changes only by such a rename is a navigation
/// change, not a wire change.
type Renames = BTreeSet<(String, String)>;

fn renames_from_moves(moves: &[(String, String)]) -> Renames {
    let mut renames = BTreeSet::new();
    for (from, to) in moves {
        let mut from = from.as_str();
        let mut to = to.as_str();
        while from != to {
            renames.insert((from.to_owned(), to.to_owned()));
            match (from.rsplit_once("::"), to.rsplit_once("::")) {
                (Some((from_parent, from_last)), Some((to_parent, to_last)))
                    if from_last == to_last =>
                {
                    from = from_parent;
                    to = to_parent;
                }
                _ => break,
            }
        }
    }
    renames
}

impl<'a> Document<'a> {
    fn new(value: &'a Value) -> Self {
        let root = value.as_object().expect("validated document object");
        let endpoints = array(root, "endpoints")
            .iter()
            .filter_map(|endpoint| {
                Some((
                    endpoint.get("path")?.as_str()?.to_owned(),
                    endpoint.pointer("/id/value")?.as_str()?.to_owned(),
                ))
            })
            .collect();
        Self { root, endpoints }
    }
}

fn array<'a>(root: &'a Map<String, Value>, key: &str) -> &'a [Value] {
    root.get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

struct Raw {
    field: String,
    before: Option<Value>,
    after: Option<Value>,
}

/// Compares two `concord.contract/3` documents (`before` → `after`).
///
/// Records are matched by their stable structural keys; endpoints whose
/// source path changed are paired as moves when their name, method and
/// output type identify them uniquely. Every difference is classified into
/// one or more [`Axis`] values ([`Axis::Unknown`] when the schema cannot
/// classify it). Redacted literals present on both sides are reported as
/// [`Unresolved`], never as equal.
///
/// # Errors
///
/// Returns [`InspectError`] when either document is not a valid
/// `concord.contract/3` document.
pub fn diff_documents(before: &Value, after: &Value) -> Result<DiffReport, InspectError> {
    validate_document(before)?;
    validate_document(after)?;
    let before = Document::new(before);
    let after = Document::new(after);
    let mut changes = Vec::new();

    let keys = before
        .root
        .keys()
        .chain(after.root.keys())
        .collect::<BTreeSet<_>>();
    for key in keys {
        if TOP_LEVEL_KEYS.contains(&key.as_str()) {
            continue;
        }
        if before.root.get(key) != after.root.get(key) {
            changes.push(Change {
                collection: "document",
                record: key.clone(),
                field: String::new(),
                axes: vec![Axis::Unknown],
                endpoints: Vec::new(),
                before: before.root.get(key).cloned(),
                after: after.root.get(key).cloned(),
                before_origin: None,
                after_origin: None,
            });
        }
    }

    let moves = endpoint_moves(&before, &after);
    let renames = renames_from_moves(&moves);

    let empty = Value::Object(Map::new());
    let client_before = before.root.get("client").unwrap_or(&empty);
    let client_after = after.root.get("client").unwrap_or(&empty);
    let client_path = client_after
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    push_record_changes(
        &mut changes,
        &before,
        &after,
        &renames,
        "client",
        &client_path,
        Some(client_before),
        Some(client_after),
    );

    diff_keyed(
        &mut changes,
        &before,
        &after,
        &renames,
        "owners",
        |record| string(record, "path"),
    );
    diff_endpoints(&mut changes, &before, &after, &renames, &moves);
    diff_keyed(
        &mut changes,
        &before,
        &after,
        &renames,
        "policies",
        |record| string(record, "id"),
    );
    diff_keyed(
        &mut changes,
        &before,
        &after,
        &renames,
        "window_sets",
        |record| string(record, "name"),
    );
    diff_keyed(
        &mut changes,
        &before,
        &after,
        &renames,
        "dependencies",
        |record| {
            Some(format!(
                "{}:{}",
                string(record, "kind")?,
                string(record, "reference")?
            ))
        },
    );
    diff_keyed(
        &mut changes,
        &before,
        &after,
        &renames,
        "diagnostics",
        |record| {
            Some(format!(
                "{}:{}:{}",
                string(record, "code")?,
                string(record, "owner")?,
                string(record, "subject")?
            ))
        },
    );

    let mut unresolved = Vec::new();
    let redacted = |document: &Document<'_>| {
        array(document.root, "dependencies")
            .iter()
            .filter(|record| string(record, "kind").as_deref() == Some("redacted_literal"))
            .filter_map(|record| string(record, "reference"))
            .collect::<BTreeSet<_>>()
    };
    for location in redacted(&before).intersection(&redacted(&after)) {
        unresolved.push(Unresolved {
            location: location.clone(),
            axis: Axis::OpaqueDependency,
            reason: "redacted sensitive literal cannot be compared; equality is not asserted",
        });
    }

    Ok(DiffReport {
        changes,
        unresolved,
    })
}

fn string(record: &Value, key: &str) -> Option<String> {
    record.get(key)?.as_str().map(str::to_owned)
}

fn diff_keyed(
    changes: &mut Vec<Change>,
    before: &Document<'_>,
    after: &Document<'_>,
    renames: &Renames,
    collection: &'static str,
    key: impl Fn(&Value) -> Option<String>,
) {
    let before_index = index_records(before.root, collection, &key);
    let after_index = index_records(after.root, collection, &key);
    let keys = before_index
        .keys()
        .chain(after_index.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    for record_key in keys {
        push_record_changes(
            changes,
            before,
            after,
            renames,
            collection,
            &record_key,
            before_index.get(&record_key).copied(),
            after_index.get(&record_key).copied(),
        );
    }
}

fn index_records<'a>(
    root: &'a Map<String, Value>,
    collection: &str,
    key: &impl Fn(&Value) -> Option<String>,
) -> BTreeMap<String, &'a Value> {
    array(root, collection)
        .iter()
        .enumerate()
        .map(|(position, record)| {
            (
                key(record).unwrap_or_else(|| format!("#{position}")),
                record,
            )
        })
        .collect()
}

fn index_items<'a>(items: &'a [Value], key: &str) -> BTreeMap<String, &'a Value> {
    items
        .iter()
        .enumerate()
        .map(|(position, item)| {
            (
                item.get(key)
                    .and_then(Value::as_str)
                    .map_or_else(|| format!("#{position}"), str::to_owned),
                item,
            )
        })
        .collect()
}

fn endpoint_moves(before: &Document<'_>, after: &Document<'_>) -> Vec<(String, String)> {
    let path_key = |record: &Value| string(record, "path");
    let before_index = index_records(before.root, "endpoints", &path_key);
    let after_index = index_records(after.root, "endpoints", &path_key);
    let identity = |record: &Value| {
        (
            string(record, "name"),
            string(record, "method"),
            record
                .pointer("/result/output_type")
                .and_then(Value::as_str)
                .map(str::to_owned),
        )
    };
    let removed = before_index
        .iter()
        .filter(|(path, _)| !after_index.contains_key(*path))
        .collect::<Vec<_>>();
    let added = after_index
        .iter()
        .filter(|(path, _)| !before_index.contains_key(*path))
        .collect::<Vec<_>>();
    let mut moves = Vec::new();
    for (before_path, before_record) in &removed {
        let candidates = added
            .iter()
            .filter(|(_, record)| identity(record) == identity(before_record))
            .collect::<Vec<_>>();
        let reverse = removed
            .iter()
            .filter(|(_, record)| identity(record) == identity(before_record))
            .count();
        if candidates.len() == 1 && reverse == 1 {
            moves.push(((*before_path).clone(), candidates[0].0.clone()));
        }
    }
    moves
}

fn diff_endpoints(
    changes: &mut Vec<Change>,
    before: &Document<'_>,
    after: &Document<'_>,
    renames: &Renames,
    moves: &[(String, String)],
) {
    let path_key = |record: &Value| string(record, "path");
    let before_index = index_records(before.root, "endpoints", &path_key);
    let after_index = index_records(after.root, "endpoints", &path_key);
    let moved_before = moves
        .iter()
        .map(|(from, _)| from.clone())
        .collect::<BTreeSet<_>>();
    let moved_after = moves
        .iter()
        .map(|(_, to)| to.clone())
        .collect::<BTreeSet<_>>();

    let keys = before_index
        .keys()
        .chain(after_index.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    for path in keys {
        if moved_before.contains(&path) || moved_after.contains(&path) {
            continue;
        }
        push_record_changes(
            changes,
            before,
            after,
            renames,
            "endpoints",
            &path,
            before_index.get(&path).copied(),
            after_index.get(&path).copied(),
        );
    }
    for (from, to) in moves {
        push_record_changes(
            changes,
            before,
            after,
            renames,
            "endpoints",
            &format!("{from} -> {to}"),
            before_index.get(from).copied(),
            after_index.get(to).copied(),
        );
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "one record comparison reads both documents, both records and the rename map"
)]
fn push_record_changes(
    changes: &mut Vec<Change>,
    before: &Document<'_>,
    after: &Document<'_>,
    renames: &Renames,
    collection: &'static str,
    record: &str,
    before_record: Option<&Value>,
    after_record: Option<&Value>,
) {
    let mut raw = Vec::new();
    match (before_record, after_record) {
        (Some(left), Some(right)) => compare_values("", left, right, &mut raw),
        (left, right) => raw.push(Raw {
            field: String::new(),
            before: left.cloned(),
            after: right.cloned(),
        }),
    }
    for change in raw {
        let normalized = normalize_field(&change.field);
        let renamed_owner = normalized
            .rsplit('.')
            .next()
            .is_some_and(|last| last == "owner")
            && match (&change.before, &change.after) {
                (Some(Value::String(from)), Some(Value::String(to))) => {
                    renames.contains(&(from.clone(), to.clone()))
                }
                _ => false,
            };
        let axes = if renamed_owner {
            vec![Axis::CallerApi, Axis::Provenance]
        } else {
            classify(collection, &normalized)
        };
        let endpoints = affected_endpoints(
            before,
            after,
            collection,
            record,
            &change.field,
            before_record,
            after_record,
        );
        changes.push(Change {
            collection,
            record: record.to_owned(),
            before_origin: before_record.and_then(|value| origin(collection, value, &change.field)),
            after_origin: after_record.and_then(|value| origin(collection, value, &change.field)),
            field: change.field,
            axes,
            endpoints,
            before: change.before,
            after: change.after,
        });
    }
}

fn join(field: &str, key: &str) -> String {
    if field.is_empty() {
        key.to_owned()
    } else {
        format!("{field}.{key}")
    }
}

/// Arrays whose elements are matched by a key field rather than position.
fn keyed_array(field: &str) -> Option<&'static str> {
    let last = field.rsplit('.').next().unwrap_or(field);
    let last = last.split('[').next().unwrap_or(last);
    match last {
        "values" | "secrets" | "credentials" => Some("name"),
        "rate" => Some("id"),
        "coverage" => Some("id"),
        _ => None,
    }
}

fn compare_values(field: &str, before: &Value, after: &Value, out: &mut Vec<Raw>) {
    if before == after {
        return;
    }
    match (before, after) {
        (Value::Object(left), Value::Object(right)) => {
            let keys = left.keys().chain(right.keys()).collect::<BTreeSet<_>>();
            for key in keys {
                let nested = join(field, key);
                match (left.get(key), right.get(key)) {
                    (Some(left), Some(right)) => compare_values(&nested, left, right, out),
                    (left, right) => out.push(Raw {
                        field: nested,
                        before: left.cloned(),
                        after: right.cloned(),
                    }),
                }
            }
        }
        (Value::Array(left), Value::Array(right)) => {
            if let Some(key) = keyed_array(field) {
                let left_index = index_items(left, key);
                let right_index = index_items(right, key);
                let keys = left_index
                    .keys()
                    .chain(right_index.keys())
                    .collect::<BTreeSet<_>>();
                for item_key in keys {
                    let nested = format!("{field}[{item_key}]");
                    match (left_index.get(item_key), right_index.get(item_key)) {
                        (Some(left), Some(right)) => compare_values(&nested, left, right, out),
                        (left, right) => out.push(Raw {
                            field: nested,
                            before: left.map(|value| (*value).clone()),
                            after: right.map(|value| (*value).clone()),
                        }),
                    }
                }
            } else if left.len() == right.len() {
                for (position, (left, right)) in left.iter().zip(right).enumerate() {
                    compare_values(&format!("{field}[{position}]"), left, right, out);
                }
            } else {
                out.push(Raw {
                    field: field.to_owned(),
                    before: Some(before.clone()),
                    after: Some(after.clone()),
                });
            }
        }
        _ => out.push(Raw {
            field: field.to_owned(),
            before: Some(before.clone()),
            after: Some(after.clone()),
        }),
    }
}

fn normalize_field(field: &str) -> String {
    let mut normalized = String::with_capacity(field.len());
    let mut depth = 0_usize;
    for character in field.chars() {
        match character {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            character if depth == 0 => normalized.push(character),
            _ => {}
        }
    }
    normalized
}

/// Walks `record` along `field` and returns the nearest declaration origin:
/// an `origin`, `attached_at` or `owner` string, or a `source.owner`.
fn origin(collection: &str, record: &Value, field: &str) -> Option<String> {
    let mut found = match collection {
        "owners" | "endpoints" => string(record, "path"),
        "client" => string(record, "path"),
        "diagnostics" => string(record, "owner"),
        _ => None,
    };
    let mut current = record;
    for segment in split_field(field) {
        let next = match segment {
            Segment::Key(key) => current.get(key),
            Segment::Index(index) => current.get(index),
            Segment::Keyed(key) => current.as_array().and_then(|items| {
                items.iter().find(|item| {
                    ["name", "id", "path"]
                        .iter()
                        .any(|candidate| item.get(*candidate).and_then(Value::as_str) == Some(key))
                })
            }),
        };
        let Some(next) = next else {
            break;
        };
        current = next;
        if let Some(value) = local_origin(current) {
            found = Some(value);
        }
    }
    found
}

fn local_origin(value: &Value) -> Option<String> {
    for key in ["origin", "attached_at", "owner"] {
        if let Some(text) = value.get(key).and_then(Value::as_str) {
            return Some(text.to_owned());
        }
    }
    value
        .get("source")
        .and_then(|source| source.get("owner"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

enum Segment<'a> {
    Key(&'a str),
    Index(usize),
    Keyed(&'a str),
}

fn split_field(field: &str) -> Vec<Segment<'_>> {
    let mut segments = Vec::new();
    for part in field.split('.').filter(|part| !part.is_empty()) {
        let (key, rest) = part.split_once('[').unwrap_or((part, ""));
        if !key.is_empty() {
            segments.push(Segment::Key(key));
        }
        for bracket in rest.split('[') {
            let inner = bracket.trim_end_matches(']');
            if inner.is_empty() {
                continue;
            }
            match inner.parse::<usize>() {
                Ok(index) => segments.push(Segment::Index(index)),
                Err(_) => segments.push(Segment::Keyed(inner)),
            }
        }
    }
    segments
}

fn endpoints_under(document: &Document<'_>, owner: &str) -> Vec<(String, String)> {
    let prefix = format!("{owner}::");
    document
        .endpoints
        .iter()
        .filter(|(path, _)| path == owner || path.starts_with(&prefix))
        .cloned()
        .collect()
}

#[allow(
    clippy::too_many_arguments,
    reason = "affected-endpoint resolution reads both documents and both records"
)]
fn affected_endpoints(
    before: &Document<'_>,
    after: &Document<'_>,
    collection: &str,
    record: &str,
    field: &str,
    before_record: Option<&Value>,
    after_record: Option<&Value>,
) -> Vec<AffectedEndpoint> {
    let mut found = BTreeSet::new();
    let documents = [before, after];
    let records = [before_record, after_record];
    match collection {
        "endpoints" => {
            for value in records.into_iter().flatten() {
                if let (Some(path), Some(id)) = (
                    string(value, "path"),
                    value.pointer("/id/value").and_then(Value::as_str),
                ) {
                    found.insert((path, id.to_owned()));
                }
            }
        }
        "owners" | "diagnostics" => {
            let owner = if collection == "owners" {
                record.to_owned()
            } else {
                records
                    .into_iter()
                    .flatten()
                    .find_map(|value| string(value, "owner"))
                    .unwrap_or_default()
            };
            for document in documents {
                found.extend(endpoints_under(document, &owner));
            }
        }
        "client" => {
            let credential = split_field(field).into_iter().nth(1).and_then(|segment| {
                match (field.starts_with("credentials"), segment) {
                    (true, Segment::Keyed(name)) => Some(name.to_owned()),
                    _ => None,
                }
            });
            for document in documents {
                for endpoint in array(document.root, "endpoints") {
                    let uses = credential.as_ref().is_none_or(|name| {
                        endpoint
                            .pointer("/auth/placements")
                            .and_then(Value::as_array)
                            .is_some_and(|placements| {
                                placements.iter().any(|placement| {
                                    placement.get("credential").and_then(Value::as_str)
                                        == Some(name.as_str())
                                })
                            })
                            || endpoint
                                .pointer("/terminals/store_as")
                                .and_then(Value::as_array)
                                .is_some_and(|names| {
                                    names
                                        .iter()
                                        .any(|stored| stored.as_str() == Some(name.as_str()))
                                })
                    });
                    if uses {
                        if let (Some(path), Some(id)) = (
                            string(endpoint, "path"),
                            endpoint.pointer("/id/value").and_then(Value::as_str),
                        ) {
                            found.insert((path, id.to_owned()));
                        }
                    }
                }
            }
        }
        "policies" => {
            for value in records.into_iter().flatten() {
                for covered in value
                    .get("coverage")
                    .and_then(Value::as_array)
                    .map_or(&[][..], Vec::as_slice)
                {
                    if let (Some(path), Some(id)) = (string(covered, "path"), string(covered, "id"))
                    {
                        found.insert((path, id));
                    }
                }
            }
        }
        "window_sets" => {
            for document in documents {
                for endpoint in array(document.root, "endpoints") {
                    let uses =
                        endpoint
                            .get("rate")
                            .and_then(Value::as_array)
                            .is_some_and(|policies| {
                                policies.iter().any(|policy| {
                                    policy.get("window_set").and_then(Value::as_str) == Some(record)
                                })
                            });
                    if uses {
                        if let (Some(path), Some(id)) = (
                            string(endpoint, "path"),
                            endpoint.pointer("/id/value").and_then(Value::as_str),
                        ) {
                            found.insert((path, id.to_owned()));
                        }
                    }
                }
            }
        }
        "dependencies" => {
            let owners = records
                .into_iter()
                .flatten()
                .filter_map(|value| value.get("sites").and_then(Value::as_array))
                .flatten()
                .filter_map(Value::as_str)
                .map(|site| site.split('#').next().unwrap_or(site).to_owned())
                .collect::<BTreeSet<_>>();
            for owner in owners {
                for document in documents {
                    found.extend(endpoints_under(document, &owner));
                }
            }
        }
        _ => {}
    }
    found
        .into_iter()
        .map(|(path, id)| AffectedEndpoint { id, path })
        .collect()
}

// ---------------------------------------------------------------------------
// Axis classification
// ---------------------------------------------------------------------------

/// One classification rule: an exact field or a field prefix.
struct Rule {
    field: &'static str,
    exact: bool,
    axes: &'static [Axis],
}

const fn exact(field: &'static str, axes: &'static [Axis]) -> Rule {
    Rule {
        field,
        exact: true,
        axes,
    }
}

const fn prefix(field: &'static str, axes: &'static [Axis]) -> Rule {
    Rule {
        field,
        exact: false,
        axes,
    }
}

use Axis::{
    AuthAuthority, CallerApi, Capability, Documentation, IdentityAndRateSchema, OpaqueDependency,
    PresenceAndDefaults, Provenance, RuntimePolicy, SharingCoverage, Wire,
};

const CLIENT_RULES: &[Rule] = &[
    prefix("name", &[CallerApi, IdentityAndRateSchema]),
    prefix("path", &[CallerApi, IdentityAndRateSchema]),
    prefix("base", &[Wire]),
    prefix("timeout.source", &[Provenance]),
    prefix("timeout", &[RuntimePolicy]),
    prefix("secrets.docs", &[Documentation]),
    prefix("secrets", &[CallerApi, AuthAuthority]),
    prefix("credentials.docs", &[Documentation]),
    prefix(
        "credentials.provider_type",
        &[AuthAuthority, OpaqueDependency],
    ),
    exact("credentials", &[AuthAuthority, CallerApi]),
    prefix("credentials", &[AuthAuthority]),
];

const OWNER_RULES: &[Rule] = &[
    exact("", &[CallerApi, Provenance]),
    exact("values", &[CallerApi, PresenceAndDefaults]),
    prefix("values.docs", &[Documentation]),
    prefix("values.deprecation", &[Documentation, CallerApi]),
    prefix("values.payload_type", &[CallerApi]),
    prefix("values.presence", &[PresenceAndDefaults, CallerApi]),
    prefix("values.initializer", &[PresenceAndDefaults]),
    prefix("values.sensitivity", &[RuntimePolicy]),
    prefix("values.liveness", &[PresenceAndDefaults, Documentation]),
    prefix("values", &[CallerApi]),
    prefix("docs", &[Documentation]),
    prefix("deprecation", &[Documentation, CallerApi]),
    prefix("local", &[Provenance]),
    prefix("kind", &[CallerApi, Provenance]),
    prefix("name", &[CallerApi, Provenance]),
    prefix("parent", &[CallerApi, Provenance]),
    prefix("children", &[CallerApi, Provenance]),
    prefix("path", &[CallerApi, Provenance]),
];

const ENDPOINT_RULES: &[Rule] = &[
    exact("", &[CallerApi, Wire, IdentityAndRateSchema, Capability]),
    prefix("path", &[CallerApi, Provenance]),
    prefix("name", &[CallerApi, Provenance]),
    prefix("owner", &[CallerApi, Provenance]),
    prefix("id.derivation_inputs", &[Provenance]),
    prefix("id", &[IdentityAndRateSchema]),
    prefix("method", &[Wire]),
    prefix("effect.source", &[Provenance]),
    prefix("effect", &[RuntimePolicy]),
    prefix("status.source", &[Provenance]),
    prefix("status.absent_on", &[RuntimePolicy, CallerApi]),
    prefix("status", &[RuntimePolicy]),
    prefix("timeout.source", &[Provenance]),
    prefix("timeout", &[RuntimePolicy]),
    prefix("result.output_type", &[CallerApi]),
    prefix("result.representation.type", &[Wire, Capability]),
    prefix("result.representation", &[Provenance]),
    prefix("result.driver", &[Capability]),
    prefix("result.accept", &[Wire]),
    exact("body", &[CallerApi, Wire]),
    prefix("body.docs", &[Documentation]),
    prefix("body.representation.type", &[Wire]),
    prefix("body.representation", &[Provenance]),
    prefix("body.description", &[Wire]),
    prefix("body.page_capable", &[Capability]),
    prefix("body.presence", &[CallerApi, PresenceAndDefaults]),
    prefix("body", &[CallerApi]),
    prefix("route.labels.origin", &[Provenance]),
    prefix("route.segments.origin", &[Provenance]),
    prefix("route.subdomain_reset", &[Provenance]),
    prefix("route.path_reset", &[Provenance]),
    prefix("route", &[Wire]),
    prefix("query.origin", &[Provenance]),
    prefix("query.site", &[Provenance]),
    prefix("query.sensitive", &[RuntimePolicy]),
    prefix("query", &[Wire]),
    prefix("headers.origin", &[Provenance]),
    prefix("headers.site", &[Provenance]),
    prefix("headers.sensitive", &[RuntimePolicy]),
    prefix("headers", &[Wire]),
    prefix("auth.placements.origin", &[Provenance]),
    prefix("auth.placements.target", &[AuthAuthority, Wire]),
    prefix(
        "auth.placements.automatic_recovery",
        &[AuthAuthority, RuntimePolicy],
    ),
    prefix(
        "auth.automatic_recovery_available",
        &[AuthAuthority, RuntimePolicy],
    ),
    prefix("auth.displaced", &[AuthAuthority, Provenance]),
    prefix("auth", &[AuthAuthority]),
    exact("rate", &[IdentityAndRateSchema, RuntimePolicy]),
    prefix("rate.origin", &[Provenance]),
    prefix("rate.definition", &[Provenance]),
    prefix("rate.cost_source", &[Provenance]),
    prefix("rate.window_set", &[Provenance]),
    prefix("rate.windows_source_order", &[Provenance]),
    prefix("rate.cost", &[RuntimePolicy]),
    prefix(
        "rate.windows_canonical",
        &[RuntimePolicy, IdentityAndRateSchema],
    ),
    prefix(
        "rate.dimensions.encoder_type",
        &[IdentityAndRateSchema, OpaqueDependency],
    ),
    prefix("rate", &[IdentityAndRateSchema]),
    prefix("pagination", &[Capability]),
    prefix("terminals", &[Capability, CallerApi]),
];

const POLICY_RULES: &[Rule] = &[
    exact("", &[IdentityAndRateSchema, SharingCoverage]),
    prefix("definition.docs", &[Documentation]),
    prefix("definition", &[Provenance]),
    prefix("schema.windows_source_order", &[Provenance]),
    prefix("schema.window_set", &[Provenance]),
    prefix(
        "schema.windows_canonical",
        &[IdentityAndRateSchema, RuntimePolicy],
    ),
    prefix(
        "schema.dimensions.encoder_type",
        &[IdentityAndRateSchema, OpaqueDependency],
    ),
    prefix("schema", &[IdentityAndRateSchema]),
    prefix("attachments", &[Provenance]),
    prefix("coverage.cost", &[RuntimePolicy]),
    prefix("coverage.attached_at", &[Provenance]),
    prefix("coverage.path", &[CallerApi, Provenance]),
    prefix("coverage", &[SharingCoverage]),
    prefix("sharing.endpoints.path", &[CallerApi, Provenance]),
    prefix("sharing.condition.labels.label", &[IdentityAndRateSchema]),
    prefix(
        "sharing.condition.labels.bindings.endpoint",
        &[CallerApi, Provenance],
    ),
    prefix("sharing", &[SharingCoverage]),
    prefix("unused", &[SharingCoverage]),
];

const WINDOW_SET_RULES: &[Rule] = &[
    exact("", &[RuntimePolicy]),
    prefix("docs", &[Documentation]),
    prefix("used_by", &[Provenance]),
    prefix("windows_source_order", &[Provenance]),
    prefix("windows_canonical", &[RuntimePolicy]),
];

const DEPENDENCY_RULES: &[Rule] = &[
    exact("", &[OpaqueDependency]),
    prefix("sites", &[Provenance]),
    prefix("", &[OpaqueDependency]),
];

const DIAGNOSTIC_RULES: &[Rule] = &[prefix("", &[Documentation])];

fn classify(collection: &str, field: &str) -> Vec<Axis> {
    // An owner's effective view uses the same field vocabulary as an
    // endpoint record, so it shares the endpoint rules.
    if collection == "owners" {
        if let Some(rest) = field.strip_prefix("effective.") {
            if rest.starts_with("request_default") || rest.starts_with("response_default") {
                // Defaults govern future inheritance; endpoint records carry
                // the selected representation and its wire consequences.
                return vec![Axis::Provenance];
            }
            return classify("endpoints", rest);
        }
        if field == "effective" {
            return vec![Axis::Wire, Axis::Provenance];
        }
    }
    let rules = match collection {
        "client" => CLIENT_RULES,
        "owners" => OWNER_RULES,
        "endpoints" => ENDPOINT_RULES,
        "policies" => POLICY_RULES,
        "window_sets" => WINDOW_SET_RULES,
        "dependencies" => DEPENDENCY_RULES,
        "diagnostics" => DIAGNOSTIC_RULES,
        _ => &[],
    };
    for rule in rules {
        let matched = if rule.exact {
            field == rule.field
        } else {
            rule.field.is_empty()
                || field == rule.field
                || field
                    .strip_prefix(rule.field)
                    .is_some_and(|rest| rest.starts_with('.'))
        };
        if matched {
            return rule.axes.to_vec();
        }
    }
    vec![Axis::Unknown]
}
