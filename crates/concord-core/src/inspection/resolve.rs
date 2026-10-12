//! Validation and effective-view derivation for [`ContractBuilder::build`].
//!
//! The builder receives only local, as-written declaration facts. Every
//! effective view (route, head programs, auth, representation, timeout,
//! policies, coverage and sharing) is derived here with the §5, §6, §9, §11
//! and §12 composition laws, so generated code cannot supply an effective
//! view that disagrees with the local declaration.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::Duration;

use http::Method;

use crate::endpoint::{Effect, EffectSource, EndpointId, ResponseAcquisition, StatusSource};
use crate::limits::{
    MAX_AUTH_PLACEMENTS, MAX_RATE_LIMIT_ID_BYTES, MAX_RATE_LIMIT_KEY_PART_NAME_BYTES,
    MAX_RATE_LIMIT_KEY_PARTS, MAX_RATE_LIMIT_POLICIES_PER_PLAN, MAX_RATE_LIMIT_WINDOWS_PER_POLICY,
    MAX_RUNTIME_ENDPOINT_METHOD_BYTES,
};
use crate::rate_limit::{
    ENDPOINT_LABEL, ORIGIN_LABEL, RateDimensionKind, UTF8_ENCODING_ID, is_valid_encoding_id,
};
use crate::redaction::is_sensitive_name;
use crate::runtime::defaults::DEFAULT_OPERATION_TIMEOUT_SECONDS;

use super::auth::{
    AuthOperation, AuthPlacement, AuthResetMode, ChallengeRule, CredentialDescription,
    CredentialSourceKind, DisplacedAuthPlacement, EffectiveAuth, EffectiveAuthPlacement,
    PlacementTarget,
};
use super::contract::{
    BaseDescription, ContractBuildError, ContractBuildErrorKind as Kind, ContractBuilder,
    ContractDescription, ContractDiagnostic, DependencyKind, DiagnosticSeverity, OpaqueDependency,
    OwnerOrEndpoint,
};
use super::endpoint::{
    AcceptedStatuses, BodyContract, BodySpec, EffectiveKeyOperation, EffectiveRepresentation,
    EffectiveRoute, EffectiveTimeout, EndpointContract, EndpointIdentity, IdSource, InitValue,
    PaginationContract, RepresentationSource, ResultContract, ResultSpec, RouteLabel, RouteReset,
    RouteSegment, StatusRange, TerminalAvailability, TimeoutSource,
};
use super::owner::{
    EffectiveDefault, EffectiveOwnerView, LocalAttachments, OwnerDescription, OwnerSpec,
};
use super::rate::{
    BoundDimension, CostSource, CoveredEndpoint, EffectiveDimension, EffectivePolicy, EncoderRole,
    FormalDimension, LabelBinding, LabelCondition, PolicyAttachmentSite, PolicyCoverage,
    PolicyDefinition, PolicyOrigin, RateAttachment, RateAttachmentSource, SharingCondition,
    SharingGroup, WindowDescription, WindowSetDescription,
};
use super::shape::{
    AtomExpression, Emission, FamilyMode, HeadFamily, KeyOperationKind, LiteralText, OperationSite,
    PathOperation, RepresentationSelection, SubdomainOperation, TemplatePart,
};
use super::values::{
    InitializerExpression, OwnerKind, ReferenceQualifier, Sensitivity, ValueDescription,
    ValueInitializer, ValueLiveness, ValuePresence, ValueRef, ValueRole, ValueSpec, value_role,
};

/// Headers that are never generic public-header targets (HEAD-01).
pub(crate) const FORBIDDEN_PUBLIC_HEADERS: &[&str] = &[
    "host",
    "authorization",
    "proxy-authorization",
    "proxy-authenticate",
    "accept",
    "accept-encoding",
    "content-type",
    "content-encoding",
    "content-length",
    "transfer-encoding",
    "connection",
    "keep-alive",
    "te",
    "trailer",
    "upgrade",
    "proxy-connection",
    "cookie",
    "set-cookie",
];

/// Singleton request headers that may not be repeated (HEAD-01).
pub(crate) const SINGLETON_HEADERS: &[&str] = &[
    "user-agent",
    "referer",
    "from",
    "date",
    "if-modified-since",
    "if-unmodified-since",
    "if-range",
    "range",
    "max-forwards",
    "origin",
    "access-control-request-method",
    "access-control-request-headers",
];

const MAX_STATIC_KEY_BYTES: usize = 256;
const MAX_DNS_LABEL_BYTES: usize = 63;

fn error(kind: Kind, owner: &'static str, subject: &str) -> ContractBuildError {
    ContractBuildError::new(kind, Some(owner), Some(subject))
}

struct Meta {
    kind: OwnerKind,
    path: &'static str,
    name: &'static str,
    parent: Option<usize>,
    children: Vec<usize>,
    docs: &'static [&'static str],
    deprecation: Option<super::values::Deprecation>,
}

struct Extra {
    method: &'static str,
    id: Option<&'static str>,
    result: Option<ResultSpec>,
    body: Option<(BodySpec, usize)>,
    accepted: Option<AcceptedStatuses>,
    absent_on: Option<&'static [u16]>,
    effect: Option<Effect>,
    pagination: Option<PaginationContract>,
    expose_accepted_bytes: bool,
}

struct Tree {
    meta: Vec<Meta>,
    values: Vec<Vec<ValueDescription>>,
    locals: Vec<LocalAttachments>,
    extras: Vec<Option<Extra>>,
    body_positions: Vec<Option<usize>>,
    lookup: HashMap<(&'static str, &'static str), (usize, usize)>,
}

impl Tree {
    fn chain(&self, node: usize) -> Vec<usize> {
        let mut chain = vec![node];
        let mut current = node;
        while let Some(parent) = self.meta[current].parent {
            chain.push(parent);
            current = parent;
        }
        chain.reverse();
        chain
    }

    fn value(&self, reference: &ValueRef) -> Option<&ValueDescription> {
        let path = reference.owner_path()?;
        self.lookup
            .get(&(path, reference.name()))
            .map(|(node, index)| &self.values[*node][*index])
    }
}

pub(crate) fn build(builder: ContractBuilder) -> Result<ContractDescription, ContractBuildError> {
    let ContractBuilder {
        client,
        base,
        owners,
        secrets,
        credentials,
        window_sets,
        policies,
        diagnostics,
    } = builder;

    let mut tree = collect_owners(client, owners)?;
    let base = parse_base(base, tree.meta[0].path)?;
    collect_values(&mut tree)?;
    resolve_local_references(&mut tree)?;

    let secret_names = validate_secrets(&secrets, tree.meta[0].path)?;
    let credentials = validate_credentials(credentials, &secret_names, &tree)?;
    let credential_kinds: HashMap<&'static str, CredentialSourceKind> = credentials
        .iter()
        .map(|credential| (credential.name, credential.kind))
        .collect();
    let window_sets = validate_window_sets(window_sets, tree.meta[0].path)?;
    let definitions = validate_policies(policies, &window_sets, tree.meta[0].path)?;

    validate_local_structure(&tree, &credential_kinds)?;
    let owner_attachments = resolve_owner_attachments(&tree, &definitions, &window_sets)?;
    let taints = owner_taints(&tree)?;

    let mut endpoints = Vec::new();
    let mut seen_ids = BTreeMap::new();
    for node in 0..tree.meta.len() {
        if tree.meta[node].kind != OwnerKind::Endpoint {
            continue;
        }
        let endpoint = resolve_endpoint(
            &tree,
            node,
            &credentials,
            &credential_kinds,
            &owner_attachments,
        )?;
        if seen_ids
            .insert(endpoint.identity.id.as_str().to_owned(), node)
            .is_some()
        {
            return Err(error(
                Kind::DuplicateEndpointId,
                endpoint.path,
                endpoint.identity.id.as_str(),
            ));
        }
        endpoints.push(endpoint);
    }
    if base.ip_literal {
        if let Some(endpoint) = endpoints
            .iter()
            .find(|endpoint| !endpoint.route.labels.is_empty())
        {
            return Err(error(
                Kind::InvalidDnsLabel,
                endpoint.path,
                "ip literal base",
            ));
        }
    }

    detect_credential_cycles(&credentials, &endpoints, tree.meta[0].path)?;
    let policies = policy_coverage(
        &tree,
        &definitions,
        &window_sets,
        &owner_attachments,
        &endpoints,
    )?;
    let window_sets = window_set_usage(window_sets, &endpoints);
    validate_liveness(&tree)?;

    // Redact local literals under keys tainted at their owner.
    for (node, local) in tree.locals.iter_mut().enumerate() {
        for family in [HeadFamily::Query, HeadFamily::Header] {
            let taint = &taints[node][family_index(family)];
            for operation in local.operations_mut(family) {
                if taint.contains(&normalize_key(family, operation.key())) {
                    if let Some(emission) = operation.emission_mut() {
                        redact_emission(emission);
                    }
                }
            }
        }
    }

    let mut views = Vec::with_capacity(tree.meta.len());
    for node in 0..tree.meta.len() {
        views.push(if tree.meta[node].kind == OwnerKind::Endpoint {
            None
        } else {
            Some(owner_view(
                &tree,
                node,
                &credential_kinds,
                &owner_attachments,
            )?)
        });
    }

    let dependencies = collect_dependencies(&tree, &credentials, &definitions, &endpoints);
    let diagnostics = collect_diagnostics(diagnostics, &tree, &policies, &window_sets, &endpoints);

    let root_timeout = match tree.locals[0].timeout {
        Some(duration) => EffectiveTimeout {
            duration,
            source: TimeoutSource::Declared(tree.meta[0].path),
        },
        None => EffectiveTimeout {
            duration: Duration::from_secs(DEFAULT_OPERATION_TIMEOUT_SECONDS),
            source: TimeoutSource::RootDefault,
        },
    };

    let Tree {
        meta,
        values,
        locals,
        ..
    } = tree;
    let owners = meta
        .iter()
        .zip(values)
        .zip(locals)
        .zip(views)
        .map(
            |(((meta_entry, values), local), effective)| OwnerDescription {
                kind: meta_entry.kind,
                path: meta_entry.path,
                name: meta_entry.name,
                parent: meta_entry.parent.map(|parent| meta[parent].path),
                children: meta_entry
                    .children
                    .iter()
                    .map(|child| meta[*child].path)
                    .collect(),
                docs: meta_entry.docs,
                deprecation: meta_entry.deprecation,
                values: values.into_boxed_slice(),
                local,
                effective,
            },
        )
        .collect::<Vec<_>>();

    let mut endpoint_index = endpoints
        .iter()
        .enumerate()
        .map(|(index, endpoint)| (Box::<str>::from(endpoint.identity.id.as_str()), index))
        .collect::<Vec<_>>();
    endpoint_index.sort();

    Ok(ContractDescription {
        base,
        root_timeout,
        owners: owners.into_boxed_slice(),
        endpoints: endpoints.into_boxed_slice(),
        secrets: secrets.into_boxed_slice(),
        credentials: credentials.into_boxed_slice(),
        window_sets: window_sets.into_boxed_slice(),
        policy_definitions: definitions.into_boxed_slice(),
        policies: policies.into_boxed_slice(),
        dependencies: dependencies.into_boxed_slice(),
        diagnostics: diagnostics.into_boxed_slice(),
        endpoint_index: endpoint_index.into_boxed_slice(),
    })
}

// ---------------------------------------------------------------------------
// Owner tree and values
// ---------------------------------------------------------------------------

fn valid_identifier(segment: &str) -> bool {
    let mut characters = segment.chars();
    match characters.next() {
        Some(first) if first == '_' || first.is_alphabetic() => {}
        _ => return false,
    }
    characters.all(|character| character == '_' || character.is_alphanumeric())
}

fn collect_owners(
    client: OwnerSpec,
    owners: Vec<OwnerOrEndpoint>,
) -> Result<Tree, ContractBuildError> {
    if client.kind != OwnerKind::Client || !valid_identifier(client.path) {
        return Err(error(Kind::InvalidOwnerPath, client.path, client.path));
    }
    if let Some(clause) = client.duplicate_clause {
        return Err(error(Kind::DuplicateClause, client.path, clause));
    }
    let client_name = client.path;
    let mut tree = Tree {
        meta: vec![Meta {
            kind: OwnerKind::Client,
            path: client.path,
            name: client.path,
            parent: None,
            children: Vec::new(),
            docs: client.docs,
            deprecation: client.deprecation,
        }],
        values: Vec::new(),
        locals: vec![client.local],
        extras: vec![None],
        body_positions: Vec::new(),
        lookup: HashMap::new(),
    };
    let mut specs = vec![client.values];
    let mut paths = HashMap::from([(client.path, 0_usize)]);

    for item in owners {
        let (spec, extra) = match item {
            OwnerOrEndpoint::Scope(spec) => (*spec, None),
            OwnerOrEndpoint::Endpoint(endpoint) => {
                let endpoint = *endpoint;
                if let Some(clause) = endpoint.duplicate_clause {
                    return Err(error(Kind::DuplicateClause, endpoint.owner.path, clause));
                }
                let extra = Extra {
                    method: endpoint.method,
                    id: endpoint.id,
                    result: endpoint.result,
                    body: endpoint.body,
                    accepted: endpoint.accepted,
                    absent_on: endpoint.absent_on,
                    effect: endpoint.effect,
                    pagination: endpoint.pagination,
                    expose_accepted_bytes: endpoint.expose_accepted_bytes,
                };
                (endpoint.owner, Some(extra))
            }
        };
        let path = spec.path;
        if spec.kind == OwnerKind::Client {
            return Err(error(Kind::InvalidOwnerPath, path, path));
        }
        let segments = path.split("::").collect::<Vec<_>>();
        if segments.len() < 2
            || segments[0] != client_name
            || !segments.iter().all(|segment| valid_identifier(segment))
        {
            return Err(error(Kind::InvalidOwnerPath, path, path));
        }
        if paths.contains_key(path) {
            return Err(error(Kind::DuplicateOwner, path, path));
        }
        let (parent_path, name) = path
            .rsplit_once("::")
            .ok_or_else(|| error(Kind::InvalidOwnerPath, path, path))?;
        let parent = *paths
            .get(parent_path)
            .ok_or_else(|| error(Kind::InvalidParent, path, parent_path))?;
        if tree.meta[parent].kind == OwnerKind::Endpoint {
            return Err(error(Kind::InvalidParent, path, parent_path));
        }
        if spec.kind == OwnerKind::Scope {
            if matches!(name, "client" | "endpoint") || name.starts_with("__concord") {
                return Err(error(Kind::InvalidScopeName, path, name));
            }
            let mut ancestor = Some(parent);
            while let Some(index) = ancestor {
                if tree.meta[index].kind == OwnerKind::Scope && tree.meta[index].name == name {
                    return Err(error(Kind::InvalidScopeName, path, name));
                }
                ancestor = tree.meta[index].parent;
            }
        }
        if let Some(clause) = spec.duplicate_clause {
            return Err(error(Kind::DuplicateClause, path, clause));
        }
        let index = tree.meta.len();
        tree.meta.push(Meta {
            kind: spec.kind,
            path,
            name,
            parent: Some(parent),
            children: Vec::new(),
            docs: spec.docs,
            deprecation: spec.deprecation,
        });
        tree.meta[parent].children.push(index);
        tree.locals.push(spec.local);
        tree.extras.push(extra);
        specs.push(spec.values);
        paths.insert(path, index);
    }
    tree.values = specs
        .into_iter()
        .map(|values| {
            values
                .into_iter()
                .map(|spec| ValueDescription {
                    owner: "",
                    owner_kind: OwnerKind::Client,
                    spec,
                    declaration_index: 0,
                    constructor_position: None,
                })
                .collect()
        })
        .collect();
    tree.body_positions = vec![None; tree.meta.len()];
    Ok(tree)
}

fn collect_values(tree: &mut Tree) -> Result<(), ContractBuildError> {
    for node in 0..tree.meta.len() {
        let owner = tree.meta[node].path;
        let kind = tree.meta[node].kind;
        let body = tree.extras[node]
            .as_ref()
            .and_then(|extra| extra.body.as_ref().map(|(body, index)| (*body, *index)));
        let mut names = BTreeSet::new();
        let count = tree.values[node].len();
        let mut position = 0;
        let mut body_position = None;
        for index in 0..=count {
            if let Some((body, body_index)) = body {
                if body_index == index {
                    if !names.insert(body.name) {
                        return Err(error(Kind::DuplicateValue, owner, body.name));
                    }
                    if body.presence == ValuePresence::Required {
                        body_position = Some(position);
                        position += 1;
                    }
                }
            }
            if index == count {
                break;
            }
            let value = &mut tree.values[node][index];
            let spec: ValueSpec = value.spec;
            if !names.insert(spec.name) {
                return Err(error(Kind::DuplicateValue, owner, spec.name));
            }
            value.owner = owner;
            value.owner_kind = kind;
            value.declaration_index = index;
            let role = value_role(kind, spec.presence, spec.initializer);
            if role == ValueRole::Constructor {
                value.constructor_position = Some(position);
                position += 1;
            }
            if spec.sensitivity == Sensitivity::Sensitive {
                if let ValueInitializer::StableValue(InitializerExpression::Literal(_)) =
                    spec.initializer
                {
                    value.spec.initializer =
                        ValueInitializer::StableValue(InitializerExpression::Redacted);
                }
            }
            if spec.liveness == ValueLiveness::DeprecatedNoOp
                && (spec.deprecation.is_none() || role != ValueRole::Configurable)
            {
                return Err(error(Kind::InvalidNoOpValue, owner, spec.name));
            }
            tree.lookup.insert((owner, spec.name), (node, index));
        }
        tree.body_positions[node] = body_position;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// References
// ---------------------------------------------------------------------------

fn resolve_ref(
    tree: &Tree,
    owner: usize,
    reference: &mut ValueRef,
) -> Result<ValueRef, ContractBuildError> {
    let owner_path = tree.meta[owner].path;
    let name = reference.name();
    let target = match reference.qualifier() {
        ReferenceQualifier::Local => owner,
        ReferenceQualifier::Client => {
            if owner == 0 {
                return Err(error(Kind::RedundantQualification, owner_path, name));
            }
            0
        }
        ReferenceQualifier::Scope(scope) => {
            let mut found = None;
            let mut current = Some(owner);
            while let Some(index) = current {
                if tree.meta[index].kind == OwnerKind::Scope && tree.meta[index].name == scope {
                    found = Some(index);
                    break;
                }
                current = tree.meta[index].parent;
            }
            match found {
                Some(index) if index == owner => {
                    return Err(error(Kind::RedundantQualification, owner_path, name));
                }
                Some(index) => index,
                None => return Err(error(Kind::UnboundReference, owner_path, name)),
            }
        }
        ReferenceQualifier::Owner(path) => {
            let mut found = None;
            let mut current = Some(owner);
            while let Some(index) = current {
                if tree.meta[index].path == path {
                    found = Some(index);
                    break;
                }
                current = tree.meta[index].parent;
            }
            found.ok_or_else(|| error(Kind::UnboundReference, owner_path, name))?
        }
    };
    let target_path = tree.meta[target].path;
    if !tree.lookup.contains_key(&(target_path, name)) {
        return Err(error(Kind::UnboundReference, owner_path, name));
    }
    *reference = ValueRef::exact(target_path, name);
    Ok(*reference)
}

fn resolve_atom(
    tree: &Tree,
    owner: usize,
    atom: &mut AtomExpression,
    total: bool,
) -> Result<(), ContractBuildError> {
    let owner_path = tree.meta[owner].path;
    if let AtomExpression::Concat(parts) = atom {
        if parts.is_empty() {
            return Err(error(Kind::InvalidEmission, owner_path, "empty concat"));
        }
    }
    let flattened = std::mem::replace(atom, AtomExpression::string("")).flatten();
    *atom = flattened;
    let nested_total = total || matches!(atom, AtomExpression::Concat(_));
    let mut failure = None;
    atom.visit_refs_mut(&mut |reference| {
        if failure.is_some() {
            return;
        }
        match resolve_ref(tree, owner, reference) {
            Ok(resolved) => {
                if nested_total
                    && tree
                        .value(&resolved)
                        .is_some_and(|value| value.presence() == ValuePresence::Optional)
                {
                    failure = Some(error(Kind::NonTotalReference, owner_path, resolved.name()));
                }
            }
            Err(err) => failure = Some(err),
        }
    });
    failure.map_or(Ok(()), Err)
}

fn resolve_local_references(tree: &mut Tree) -> Result<(), ContractBuildError> {
    for node in 0..tree.meta.len() {
        let mut local = std::mem::take(&mut tree.locals[node]);
        let result = resolve_local(tree, node, &mut local);
        tree.locals[node] = local;
        result?;
        if let Some(mut extra) = tree.extras[node].take() {
            let result = extra.pagination.as_mut().map_or(Ok(()), |pagination| {
                for reference in pagination.refs_mut() {
                    resolve_ref(tree, node, reference)?;
                }
                Ok(())
            });
            tree.extras[node] = Some(extra);
            result?;
        }
    }
    Ok(())
}

fn resolve_local(
    tree: &Tree,
    node: usize,
    local: &mut LocalAttachments,
) -> Result<(), ContractBuildError> {
    let owner = tree.meta[node].path;
    if let Some(subdomain) = local.subdomain.as_mut() {
        for label in subdomain.labels_mut() {
            resolve_atom(tree, node, label, true)?;
            validate_literal_label(label, owner)?;
        }
    }
    if let Some(path) = local.path.as_mut() {
        if let Some(template) = path.template_mut() {
            if template.segments().is_empty() {
                return Err(error(Kind::InvalidPathSegment, owner, template.text()));
            }
            for segment in template.segments_mut() {
                if segment.parts().is_empty() {
                    return Err(error(Kind::InvalidPathSegment, owner, "empty segment"));
                }
                let mut literal_text = String::new();
                let mut all_literal = true;
                for part in segment.parts_mut() {
                    match part {
                        TemplatePart::Literal(text) => {
                            if text.contains(['/', '\\', '?', '#']) {
                                return Err(error(Kind::InvalidPathSegment, owner, text));
                            }
                            literal_text.push_str(text);
                        }
                        TemplatePart::Hole { value, .. } => {
                            all_literal = false;
                            let resolved = resolve_ref(tree, node, value)?;
                            if tree
                                .value(&resolved)
                                .is_some_and(|value| value.presence() == ValuePresence::Optional)
                            {
                                return Err(error(Kind::NonTotalReference, owner, resolved.name()));
                            }
                        }
                    }
                }
                if all_literal && matches!(literal_text.as_str(), "" | "." | "..") {
                    return Err(error(Kind::InvalidPathSegment, owner, &literal_text));
                }
            }
        }
    }
    for family in [HeadFamily::Query, HeadFamily::Header] {
        for operation in local.operations_mut(family) {
            if let Some(emission) = operation.emission_mut() {
                match emission {
                    Emission::Scalar(atom) => resolve_atom(tree, node, atom, false)?,
                    Emission::Each { value, .. } => {
                        resolve_ref(tree, node, value)?;
                    }
                }
            }
        }
    }
    for attachment in &mut local.rate_limits {
        match &mut attachment.source {
            RateAttachmentSource::Named { bindings, .. } => {
                for binding in bindings.iter_mut() {
                    resolve_ref(tree, node, &mut binding.value)?;
                }
            }
            RateAttachmentSource::Inline { dimensions, .. } => {
                for dimension in dimensions.iter_mut() {
                    if let BoundDimension::Custom { value, .. } = dimension {
                        resolve_ref(tree, node, value)?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn valid_dns_label(label: &str) -> bool {
    let bytes = label.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= MAX_DNS_LABEL_BYTES
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
        && bytes[0] != b'-'
        && bytes[bytes.len() - 1] != b'-'
}

fn literal_only_text(atom: &AtomExpression) -> Option<String> {
    match atom {
        AtomExpression::Literal {
            text: LiteralText::Text(text),
            ..
        } => Some((*text).to_owned()),
        AtomExpression::Concat(parts) => parts.iter().map(literal_only_text).collect(),
        _ => None,
    }
}

fn validate_literal_label(
    atom: &AtomExpression,
    owner: &'static str,
) -> Result<(), ContractBuildError> {
    if let Some(text) = literal_only_text(atom) {
        if !valid_dns_label(&text) {
            return Err(error(Kind::InvalidDnsLabel, owner, &text));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Base
// ---------------------------------------------------------------------------

fn parse_base(
    literal: &'static str,
    owner: &'static str,
) -> Result<BaseDescription, ContractBuildError> {
    let invalid = || error(Kind::InvalidBase, owner, "base");
    let url = url::Url::parse(literal).map_err(|_| invalid())?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid());
    }
    let host = url.host().ok_or_else(invalid)?;
    let ip_literal = !matches!(host, url::Host::Domain(_));
    let host = match host {
        url::Host::Domain(domain) => domain.to_ascii_lowercase(),
        url::Host::Ipv4(address) => address.to_string(),
        url::Host::Ipv6(address) => format!("[{address}]"),
    };
    let port = url.port_or_known_default().ok_or_else(invalid)?;
    let mut path_prefix = url.path().to_owned();
    if !path_prefix.ends_with('/') {
        path_prefix.push('/');
    }
    Ok(BaseDescription {
        literal,
        scheme: url.scheme().into(),
        host: host.into(),
        port,
        path_prefix: path_prefix.into(),
        ip_literal,
    })
}

// ---------------------------------------------------------------------------
// Registry validation
// ---------------------------------------------------------------------------

fn validate_secrets(
    secrets: &[super::auth::SecretDescription],
    client: &'static str,
) -> Result<BTreeSet<&'static str>, ContractBuildError> {
    let mut names = BTreeSet::new();
    for secret in secrets {
        if !names.insert(secret.name()) {
            return Err(error(Kind::DuplicateSecret, client, secret.name()));
        }
    }
    Ok(names)
}

fn validate_credentials(
    credentials: Vec<CredentialDescription>,
    secrets: &BTreeSet<&'static str>,
    tree: &Tree,
) -> Result<Vec<CredentialDescription>, ContractBuildError> {
    let client = tree.meta[0].path;
    let mut names = BTreeSet::new();
    let mut resolved = Vec::with_capacity(credentials.len());
    for mut credential in credentials {
        let name = credential.name;
        if !names.insert(name) {
            return Err(error(Kind::DuplicateCredential, client, name));
        }
        let arity_ok = match credential.kind {
            CredentialSourceKind::ApiKey | CredentialSourceKind::Bearer => {
                credential.secrets.len() == 1
            }
            CredentialSourceKind::Basic => credential.secrets.len() == 2,
            CredentialSourceKind::CustomProvider => {
                credential.secrets.is_empty() && credential.provider_type.is_some()
            }
            CredentialSourceKind::Endpoint => {
                credential.secrets.is_empty() && credential.source_endpoint.is_some()
            }
        };
        if !arity_ok {
            return Err(error(Kind::InvalidCredentialSource, client, name));
        }
        for secret in credential.secrets.iter() {
            if !secrets.contains(secret) {
                return Err(error(Kind::UnknownCredential, client, secret));
            }
        }
        if let Some(relative) = credential.source_endpoint {
            let full = format!("{client}::{relative}");
            let target = tree
                .meta
                .iter()
                .find(|meta| meta.kind == OwnerKind::Endpoint && meta.path == full)
                .ok_or_else(|| error(Kind::InvalidCredentialSource, client, name))?;
            credential.resolved_source_endpoint = Some(target.path);
        }
        resolved.push(credential);
    }
    Ok(resolved)
}

fn validate_windows(
    windows: &[WindowDescription],
    owner: &'static str,
    subject: &str,
) -> Result<(), ContractBuildError> {
    if windows.is_empty() || windows.len() > MAX_RATE_LIMIT_WINDOWS_PER_POLICY {
        return Err(error(Kind::InvalidWindow, owner, subject));
    }
    let mut durations = BTreeSet::new();
    for window in windows {
        if window.capacity() == 0 || window.duration().is_zero() {
            return Err(error(Kind::InvalidWindow, owner, subject));
        }
        if !durations.insert(window.duration()) {
            return Err(error(Kind::InvalidWindow, owner, subject));
        }
    }
    Ok(())
}

fn validate_window_sets(
    sets: Vec<WindowSetDescription>,
    client: &'static str,
) -> Result<Vec<WindowSetDescription>, ContractBuildError> {
    let mut names = BTreeSet::new();
    for set in &sets {
        if !names.insert(set.name) {
            return Err(error(Kind::InvalidWindowSet, client, set.name));
        }
        validate_windows(&set.windows, client, set.name)?;
    }
    Ok(sets)
}

fn validate_policy_id(id: &str, owner: &'static str) -> Result<(), ContractBuildError> {
    if id.is_empty() || id.len() > MAX_RATE_LIMIT_ID_BYTES {
        return Err(error(Kind::InvalidPolicyId, owner, "policy id"));
    }
    Ok(())
}

fn validate_label(label: &str, owner: &'static str) -> Result<(), ContractBuildError> {
    if label.is_empty()
        || label.len() > MAX_RATE_LIMIT_KEY_PART_NAME_BYTES
        || label == ORIGIN_LABEL
        || label == ENDPOINT_LABEL
    {
        return Err(error(Kind::InvalidRateKey, owner, label));
    }
    Ok(())
}

fn window_list<'a>(
    inline: &'a [WindowDescription],
    set: Option<&'static str>,
    sets: &'a [WindowSetDescription],
    owner: &'static str,
    subject: &str,
) -> Result<&'a [WindowDescription], ContractBuildError> {
    match (inline.is_empty(), set) {
        (false, None) => {
            validate_windows(inline, owner, subject)?;
            Ok(inline)
        }
        (true, Some(name)) => sets
            .iter()
            .find(|candidate| candidate.name == name)
            .map(|candidate| &*candidate.windows)
            .ok_or_else(|| error(Kind::InvalidWindowSet, owner, name)),
        _ => Err(error(Kind::InvalidWindow, owner, subject)),
    }
}

fn validate_cost(
    cost: u32,
    windows: &[WindowDescription],
    owner: &'static str,
    subject: &str,
) -> Result<(), ContractBuildError> {
    if cost == 0 || windows.iter().any(|window| cost > window.capacity()) {
        return Err(error(Kind::InvalidCost, owner, subject));
    }
    Ok(())
}

fn validate_policies(
    policies: Vec<PolicyDefinition>,
    sets: &[WindowSetDescription],
    client: &'static str,
) -> Result<Vec<PolicyDefinition>, ContractBuildError> {
    let mut names = BTreeSet::new();
    let mut ids = BTreeSet::new();
    for policy in &policies {
        if !names.insert(policy.name) {
            return Err(error(Kind::InvalidPolicy, client, policy.name));
        }
        validate_policy_id(policy.id, client)?;
        if !ids.insert(policy.id) {
            return Err(error(Kind::InvalidPolicyId, client, policy.name));
        }
        let mut formals = BTreeSet::new();
        for formal in &policy.formals {
            if !formals.insert(*formal) {
                return Err(error(Kind::InvalidRateBinding, client, formal));
            }
        }
        if policy.dimensions.is_empty() || policy.dimensions.len() > MAX_RATE_LIMIT_KEY_PARTS {
            return Err(error(Kind::InvalidRateKey, client, policy.name));
        }
        let mut labels = BTreeSet::new();
        let mut used = BTreeSet::new();
        for dimension in &policy.dimensions {
            let label = match dimension {
                FormalDimension::Origin => ORIGIN_LABEL,
                FormalDimension::Endpoint => ENDPOINT_LABEL,
                FormalDimension::Custom { label, formal, .. } => {
                    validate_label(label, client)?;
                    if !formals.contains(formal) {
                        return Err(error(Kind::InvalidRateBinding, client, formal));
                    }
                    used.insert(*formal);
                    label
                }
            };
            if !labels.insert(label) {
                return Err(error(Kind::InvalidRateKey, client, label));
            }
        }
        if let Some(unused) = formals.difference(&used).next() {
            return Err(error(Kind::UnusedFormal, client, unused));
        }
        let windows = window_list(
            &policy.windows,
            policy.window_set,
            sets,
            client,
            policy.name,
        )?;
        if let Some(cost) = policy.cost {
            validate_cost(cost, windows, client, policy.name)?;
        }
    }
    Ok(policies)
}

// ---------------------------------------------------------------------------
// Local structure validation
// ---------------------------------------------------------------------------

fn normalize_key(family: HeadFamily, key: &str) -> String {
    match family {
        HeadFamily::Query => key.to_owned(),
        HeadFamily::Header => key.to_ascii_lowercase(),
    }
}

const fn family_index(family: HeadFamily) -> usize {
    match family {
        HeadFamily::Query => 0,
        HeadFamily::Header => 1,
    }
}

fn validate_key(
    family: HeadFamily,
    key: &str,
    owner: &'static str,
) -> Result<(), ContractBuildError> {
    if key.is_empty() || key.len() > MAX_STATIC_KEY_BYTES {
        return Err(error(Kind::InvalidKey, owner, key));
    }
    if family == HeadFamily::Header {
        if http::HeaderName::from_bytes(key.as_bytes()).is_err() {
            return Err(error(Kind::InvalidKey, owner, key));
        }
        if FORBIDDEN_PUBLIC_HEADERS.contains(&key.to_ascii_lowercase().as_str()) {
            return Err(error(Kind::ForbiddenHeader, owner, key));
        }
    }
    Ok(())
}

fn validate_local_structure(
    tree: &Tree,
    credentials: &HashMap<&'static str, CredentialSourceKind>,
) -> Result<(), ContractBuildError> {
    for (node, local) in tree.locals.iter().enumerate() {
        let owner = tree.meta[node].path;
        if let Some(timeout) = local.timeout {
            if timeout.is_zero() {
                return Err(error(Kind::InvalidDuration, owner, "timeout"));
            }
        }
        for family in [HeadFamily::Query, HeadFamily::Header] {
            let operations = local.operations(family);
            if local.mode(family) == FamilyMode::Off && !operations.is_empty() {
                return Err(error(Kind::FamilyOffWithOperations, owner, family.label()));
            }
            let mut block_seen = false;
            for operation in operations {
                validate_key(family, operation.key(), owner)?;
                match operation.site() {
                    OperationSite::Block => block_seen = true,
                    OperationSite::DirectPlacement => {
                        if block_seen || operation.kind() != KeyOperationKind::Introduce {
                            return Err(error(Kind::InvalidPlacementOrder, owner, operation.key()));
                        }
                    }
                }
                match (operation.kind(), operation.emission()) {
                    (KeyOperationKind::Remove, None) => {}
                    (KeyOperationKind::Remove, Some(_)) | (_, None) => {
                        return Err(error(Kind::InvalidEmission, owner, operation.key()));
                    }
                    (kind, Some(emission)) => {
                        if kind == KeyOperationKind::WhenPresent {
                            let optional = emission
                                .presence_source()
                                .and_then(|source| tree.value(source))
                                .is_some_and(|value| value.presence() == ValuePresence::Optional);
                            if !optional {
                                return Err(error(
                                    Kind::WhenPresentRequiresOptional,
                                    owner,
                                    operation.key(),
                                ));
                            }
                        }
                        if family == HeadFamily::Header
                            && SINGLETON_HEADERS
                                .contains(&operation.key().to_ascii_lowercase().as_str())
                            && (kind == KeyOperationKind::Append
                                || matches!(emission, Emission::Each { .. }))
                        {
                            return Err(error(
                                Kind::SingletonHeaderRepetition,
                                owner,
                                operation.key(),
                            ));
                        }
                    }
                }
            }
        }
        if let Some(auth) = &local.auth {
            for placement in auth.placements() {
                if !credentials.contains_key(placement.credential()) {
                    return Err(error(
                        Kind::UnknownCredential,
                        owner,
                        placement.credential(),
                    ));
                }
                let valid_target = match placement.target() {
                    PlacementTarget::Header(name) => {
                        name.len() <= MAX_STATIC_KEY_BYTES
                            && http::HeaderName::from_bytes(name.as_bytes()).is_ok()
                    }
                    PlacementTarget::Query(name) => {
                        !name.is_empty() && name.len() <= MAX_STATIC_KEY_BYTES
                    }
                    PlacementTarget::Authorization {
                        scheme: Some(scheme),
                    } => {
                        scheme.len() <= MAX_STATIC_KEY_BYTES
                            && http::HeaderName::from_bytes(scheme.as_bytes()).is_ok()
                    }
                    _ => true,
                };
                if !valid_target {
                    return Err(error(Kind::InvalidKey, owner, placement.credential()));
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Head programs
// ---------------------------------------------------------------------------

fn emission_sensitive(tree: &Tree, emission: &Emission) -> bool {
    let mut sensitive = false;
    emission.visit_refs(&mut |reference| {
        if tree
            .value(reference)
            .is_some_and(|value| value.sensitivity() == Sensitivity::Sensitive)
        {
            sensitive = true;
        }
    });
    sensitive
}

fn redact_emission(emission: &mut Emission) {
    if let Emission::Scalar(atom) = emission {
        atom.redact_literals();
    }
}

struct Composition {
    operations: Vec<EffectiveKeyOperation>,
    taint: BTreeSet<String>,
}

fn compose_family(
    tree: &Tree,
    chain: &[usize],
    family: HeadFamily,
) -> Result<Composition, ContractBuildError> {
    let mut operations: Vec<EffectiveKeyOperation> = Vec::new();
    let mut taint = BTreeSet::new();
    for &node in chain {
        let local = &tree.locals[node];
        let owner = tree.meta[node].path;
        if matches!(local.mode(family), FamilyMode::Replace | FamilyMode::Off) {
            operations.clear();
        }
        for operation in local.operations(family) {
            let key = normalize_key(family, operation.key());
            let prior = operations
                .iter()
                .any(|existing| normalize_key(family, existing.operation.key()) == key);
            let sensitive = is_sensitive_name(operation.key())
                || operation
                    .emission()
                    .is_some_and(|emission| emission_sensitive(tree, emission));
            if sensitive {
                taint.insert(key.clone());
            }
            let conditional_on = operation.emission().and_then(|emission| {
                emission.presence_source().copied().filter(|source| {
                    tree.value(source)
                        .is_some_and(|value| value.presence() == ValuePresence::Optional)
                })
            });
            let effective = EffectiveKeyOperation {
                family,
                operation: operation.clone(),
                origin: owner,
                sensitive: false,
                conditional_on,
                superseded_when_present: Box::new([]),
            };
            match operation.kind() {
                KeyOperationKind::Introduce => {
                    if prior {
                        return Err(error(Kind::IntroduceConflict, owner, operation.key()));
                    }
                    operations.push(effective);
                }
                KeyOperationKind::Replace => {
                    if !prior {
                        return Err(error(Kind::MissingOverrideTarget, owner, operation.key()));
                    }
                    operations
                        .retain(|existing| normalize_key(family, existing.operation.key()) != key);
                    operations.push(effective);
                }
                KeyOperationKind::WhenPresent => {
                    if !prior {
                        return Err(error(Kind::MissingOverrideTarget, owner, operation.key()));
                    }
                    if let Some(source) = conditional_on {
                        for existing in &mut operations {
                            if normalize_key(family, existing.operation.key()) == key {
                                let mut superseded =
                                    std::mem::take(&mut existing.superseded_when_present)
                                        .into_vec();
                                superseded.push(source);
                                existing.superseded_when_present = superseded.into_boxed_slice();
                            }
                        }
                    }
                    operations.push(effective);
                }
                KeyOperationKind::Append => operations.push(effective),
                KeyOperationKind::Remove => {
                    if !prior {
                        return Err(error(Kind::MissingOverrideTarget, owner, operation.key()));
                    }
                    operations
                        .retain(|existing| normalize_key(family, existing.operation.key()) != key);
                }
            }
        }
    }
    for operation in &mut operations {
        let key = normalize_key(family, operation.operation.key());
        operation.sensitive = taint.contains(&key);
        if operation.sensitive {
            if let Some(emission) = operation.operation.emission_mut() {
                redact_emission(emission);
            }
        }
    }
    Ok(Composition { operations, taint })
}

fn owner_taints(tree: &Tree) -> Result<Vec<[BTreeSet<String>; 2]>, ContractBuildError> {
    let mut taints = Vec::with_capacity(tree.meta.len());
    for node in 0..tree.meta.len() {
        let chain = tree.chain(node);
        let query = compose_family(tree, &chain, HeadFamily::Query)?;
        let headers = compose_family(tree, &chain, HeadFamily::Header)?;
        taints.push([query.taint, headers.taint]);
    }
    Ok(taints)
}

// ---------------------------------------------------------------------------
// Rate attachments
// ---------------------------------------------------------------------------

fn builtin_dimension(kind: RateDimensionKind) -> EffectiveDimension {
    EffectiveDimension {
        label: if kind == RateDimensionKind::Origin {
            ORIGIN_LABEL
        } else {
            ENDPOINT_LABEL
        },
        kind,
        binding: None,
        encoder_role: EncoderRole::BuiltinUtf8,
        encoder_type: None,
        encoding_id: Some(UTF8_ENCODING_ID),
    }
}

fn custom_dimension(
    tree: &Tree,
    owner: &'static str,
    label: &'static str,
    value: ValueRef,
    encoder: Option<&'static str>,
    encoding_id: Option<&'static str>,
) -> Result<EffectiveDimension, ContractBuildError> {
    let description = tree
        .value(&value)
        .ok_or_else(|| error(Kind::UnboundReference, owner, value.name()))?;
    if description.presence() != ValuePresence::Required {
        return Err(error(Kind::InvalidRateValue, owner, value.name()));
    }
    if let Some(id) = encoding_id {
        if !is_valid_encoding_id(id) {
            return Err(error(Kind::InvalidEncodingId, owner, label));
        }
    }
    Ok(EffectiveDimension {
        label,
        kind: RateDimensionKind::Custom,
        binding: Some(value),
        encoder_role: if encoder.is_some() {
            EncoderRole::Encoder
        } else {
            EncoderRole::ValueType
        },
        encoder_type: Some(encoder.unwrap_or(description.payload_type())),
        encoding_id,
    })
}

fn resolve_attachment(
    tree: &Tree,
    node: usize,
    attachment: &RateAttachment,
    definitions: &[PolicyDefinition],
    sets: &[WindowSetDescription],
) -> Result<EffectivePolicy, ContractBuildError> {
    let owner = tree.meta[node].path;
    let (id, definition, dimensions, windows, window_set, definition_cost) =
        match &attachment.source {
            RateAttachmentSource::Named { policy, bindings } => {
                let definition = definitions
                    .iter()
                    .find(|candidate| candidate.name == *policy)
                    .ok_or_else(|| error(Kind::InvalidPolicy, owner, policy))?;
                let mut bound = BTreeMap::new();
                for binding in bindings.iter() {
                    if !definition.formals.contains(&binding.formal)
                        || bound.insert(binding.formal, *binding).is_some()
                    {
                        return Err(error(Kind::InvalidRateBinding, owner, binding.formal));
                    }
                }
                if bound.len() != definition.formals.len() {
                    return Err(error(Kind::InvalidRateBinding, owner, policy));
                }
                let mut dimensions = Vec::with_capacity(definition.dimensions.len());
                for dimension in &definition.dimensions {
                    dimensions.push(match dimension {
                        FormalDimension::Origin => builtin_dimension(RateDimensionKind::Origin),
                        FormalDimension::Endpoint => builtin_dimension(RateDimensionKind::Endpoint),
                        FormalDimension::Custom {
                            label,
                            formal,
                            encoder,
                        } => {
                            let binding = bound[formal];
                            custom_dimension(
                                tree,
                                owner,
                                label,
                                binding.value,
                                *encoder,
                                binding.encoding_id,
                            )?
                        }
                    });
                }
                let windows = window_list(
                    &definition.windows,
                    definition.window_set,
                    sets,
                    owner,
                    definition.name,
                )?
                .to_vec();
                (
                    definition.id,
                    PolicyOrigin::Named(definition.name),
                    dimensions,
                    windows,
                    definition.window_set,
                    definition.cost,
                )
            }
            RateAttachmentSource::Inline {
                id,
                dimensions: bound,
                windows,
                window_set,
            } => {
                validate_policy_id(id, owner)?;
                if bound.is_empty() || bound.len() > MAX_RATE_LIMIT_KEY_PARTS {
                    return Err(error(Kind::InvalidRateKey, owner, id));
                }
                let mut labels = BTreeSet::new();
                let mut dimensions = Vec::with_capacity(bound.len());
                for dimension in bound.iter() {
                    let effective = match dimension {
                        BoundDimension::Origin => builtin_dimension(RateDimensionKind::Origin),
                        BoundDimension::Endpoint => builtin_dimension(RateDimensionKind::Endpoint),
                        BoundDimension::Custom {
                            label,
                            value,
                            encoder,
                            encoding_id,
                        } => {
                            validate_label(label, owner)?;
                            custom_dimension(tree, owner, label, *value, *encoder, *encoding_id)?
                        }
                    };
                    if !labels.insert(effective.label) {
                        return Err(error(Kind::InvalidRateKey, owner, effective.label));
                    }
                    dimensions.push(effective);
                }
                let windows = window_list(windows, *window_set, sets, owner, id)?.to_vec();
                (
                    *id,
                    PolicyOrigin::Inline,
                    dimensions,
                    windows,
                    *window_set,
                    None,
                )
            }
        };
    let (cost, cost_source) = match (attachment.cost, definition_cost) {
        (Some(cost), _) => (cost, CostSource::Attachment),
        (None, Some(cost)) => (cost, CostSource::Definition),
        (None, None) => (1, CostSource::Default),
    };
    validate_cost(cost, &windows, owner, id)?;
    Ok(EffectivePolicy {
        id,
        definition,
        origin: owner,
        dimensions: dimensions.into_boxed_slice(),
        cost,
        cost_source,
        windows: windows.into_boxed_slice(),
        window_set,
    })
}

fn resolve_owner_attachments(
    tree: &Tree,
    definitions: &[PolicyDefinition],
    sets: &[WindowSetDescription],
) -> Result<Vec<Vec<EffectivePolicy>>, ContractBuildError> {
    let mut resolved = Vec::with_capacity(tree.meta.len());
    for node in 0..tree.meta.len() {
        let mut policies = Vec::new();
        for attachment in &tree.locals[node].rate_limits {
            policies.push(resolve_attachment(
                tree,
                node,
                attachment,
                definitions,
                sets,
            )?);
        }
        resolved.push(policies);
    }
    Ok(resolved)
}

// ---------------------------------------------------------------------------
// Endpoints
// ---------------------------------------------------------------------------

fn to_snake_case(name: &str) -> String {
    let name = name.strip_prefix("r#").unwrap_or(name);
    let characters = name.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(name.len() + 4);
    for (index, character) in characters.iter().copied().enumerate() {
        if character.is_uppercase() {
            if index > 0 {
                let previous = characters[index - 1];
                let previous_is_word_tail = previous.is_lowercase() || previous.is_ascii_digit();
                let acronym_end = previous.is_uppercase()
                    && characters
                        .get(index + 1)
                        .is_some_and(|next| next.is_lowercase());
                if previous_is_word_tail || acronym_end {
                    output.push('_');
                }
            }
            output.extend(character.to_lowercase());
        } else {
            output.push(character);
        }
    }
    output
}

fn validate_method(
    method: &'static str,
    owner: &'static str,
) -> Result<Method, ContractBuildError> {
    if method.is_empty() || method.len() > MAX_RUNTIME_ENDPOINT_METHOD_BYTES {
        return Err(error(Kind::InvalidMethod, owner, "method"));
    }
    if method.eq_ignore_ascii_case("CONNECT") || method.eq_ignore_ascii_case("TRACE") {
        return Err(error(Kind::InvalidMethod, owner, method));
    }
    Method::from_bytes(method.as_bytes()).map_err(|_| error(Kind::InvalidMethod, owner, "method"))
}

fn canonical_ranges(
    ranges: &[StatusRange],
    owner: &'static str,
) -> Result<Box<[StatusRange]>, ContractBuildError> {
    if ranges.is_empty() {
        return Err(error(Kind::InvalidStatus, owner, "status"));
    }
    let mut sorted = ranges.to_vec();
    sorted.sort();
    for range in &sorted {
        if range.start() > range.end() || range.start() < 200 || range.end() > 599 {
            return Err(error(Kind::InvalidStatus, owner, "status"));
        }
    }
    for pair in sorted.windows(2) {
        if pair[1].start() <= pair[0].end() {
            return Err(error(Kind::InvalidStatus, owner, "status"));
        }
    }
    Ok(sorted.into_boxed_slice())
}

fn resolve_representation(
    tree: &Tree,
    chain: &[usize],
    inline: Option<&'static str>,
    select: impl Fn(&LocalAttachments) -> Option<RepresentationSelection>,
    direction: &str,
) -> Result<EffectiveRepresentation, ContractBuildError> {
    let endpoint = tree.meta[chain[chain.len() - 1]].path;
    let mut inherited = None;
    let mut fence = None;
    for &node in chain[..chain.len() - 1].iter().rev() {
        match select(&tree.locals[node]) {
            Some(RepresentationSelection::Type(type_name)) => {
                inherited = Some((tree.meta[node].path, type_name));
                break;
            }
            Some(RepresentationSelection::ExplicitFence) => {
                fence = Some(tree.meta[node].path);
                break;
            }
            None => {}
        }
    }
    match (inline, inherited) {
        (Some(type_name), _) => Ok(EffectiveRepresentation {
            type_name,
            source: RepresentationSource::Inline,
            blocking_fence: fence,
            shadowed_default: inherited,
        }),
        (None, Some((owner, type_name))) => Ok(EffectiveRepresentation {
            type_name,
            source: RepresentationSource::Inherited(owner),
            blocking_fence: None,
            shadowed_default: None,
        }),
        (None, None) => Err(error(
            Kind::MissingRepresentation,
            fence.unwrap_or(endpoint),
            direction,
        )),
    }
}

fn resolve_route(tree: &Tree, chain: &[usize]) -> EffectiveRoute {
    let mut labels: Vec<RouteLabel> = Vec::new();
    let mut segments: Vec<RouteSegment> = Vec::new();
    let mut subdomain_reset = None;
    let mut path_reset = None;
    for &node in chain {
        let owner = tree.meta[node].path;
        let local = &tree.locals[node];
        match &local.subdomain {
            Some(SubdomainOperation::Prepend(new)) => {
                let mut next = new
                    .iter()
                    .map(|atom| RouteLabel {
                        atom: atom.clone(),
                        origin: owner,
                    })
                    .collect::<Vec<_>>();
                next.append(&mut labels);
                labels = next;
            }
            Some(SubdomainOperation::Replace(new)) => {
                labels = new
                    .iter()
                    .map(|atom| RouteLabel {
                        atom: atom.clone(),
                        origin: owner,
                    })
                    .collect();
                subdomain_reset = Some(RouteReset { owner, off: false });
            }
            Some(SubdomainOperation::Off) => {
                labels.clear();
                subdomain_reset = Some(RouteReset { owner, off: true });
            }
            None => {}
        }
        match &local.path {
            Some(PathOperation::Append(template)) => {
                segments.extend(template.segments().iter().map(|segment| RouteSegment {
                    segment: segment.clone(),
                    origin: owner,
                }))
            }
            Some(PathOperation::Replace(template)) => {
                segments = template
                    .segments()
                    .iter()
                    .map(|segment| RouteSegment {
                        segment: segment.clone(),
                        origin: owner,
                    })
                    .collect();
                path_reset = Some(RouteReset { owner, off: false });
            }
            Some(PathOperation::Off) => {
                segments.clear();
                path_reset = Some(RouteReset { owner, off: true });
            }
            None => {}
        }
    }
    EffectiveRoute {
        labels: labels.into_boxed_slice(),
        segments: segments.into_boxed_slice(),
        subdomain_reset,
        path_reset,
    }
}

fn resolve_auth(
    tree: &Tree,
    chain: &[usize],
    credential_kinds: &HashMap<&'static str, CredentialSourceKind>,
    effect: Effect,
) -> Result<EffectiveAuth, ContractBuildError> {
    let endpoint = tree.meta[chain[chain.len() - 1]].path;
    let mut placements: Vec<(AuthPlacement, &'static str)> = Vec::new();
    let mut displaced = Vec::new();
    for &node in chain {
        let owner = tree.meta[node].path;
        let (mode, new) = match &tree.locals[node].auth {
            Some(AuthOperation::Append(new)) => (None, &**new),
            Some(AuthOperation::Replace(new)) => (Some(AuthResetMode::Replace), &**new),
            Some(AuthOperation::Off) => (Some(AuthResetMode::Off), &[][..]),
            None => continue,
        };
        if let Some(mode) = mode {
            displaced.extend(placements.drain(..).map(|(placement, origin)| {
                DisplacedAuthPlacement {
                    placement,
                    origin,
                    displaced_by: owner,
                    mode,
                }
            }));
        }
        placements.extend(new.iter().map(|placement| (*placement, owner)));
    }
    if placements.len() > MAX_AUTH_PLACEMENTS {
        return Err(error(Kind::AuthPlacementConflict, endpoint, "placements"));
    }
    let mut targets = BTreeSet::new();
    let mut challenges = 0;
    let mut effective = Vec::with_capacity(placements.len());
    for (placement, origin) in placements {
        let (family, field) = placement.target().owned_field();
        if !targets.insert((family_index(family), field)) {
            return Err(error(
                Kind::AuthPlacementConflict,
                endpoint,
                placement.credential(),
            ));
        }
        if placement.challenge_rule() != ChallengeRule::Never {
            challenges += 1;
        }
        let source_kind = *credential_kinds
            .get(placement.credential())
            .ok_or_else(|| error(Kind::UnknownCredential, endpoint, placement.credential()))?;
        effective.push(EffectiveAuthPlacement {
            placement,
            origin,
            source_kind,
            automatic_recovery: placement.challenge_rule() != ChallengeRule::Never
                && source_kind.may_replay_after_challenge(effect),
        });
    }
    if challenges > 1 {
        return Err(error(Kind::AuthPlacementConflict, endpoint, "challenge"));
    }
    Ok(EffectiveAuth {
        placements: effective.into_boxed_slice(),
        displaced: displaced.into_boxed_slice(),
    })
}

fn resolve_endpoint(
    tree: &Tree,
    node: usize,
    credentials: &[CredentialDescription],
    credential_kinds: &HashMap<&'static str, CredentialSourceKind>,
    owner_attachments: &[Vec<EffectivePolicy>],
) -> Result<EndpointContract, ContractBuildError> {
    let meta = &tree.meta[node];
    let path = meta.path;
    let extra = tree.extras[node]
        .as_ref()
        .ok_or_else(|| error(Kind::InvalidOwnerPath, path, path))?;
    let chain = tree.chain(node);

    let identity = match extra.id {
        Some(id) => EndpointIdentity {
            id: EndpointId::new(id).map_err(|_| error(Kind::InvalidEndpointId, path, "id"))?,
            source: IdSource::Explicit,
            derivation_inputs: Box::new([]),
        },
        None => {
            let inputs = std::iter::once(to_snake_case(tree.meta[0].name))
                .chain(
                    chain[1..]
                        .iter()
                        .map(|index| tree.meta[*index].name.to_owned()),
                )
                .map(String::into_boxed_str)
                .collect::<Vec<_>>();
            let derived = inputs
                .iter()
                .map(|input| &**input)
                .collect::<Vec<_>>()
                .join(".");
            EndpointIdentity {
                id: EndpointId::new(&derived)
                    .map_err(|_| error(Kind::InvalidDerivedEndpointId, path, path))?,
                source: IdSource::Derived,
                derivation_inputs: inputs.into_boxed_slice(),
            }
        }
    };

    let method = validate_method(extra.method, path)?;
    let (effect, effect_source) = match extra.effect {
        Some(effect) => (effect, EffectSource::Explicit),
        None => (Effect::for_method(&method), EffectSource::MethodDefault),
    };
    let (accepted, status_source) = match &extra.accepted {
        None => (AcceptedStatuses::Success, StatusSource::DefaultSuccess),
        Some(AcceptedStatuses::Success) => (AcceptedStatuses::Success, StatusSource::Explicit),
        Some(AcceptedStatuses::Explicit(ranges)) => (
            AcceptedStatuses::Explicit(canonical_ranges(ranges, path)?),
            StatusSource::Explicit,
        ),
    };
    let absent_on = match extra.absent_on {
        None => None,
        Some(codes) => {
            let mut sorted = codes.to_vec();
            sorted.sort_unstable();
            if sorted.is_empty()
                || sorted.windows(2).any(|pair| pair[0] == pair[1])
                || sorted.iter().any(|code| !(200..=599).contains(code))
                || sorted.iter().any(|code| accepted.contains(*code))
            {
                return Err(error(Kind::InvalidStatus, path, "absent"));
            }
            Some(sorted.into_boxed_slice())
        }
    };

    let timeout = chain
        .iter()
        .rev()
        .find_map(|index| {
            tree.locals[*index]
                .timeout
                .map(|duration| EffectiveTimeout {
                    duration,
                    source: TimeoutSource::Declared(tree.meta[*index].path),
                })
        })
        .unwrap_or(EffectiveTimeout {
            duration: Duration::from_secs(DEFAULT_OPERATION_TIMEOUT_SECONDS),
            source: TimeoutSource::RootDefault,
        });

    let result_spec = extra
        .result
        .ok_or_else(|| error(Kind::MissingResult, path, "result"))?;
    let result = ResultContract {
        output_type: result_spec.output_type,
        representation: resolve_representation(
            tree,
            &chain,
            result_spec.inline_representation,
            |local| local.response_default,
            "response",
        )?,
        driver: result_spec.driver,
        accept: result_spec.accept,
    };
    let body = match &extra.body {
        None => None,
        Some((spec, declaration_index)) => {
            let declaration_index = *declaration_index;
            let constructor_position = tree.body_positions[node];
            Some(BodyContract {
                spec: *spec,
                representation: resolve_representation(
                    tree,
                    &chain,
                    spec.inline_representation,
                    |local| local.request_default,
                    "request",
                )?,
                constructor_position,
                declaration_index,
            })
        }
    };

    let route = resolve_route(tree, &chain);
    let query = compose_family(tree, &chain, HeadFamily::Query)?.operations;
    let headers = compose_family(tree, &chain, HeadFamily::Header)?.operations;
    let auth = resolve_auth(tree, &chain, credential_kinds, effect)?;

    for placement in auth.placements() {
        let (family, field) = placement.placement.target().owned_field();
        let public = match family {
            HeadFamily::Query => &query,
            HeadFamily::Header => &headers,
        };
        if public
            .iter()
            .any(|operation| normalize_key(family, operation.operation.key()) == field)
        {
            return Err(error(Kind::ForbiddenHeader, path, &field));
        }
        for status in placement.placement.challenge_rule().statuses() {
            if accepted.contains(*status)
                || absent_on
                    .as_deref()
                    .is_some_and(|codes| codes.contains(status))
            {
                return Err(error(Kind::ChallengeStatusOverlap, path, "challenge"));
            }
        }
    }

    let cursor_destinations = extra
        .pagination
        .as_ref()
        .map(|pagination| pagination.cursor_destinations().to_vec())
        .unwrap_or_default();

    let policies = effective_policies(tree, &chain, owner_attachments)?;
    for policy in &policies {
        if policy.uses_endpoint() && identity.source != IdSource::Explicit {
            return Err(error(
                Kind::EndpointDimensionRequiresExplicitId,
                path,
                policy.id,
            ));
        }
        for dimension in policy.dimensions.iter() {
            if let Some(binding) = dimension.binding {
                if cursor_destinations.contains(&binding) {
                    return Err(error(Kind::InvalidRateValue, path, binding.name()));
                }
            }
        }
    }

    if let Some(pagination) = &extra.pagination {
        let invalid = |subject: &str| error(Kind::InvalidPagination, path, subject);
        if effect != Effect::Safe {
            return Err(invalid("effect"));
        }
        if result.driver != ResponseAcquisition::Buffered {
            return Err(invalid("driver"));
        }
        if body.is_some_and(|body| !body.page_capable()) {
            return Err(invalid("body"));
        }
        if cursor_destinations.is_empty() {
            return Err(invalid("cursor"));
        }
        let mut destinations = BTreeSet::new();
        for destination in &cursor_destinations {
            if destination.owner_path() != Some(path) || !destinations.insert(*destination) {
                return Err(invalid(destination.name()));
            }
            let each_source = query.iter().chain(headers.iter()).any(|operation| {
                matches!(operation.operation.emission(), Some(Emission::Each { value, .. }) if value == destination)
            });
            if each_source {
                return Err(invalid(destination.name()));
            }
        }
        let mut fields = BTreeSet::new();
        for field in pagination.init() {
            if !fields.insert(field.field()) {
                return Err(invalid(field.field()));
            }
        }
    }

    let store_as = credentials
        .iter()
        .filter(|credential| credential.resolved_source_endpoint == Some(path))
        .map(|credential| credential.name)
        .collect::<Vec<_>>();
    if !store_as.is_empty()
        && (result.driver != ResponseAcquisition::Buffered
            || absent_on.is_some()
            || extra.pagination.is_some())
    {
        return Err(error(Kind::InvalidCredentialSource, path, store_as[0]));
    }
    if absent_on.is_some()
        && (result.driver != ResponseAcquisition::Buffered
            || extra.pagination.is_some()
            || extra.expose_accepted_bytes)
    {
        return Err(error(Kind::InvalidAbsence, path, "absent"));
    }
    if extra.expose_accepted_bytes && result.driver == ResponseAcquisition::Streaming {
        return Err(error(Kind::InvalidAcceptedBytes, path, "accepted_bytes"));
    }

    Ok(EndpointContract {
        path,
        name: meta.name,
        owner: meta
            .parent
            .map_or(tree.meta[0].path, |parent| tree.meta[parent].path),
        identity,
        method: extra.method,
        effect,
        effect_source,
        accepted,
        status_source,
        absent_on,
        timeout,
        result,
        body,
        route,
        query: query.into_boxed_slice(),
        headers: headers.into_boxed_slice(),
        auth,
        policies: policies.into_boxed_slice(),
        pagination: extra.pagination.clone(),
        terminals: TerminalAvailability {
            accepted_bytes: extra.expose_accepted_bytes,
            pagination: extra.pagination.is_some(),
            store_as: store_as.into_boxed_slice(),
        },
    })
}

fn effective_default(
    tree: &Tree,
    chain: &[usize],
    select: impl Fn(&LocalAttachments) -> Option<RepresentationSelection>,
) -> EffectiveDefault {
    for &node in chain.iter().rev() {
        match select(&tree.locals[node]) {
            Some(RepresentationSelection::Type(type_name)) => {
                return EffectiveDefault {
                    type_name: Some(type_name),
                    owner: Some(tree.meta[node].path),
                    fenced: false,
                };
            }
            Some(RepresentationSelection::ExplicitFence) => {
                return EffectiveDefault {
                    type_name: None,
                    owner: Some(tree.meta[node].path),
                    fenced: true,
                };
            }
            None => {}
        }
    }
    EffectiveDefault::default()
}

fn effective_policies(
    tree: &Tree,
    chain: &[usize],
    owner_attachments: &[Vec<EffectivePolicy>],
) -> Result<Vec<EffectivePolicy>, ContractBuildError> {
    let owner = tree.meta[chain[chain.len() - 1]].path;
    let mut policies: Vec<EffectivePolicy> = Vec::new();
    for index in chain {
        for policy in &owner_attachments[*index] {
            if let Some(existing) = policies.iter().find(|existing| existing.id == policy.id) {
                if existing.dimensions == policy.dimensions
                    && existing.windows == policy.windows
                    && existing.cost == policy.cost
                    && existing.definition == policy.definition
                {
                    continue;
                }
                return Err(error(Kind::ConflictingPolicy, owner, policy.id));
            }
            policies.push(policy.clone());
        }
    }
    if policies.len() > MAX_RATE_LIMIT_POLICIES_PER_PLAN {
        return Err(error(Kind::ConflictingPolicy, owner, "policies"));
    }
    Ok(policies)
}

fn owner_view(
    tree: &Tree,
    node: usize,
    credential_kinds: &HashMap<&'static str, CredentialSourceKind>,
    owner_attachments: &[Vec<EffectivePolicy>],
) -> Result<EffectiveOwnerView, ContractBuildError> {
    let chain = tree.chain(node);
    let timeout = chain
        .iter()
        .rev()
        .find_map(|index| {
            tree.locals[*index]
                .timeout
                .map(|duration| EffectiveTimeout {
                    duration,
                    source: TimeoutSource::Declared(tree.meta[*index].path),
                })
        })
        .unwrap_or(EffectiveTimeout {
            duration: Duration::from_secs(DEFAULT_OPERATION_TIMEOUT_SECONDS),
            source: TimeoutSource::RootDefault,
        });
    Ok(EffectiveOwnerView {
        route: resolve_route(tree, &chain),
        query: compose_family(tree, &chain, HeadFamily::Query)?
            .operations
            .into_boxed_slice(),
        headers: compose_family(tree, &chain, HeadFamily::Header)?
            .operations
            .into_boxed_slice(),
        auth: resolve_auth(tree, &chain, credential_kinds, Effect::Safe)?,
        policies: effective_policies(tree, &chain, owner_attachments)?.into_boxed_slice(),
        timeout,
        request_default: effective_default(tree, &chain, |local| local.request_default),
        response_default: effective_default(tree, &chain, |local| local.response_default),
    })
}

fn detect_credential_cycles(
    credentials: &[CredentialDescription],
    endpoints: &[EndpointContract],
    client: &'static str,
) -> Result<(), ContractBuildError> {
    let edges: HashMap<&'static str, Vec<&'static str>> = credentials
        .iter()
        .map(|credential| {
            let used = credential
                .resolved_source_endpoint
                .and_then(|path| endpoints.iter().find(|endpoint| endpoint.path == path))
                .map(|endpoint| {
                    endpoint
                        .auth
                        .placements()
                        .iter()
                        .map(|placement| placement.placement.credential())
                        .collect()
                })
                .unwrap_or_default();
            (credential.name, used)
        })
        .collect();
    fn visit(
        name: &'static str,
        edges: &HashMap<&'static str, Vec<&'static str>>,
        visiting: &mut BTreeSet<&'static str>,
        done: &mut BTreeSet<&'static str>,
    ) -> Option<&'static str> {
        if done.contains(name) {
            return None;
        }
        if !visiting.insert(name) {
            return Some(name);
        }
        for next in edges.get(name).map(Vec::as_slice).unwrap_or_default() {
            if let Some(cycle) = visit(next, edges, visiting, done) {
                return Some(cycle);
            }
        }
        visiting.remove(name);
        done.insert(name);
        None
    }
    let mut done = BTreeSet::new();
    for credential in credentials {
        let mut visiting = BTreeSet::new();
        if let Some(cycle) = visit(credential.name, &edges, &mut visiting, &mut done) {
            return Err(error(Kind::CredentialCycle, client, cycle));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Coverage and sharing
// ---------------------------------------------------------------------------

/// Whether two effective policies of one ID have compatible schemas: equal
/// ordered labels and kinds, equal canonical windows, and equal canonical
/// encoding IDs wherever both are statically known. Rust type spellings may
/// differ (aliases), so they are exported per endpoint but not compared here.
fn compatible_schema(left: &EffectivePolicy, right: &EffectivePolicy) -> bool {
    left.dimensions.len() == right.dimensions.len()
        && left
            .dimensions
            .iter()
            .zip(right.dimensions.iter())
            .all(|(left, right)| {
                left.label == right.label
                    && left.kind == right.kind
                    && match (left.encoding_id, right.encoding_id) {
                        (Some(left), Some(right)) => left == right,
                        _ => true,
                    }
            })
        && left.canonical_windows() == right.canonical_windows()
}

fn policy_coverage(
    tree: &Tree,
    definitions: &[PolicyDefinition],
    sets: &[WindowSetDescription],
    owner_attachments: &[Vec<EffectivePolicy>],
    endpoints: &[EndpointContract],
) -> Result<Vec<PolicyCoverage>, ContractBuildError> {
    let mut by_id: BTreeMap<&'static str, PolicyCoverage> = BTreeMap::new();
    let mut schemas: BTreeMap<&'static str, &EffectivePolicy> = BTreeMap::new();
    for (node, policies) in owner_attachments.iter().enumerate() {
        for (index, policy) in policies.iter().enumerate() {
            if let Some(existing) = schemas.get(policy.id) {
                if !compatible_schema(existing, policy) {
                    return Err(error(
                        Kind::InconsistentPolicySchema,
                        tree.meta[node].path,
                        policy.id,
                    ));
                }
            } else {
                schemas.insert(policy.id, policy);
            }
            let entry = by_id.entry(policy.id).or_insert_with(|| PolicyCoverage {
                id: policy.id,
                definition: policy.definition,
                dimensions: policy
                    .dimensions
                    .iter()
                    .map(|dimension| EffectiveDimension {
                        binding: None,
                        ..*dimension
                    })
                    .collect(),
                windows: policy.windows.clone(),
                window_set: policy.window_set,
                attachments: Box::new([]),
                covered: Box::new([]),
                sharing: Box::new([]),
            });
            if entry.definition != policy.definition {
                return Err(error(
                    Kind::InconsistentPolicySchema,
                    tree.meta[node].path,
                    policy.id,
                ));
            }
            let mut attachments = std::mem::take(&mut entry.attachments).into_vec();
            attachments.push(PolicyAttachmentSite {
                owner: tree.meta[node].path,
                attachment: tree.locals[node].rate_limits[index].clone(),
            });
            entry.attachments = attachments.into_boxed_slice();
        }
    }
    for definition in definitions {
        by_id
            .entry(definition.id)
            .or_insert_with(|| PolicyCoverage {
                id: definition.id,
                definition: PolicyOrigin::Named(definition.name),
                dimensions: definition
                    .dimensions
                    .iter()
                    .map(|dimension| match dimension {
                        FormalDimension::Origin => builtin_dimension(RateDimensionKind::Origin),
                        FormalDimension::Endpoint => builtin_dimension(RateDimensionKind::Endpoint),
                        FormalDimension::Custom { label, encoder, .. } => EffectiveDimension {
                            label,
                            kind: RateDimensionKind::Custom,
                            binding: None,
                            encoder_role: if encoder.is_some() {
                                EncoderRole::Encoder
                            } else {
                                EncoderRole::ValueType
                            },
                            encoder_type: *encoder,
                            encoding_id: None,
                        },
                    })
                    .collect(),
                windows: definition
                    .window_set
                    .and_then(|name| sets.iter().find(|set| set.name == name))
                    .map_or_else(
                        || definition.windows.clone().into_boxed_slice(),
                        |set| set.windows.clone(),
                    ),
                window_set: definition.window_set,
                attachments: Box::new([]),
                covered: Box::new([]),
                sharing: Box::new([]),
            });
    }
    for coverage in by_id.values_mut() {
        let mut covered = Vec::new();
        let mut members: Vec<(&EndpointContract, &EffectivePolicy)> = Vec::new();
        for endpoint in endpoints {
            if let Some(policy) = endpoint
                .policies
                .iter()
                .find(|policy| policy.id == coverage.id)
            {
                covered.push(CoveredEndpoint {
                    id: endpoint.identity.id.clone(),
                    path: endpoint.path,
                    cost: policy.cost,
                    origin: policy.origin,
                });
                members.push((endpoint, policy));
            }
        }
        let sharing = if members.is_empty() {
            Vec::new()
        } else if members[0].1.uses_endpoint() {
            members
                .iter()
                .map(|(endpoint, policy)| SharingGroup {
                    endpoints: Box::new([endpoint.path]),
                    condition: sharing_condition(
                        policy,
                        std::slice::from_ref(&(*endpoint, *policy)),
                    ),
                })
                .collect()
        } else {
            vec![SharingGroup {
                endpoints: members.iter().map(|(endpoint, _)| endpoint.path).collect(),
                condition: sharing_condition(members[0].1, &members),
            }]
        };
        coverage.covered = covered.into_boxed_slice();
        coverage.sharing = sharing.into_boxed_slice();
    }
    Ok(by_id.into_values().collect())
}

fn sharing_condition(
    policy: &EffectivePolicy,
    members: &[(&EndpointContract, &EffectivePolicy)],
) -> SharingCondition {
    let labels = policy
        .dimensions
        .iter()
        .filter(|dimension| dimension.kind == RateDimensionKind::Custom)
        .map(|dimension| LabelCondition {
            label: dimension.label,
            bindings: members
                .iter()
                .filter_map(|(endpoint, member)| {
                    member
                        .dimensions
                        .iter()
                        .find(|candidate| candidate.label == dimension.label)
                        .and_then(|candidate| candidate.binding)
                        .map(|value| LabelBinding {
                            endpoint: endpoint.path,
                            value,
                        })
                })
                .collect(),
        })
        .collect();
    SharingCondition {
        same_origin: policy.uses_origin(),
        same_endpoint: policy.uses_endpoint(),
        labels,
    }
}

fn window_set_usage(
    sets: Vec<WindowSetDescription>,
    endpoints: &[EndpointContract],
) -> Vec<WindowSetDescription> {
    sets.into_iter()
        .map(|mut set| {
            let used = endpoints
                .iter()
                .flat_map(|endpoint| endpoint.policies.iter())
                .filter(|policy| policy.window_set == Some(set.name))
                .map(|policy| policy.id)
                .collect::<BTreeSet<_>>();
            set.used_by = used.into_iter().collect();
            set
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Liveness, dependencies and diagnostics
// ---------------------------------------------------------------------------

fn collect_uses(tree: &Tree) -> BTreeSet<ValueRef> {
    let mut uses = BTreeSet::new();
    let mut record = |reference: &ValueRef| {
        uses.insert(*reference);
    };
    for (node, local) in tree.locals.iter().enumerate() {
        if let Some(subdomain) = &local.subdomain {
            for label in subdomain.labels() {
                label.visit_refs(&mut record);
            }
        }
        if let Some(template) = local.path.as_ref().and_then(PathOperation::template) {
            for segment in template.segments() {
                for part in segment.parts() {
                    if let TemplatePart::Hole { value, .. } = part {
                        record(value);
                    }
                }
            }
        }
        for family in [HeadFamily::Query, HeadFamily::Header] {
            for operation in local.operations(family) {
                if let Some(emission) = operation.emission() {
                    emission.visit_refs(&mut record);
                }
            }
        }
        for attachment in &local.rate_limits {
            match &attachment.source {
                RateAttachmentSource::Named { bindings, .. } => {
                    for binding in bindings.iter() {
                        record(&binding.value);
                    }
                }
                RateAttachmentSource::Inline { dimensions, .. } => {
                    for dimension in dimensions.iter() {
                        if let BoundDimension::Custom { value, .. } = dimension {
                            record(value);
                        }
                    }
                }
            }
        }
        if let Some(pagination) = tree.extras[node]
            .as_ref()
            .and_then(|extra| extra.pagination.as_ref())
        {
            for field in pagination.init() {
                if let InitValue::Value(value) = field.value() {
                    record(&value);
                }
            }
            for destination in pagination.cursor_destinations() {
                record(destination);
            }
        }
    }
    uses
}

fn validate_liveness(tree: &Tree) -> Result<(), ContractBuildError> {
    let uses = collect_uses(tree);
    for values in &tree.values {
        for value in values {
            let used = uses.contains(&value.reference());
            match value.liveness() {
                ValueLiveness::Live if !used => {
                    return Err(error(Kind::DeadValue, value.owner, value.name()));
                }
                ValueLiveness::DeprecatedNoOp if used => {
                    return Err(error(Kind::DeadValue, value.owner, value.name()));
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn collect_dependencies(
    tree: &Tree,
    credentials: &[CredentialDescription],
    definitions: &[PolicyDefinition],
    endpoints: &[EndpointContract],
) -> Vec<OpaqueDependency> {
    let mut map: BTreeMap<(DependencyKind, String), BTreeSet<String>> = BTreeMap::new();
    let mut add = |kind: DependencyKind, reference: &str, site: String| {
        map.entry((kind, reference.to_owned()))
            .or_default()
            .insert(site);
    };
    for (node, values) in tree.values.iter().enumerate() {
        let owner = tree.meta[node].path;
        for value in values {
            let site = format!("{owner}#values.{}", value.name());
            add(
                DependencyKind::PayloadType,
                value.payload_type(),
                site.clone(),
            );
            match value.initializer() {
                ValueInitializer::StableValue(InitializerExpression::Constant(path)) => {
                    add(
                        DependencyKind::Constant,
                        path,
                        format!("{site}.initializer"),
                    );
                }
                ValueInitializer::StableValue(InitializerExpression::Redacted) => {
                    let location = format!("{site}.initializer");
                    add(DependencyKind::RedactedLiteral, &location, location.clone());
                }
                _ => {}
            }
        }
    }
    for (node, local) in tree.locals.iter().enumerate() {
        let owner = tree.meta[node].path;
        if let Some(RepresentationSelection::Type(type_name)) = local.request_default {
            add(
                DependencyKind::Representation,
                type_name,
                format!("{owner}#request_default"),
            );
        }
        if let Some(RepresentationSelection::Type(type_name)) = local.response_default {
            add(
                DependencyKind::Representation,
                type_name,
                format!("{owner}#response_default"),
            );
        }
        if let Some(subdomain) = &local.subdomain {
            for (index, label) in subdomain.labels().iter().enumerate() {
                label.visit_adapters(&mut |adapter| {
                    add(
                        DependencyKind::AtomAdapter,
                        adapter,
                        format!("{owner}#subdomain[{index}]"),
                    );
                });
            }
        }
        if let Some(template) = local.path.as_ref().and_then(PathOperation::template) {
            for (index, segment) in template.segments().iter().enumerate() {
                for part in segment.parts() {
                    if let TemplatePart::Hole {
                        adapter: Some(adapter),
                        ..
                    } = part
                    {
                        add(
                            DependencyKind::AtomAdapter,
                            adapter,
                            format!("{owner}#path[{index}]"),
                        );
                    }
                }
            }
        }
        for family in [HeadFamily::Query, HeadFamily::Header] {
            let label = match family {
                HeadFamily::Query => "query",
                HeadFamily::Header => "headers",
            };
            for (index, operation) in local.operations(family).iter().enumerate() {
                let site = format!("{owner}#{label}[{index}]");
                if let Some(emission) = operation.emission() {
                    emission.visit_adapters(&mut |adapter| {
                        add(DependencyKind::AtomAdapter, adapter, site.clone());
                    });
                    if let Emission::Scalar(atom) = emission {
                        if contains_redacted(atom) {
                            let location = format!("{site}.emission");
                            add(DependencyKind::RedactedLiteral, &location, location.clone());
                        }
                    }
                }
            }
        }
        for (index, attachment) in local.rate_limits.iter().enumerate() {
            if let RateAttachmentSource::Inline { dimensions, .. } = &attachment.source {
                for dimension in dimensions.iter() {
                    if let BoundDimension::Custom {
                        encoder: Some(encoder),
                        ..
                    } = dimension
                    {
                        add(
                            DependencyKind::RateKeyEncoder,
                            encoder,
                            format!("{owner}#rate_limits[{index}]"),
                        );
                    }
                }
            }
        }
    }
    let client = tree.meta[0].path;
    for definition in definitions {
        for dimension in &definition.dimensions {
            if let FormalDimension::Custom {
                encoder: Some(encoder),
                ..
            } = dimension
            {
                add(
                    DependencyKind::RateKeyEncoder,
                    encoder,
                    format!("{client}#policies.{}", definition.name),
                );
            }
        }
    }
    for credential in credentials {
        if let Some(provider) = credential.provider_type {
            add(
                DependencyKind::CredentialProvider,
                provider,
                format!("{client}#credentials.{}", credential.name),
            );
        }
    }
    for endpoint in endpoints {
        let path = endpoint.path;
        add(
            DependencyKind::OutputType,
            endpoint.result.output_type,
            format!("{path}#result"),
        );
        if endpoint.result.representation.source == RepresentationSource::Inline {
            add(
                DependencyKind::Representation,
                endpoint.result.representation.type_name,
                format!("{path}#result.via"),
            );
        }
        if let Some(body) = &endpoint.body {
            add(
                DependencyKind::BodyInputType,
                body.input_type(),
                format!("{path}#body.{}", body.name()),
            );
            if body.representation.source == RepresentationSource::Inline {
                add(
                    DependencyKind::Representation,
                    body.representation.type_name,
                    format!("{path}#body.via"),
                );
            }
        }
        if let Some(pagination) = &endpoint.pagination {
            add(
                DependencyKind::PaginationController,
                pagination.controller(),
                format!("{path}#pagination"),
            );
            add(
                DependencyKind::PaginationInit,
                pagination.init_type(),
                format!("{path}#pagination"),
            );
            for field in pagination.init() {
                if let InitValue::Constant(constant) = field.value() {
                    add(
                        DependencyKind::Constant,
                        constant,
                        format!("{path}#pagination.init.{}", field.field()),
                    );
                }
            }
        }
    }
    map.into_iter()
        .map(|((kind, reference), sites)| OpaqueDependency {
            kind,
            reference: reference.into_boxed_str(),
            sites: sites.into_iter().map(String::into_boxed_str).collect(),
        })
        .collect()
}

fn contains_redacted(atom: &AtomExpression) -> bool {
    match atom {
        AtomExpression::Literal { text, .. } => *text == LiteralText::Redacted,
        AtomExpression::Value { .. } => false,
        AtomExpression::Concat(parts) => parts.iter().any(contains_redacted),
    }
}

fn collect_diagnostics(
    mut diagnostics: Vec<ContractDiagnostic>,
    tree: &Tree,
    policies: &[PolicyCoverage],
    window_sets: &[WindowSetDescription],
    endpoints: &[EndpointContract],
) -> Vec<ContractDiagnostic> {
    let client = tree.meta[0].path;
    for policy in policies {
        if let (true, PolicyOrigin::Named(name)) = (policy.is_unused(), policy.definition) {
            diagnostics.push(
                ContractDiagnostic::new(
                    "C3-RATE",
                    DiagnosticSeverity::Note,
                    client,
                    name,
                    "rate policy is declared but covers no endpoint",
                )
                .correction("attach it with `rate_limit name(...)` or remove the definition"),
            );
        }
    }
    for set in window_sets {
        if set.used_by.is_empty() {
            diagnostics.push(ContractDiagnostic::new(
                "C3-RATE",
                DiagnosticSeverity::Note,
                client,
                set.name,
                "window set is declared but used by no effective policy",
            ));
        }
    }
    for values in &tree.values {
        for value in values {
            if value.liveness() == ValueLiveness::DeprecatedNoOp {
                diagnostics.push(ContractDiagnostic::new(
                    "C3-PLACE",
                    DiagnosticSeverity::Warning,
                    value.owner,
                    value.name(),
                    "deprecated value is retained as a no-op migration bridge",
                ));
            }
        }
    }
    let mut removed = BTreeSet::new();
    for endpoint in endpoints {
        for displaced in endpoint.auth.displaced() {
            if displaced.mode == AuthResetMode::Off {
                removed.insert(displaced.displaced_by);
            }
        }
    }
    for owner in removed {
        diagnostics.push(ContractDiagnostic::new(
            "C3-AUTH",
            DiagnosticSeverity::Warning,
            owner,
            "auth off",
            "inherited authentication is removed for this owner's endpoints",
        ));
    }
    diagnostics.sort();
    diagnostics.dedup();
    diagnostics
}
