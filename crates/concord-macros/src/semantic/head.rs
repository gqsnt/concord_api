//! Routes, DNS labels and the symbolic query/header patch program
//! (§5.3–§5.4, §6).

use proc_macro2::Span;

use super::{
    Emission, FamilyMode, HeadOp, HostLabel, LiteralKind, LocalHost, LocalPath, LocalProgram,
    OpSite, PathTemplate, PlacementFamily, RAtom, RepresentationChoice, Resolver, SegPart, Segment,
    Site, StatementKind, VRef,
};
use crate::syntax::ast::{
    Atom, Emission as SourceEmission, KeyBlock, KeyStatement, PathOperation,
    RepresentationSelection, SubdomainOperation, Template, TemplatePart,
};
use crate::syntax::diagnostics::{Code, SINGLETON_HEADERS, coded, related};

/// One potential writer of a key (PATCH-02 diagnostics name both writers).
#[derive(Clone)]
pub(crate) struct Writer {
    pub(crate) key: String,
    pub(crate) source: String,
    pub(crate) span: Span,
}

/// The potential key ownership after a site's program.
#[derive(Clone, Default)]
pub(crate) struct Potential {
    pub(crate) query: Vec<Writer>,
    pub(crate) header: Vec<Writer>,
}

impl Potential {
    fn family_mut(&mut self, family: PlacementFamily) -> &mut Vec<Writer> {
        match family {
            PlacementFamily::Query => &mut self.query,
            PlacementFamily::Header => &mut self.header,
        }
    }

    pub(crate) fn contains(&self, family: PlacementFamily, key: &str) -> Option<&Writer> {
        let set = match family {
            PlacementFamily::Query => &self.query,
            PlacementFamily::Header => &self.header,
        };
        set.iter().find(|writer| same_key(family, &writer.key, key))
    }
}

pub(crate) fn same_key(family: PlacementFamily, left: &str, right: &str) -> bool {
    match family {
        PlacementFamily::Query => left == right,
        PlacementFamily::Header => left.eq_ignore_ascii_case(right),
    }
}

fn family_name(family: PlacementFamily) -> &'static str {
    match family {
        PlacementFamily::Query => "query key",
        PlacementFamily::Header => "header",
    }
}

pub(crate) fn resolve_owner(resolver: &mut Resolver, owner: usize) -> syn::Result<()> {
    let site = Site::Owner(owner);
    let source = std::mem::take(&mut resolver.owner_sources[owner].site);
    let host = source
        .subdomain
        .as_ref()
        .map(|clause| resolve_host(resolver, site, &clause.value))
        .transpose()?;
    if let Some(host) = &host {
        validate_host_base(
            resolver,
            site,
            host,
            source
                .subdomain
                .as_ref()
                .map_or(Span::call_site(), |c| c.key_span),
        )?;
    }
    let path = source
        .path
        .as_ref()
        .map(|clause| resolve_path(resolver, site, &clause.value))
        .transpose()?;
    let program = normalize_program(
        resolver,
        site,
        source.query.as_ref().map(|c| (&c.value, c.key_span)),
        source.headers.as_ref().map(|c| (&c.value, c.key_span)),
    )?;
    let parent_potential = resolver.model.owners[owner]
        .parent
        .map(|parent| resolver.owner_potential[parent].clone())
        .unwrap_or_default();
    let display = resolver.model.owners[owner].display.clone();
    let potential = apply_program(resolver, parent_potential, &program, site)?;
    let parent = resolver.model.owners[owner].parent;
    let (parent_timeout, parent_request, parent_response) = parent.map_or_else(
        || (None, Default::default(), Default::default()),
        |parent| {
            let parent = &resolver.model.owners[parent];
            (
                parent.effective_timeout,
                parent.request_default.clone(),
                parent.response_default.clone(),
            )
        },
    );
    let timeout = source.timeout.as_ref().map(|clause| clause.value.value);
    let request_choice = source.request.as_ref().map(|clause| choice(&clause.value));
    let response_choice = source.response.as_ref().map(|clause| choice(&clause.value));
    let entry = &mut resolver.model.owners[owner];
    entry.host = host;
    entry.path = path;
    entry.program = program;
    entry.timeout = timeout;
    entry.effective_timeout = timeout.or(parent_timeout);
    entry.request_default = apply_choice(parent_request, request_choice.as_ref(), &display);
    entry.response_default = apply_choice(parent_response, response_choice.as_ref(), &display);
    entry.request_choice = request_choice;
    entry.response_choice = response_choice;
    resolver.owner_potential[owner] = potential;
    // Auth and rate syntax is consumed by their resolvers.
    resolver.owner_sources[owner].site.auth = source.auth;
    resolver.owner_sources[owner].site.rate_limits = source.rate_limits;
    Ok(())
}

fn choice(selection: &RepresentationSelection) -> RepresentationChoice {
    match selection {
        RepresentationSelection::Select(ty) => RepresentationChoice::Select((**ty).clone()),
        RepresentationSelection::Explicit => RepresentationChoice::Explicit,
    }
}

fn apply_choice(
    inherited: super::RepresentationDefault,
    choice: Option<&RepresentationChoice>,
    display: &str,
) -> super::RepresentationDefault {
    match choice {
        None => inherited,
        Some(RepresentationChoice::Select(ty)) => super::RepresentationDefault {
            ty: Some(ty.clone()),
            origin: Some(display.to_owned()),
            fenced: false,
        },
        Some(RepresentationChoice::Explicit) => super::RepresentationDefault {
            ty: None,
            origin: Some(display.to_owned()),
            fenced: true,
        },
    }
}

pub(crate) fn resolve_endpoint(resolver: &mut Resolver, endpoint: usize) -> syn::Result<()> {
    let site = Site::Endpoint(endpoint);
    let source = std::mem::take(&mut resolver.endpoint_sources[endpoint].site);
    let path = source
        .path
        .as_ref()
        .map(|clause| resolve_path(resolver, site, &clause.value))
        .transpose()?;
    let program = normalize_program(
        resolver,
        site,
        source.query.as_ref().map(|c| (&c.value, c.key_span)),
        source.headers.as_ref().map(|c| (&c.value, c.key_span)),
    )?;
    let owner = resolver.model.endpoints[endpoint].owner;
    let potential = apply_program(
        resolver,
        resolver.owner_potential[owner].clone(),
        &program,
        site,
    )?;
    let timeout = source.timeout.as_ref().map(|clause| clause.value.value);
    let owner_timeout = resolver.model.owners[owner].effective_timeout;
    let entry = &mut resolver.model.endpoints[endpoint];
    entry.path = path;
    entry.program = program;
    entry.timeout = timeout;
    entry.effective_timeout = timeout.or(owner_timeout);
    resolver.endpoint_potential[endpoint] = potential;
    resolver.endpoint_sources[endpoint].site.auth = source.auth;
    resolver.endpoint_sources[endpoint].site.rate_limits = source.rate_limits;
    Ok(())
}

// ---- atoms ----

/// Resolves one atom. `keyed` permits exactly one outer-optional reference as
/// the whole atom (a keyed emission owns outer presence); every other
/// position requires total values.
pub(crate) fn resolve_atom(
    resolver: &Resolver,
    site: Site,
    atom: &Atom,
    keyed: bool,
) -> syn::Result<(RAtom, Option<VRef>)> {
    match atom {
        Atom::Str(literal) => Ok((
            RAtom::Literal {
                text: literal.value(),
                kind: LiteralKind::String,
            },
            None,
        )),
        Atom::Char(literal) => Ok((
            RAtom::Literal {
                text: literal.value().to_string(),
                kind: LiteralKind::Char,
            },
            None,
        )),
        Atom::Bool(literal) => Ok((
            RAtom::Literal {
                text: literal.value.to_string(),
                kind: LiteralKind::Bool,
            },
            None,
        )),
        Atom::Int { negative, literal } => Ok((
            RAtom::Literal {
                text: format!(
                    "{}{}",
                    if *negative { "-" } else { "" },
                    literal.base10_digits()
                ),
                kind: LiteralKind::Integer,
            },
            None,
        )),
        Atom::Reference { reference, via } => {
            let key = resolver.resolve_ref(site, reference)?;
            let value = resolver.model.value(key);
            let presence = if value.optional {
                if !keyed {
                    return Err(coded(
                        reference.span,
                        Code::Atom,
                        format!(
                            "`{}` is outer-optional; optional sources are permitted only as a whole keyed query/header emission, not inside `concat`, a path hole or a subdomain label (in {}); emit it as `\"key\" = {}` or declare it total",
                            reference.display(),
                            resolver.site_label(site),
                            reference.display()
                        ),
                    ));
                }
                Some(key)
            } else {
                None
            };
            Ok((
                RAtom::Value {
                    value: key,
                    via: via.as_ref().map(|via| (**via).clone()),
                    span: reference.span,
                },
                presence,
            ))
        }
        Atom::Concat { parts, span } => {
            if parts.is_empty() {
                return Err(coded(
                    *span,
                    Code::Atom,
                    format!(
                        "`concat []` is empty in {}; a concatenation needs at least one atom",
                        resolver.site_label(site)
                    ),
                ));
            }
            let mut resolved = Vec::with_capacity(parts.len());
            for part in parts {
                resolved.push(resolve_atom(resolver, site, part, false)?.0);
            }
            let flattened = RAtom::Concat(resolved).flatten();
            Ok((RAtom::Concat(flattened), None))
        }
    }
}

fn atom_sensitive(resolver: &Resolver, atom: &RAtom) -> bool {
    let mut values = Vec::new();
    atom.values(&mut values);
    values
        .iter()
        .any(|value| resolver.model.value(*value).sensitive)
}

// ---- host ----

fn resolve_host(
    resolver: &Resolver,
    site: Site,
    operation: &SubdomainOperation,
) -> syn::Result<LocalHost> {
    let labels = |atoms: &[Atom]| -> syn::Result<Vec<HostLabel>> {
        atoms
            .iter()
            .map(|atom| {
                let (atom, _) = resolve_atom(resolver, site, atom, false)?;
                Ok(HostLabel {
                    sensitive: atom_sensitive(resolver, &atom),
                    atom,
                })
            })
            .collect()
    };
    Ok(match operation {
        SubdomainOperation::Prepend(atoms) => LocalHost::Prepend(labels(atoms)?),
        SubdomainOperation::Replace(atoms) => LocalHost::Replace(labels(atoms)?),
        SubdomainOperation::Off => LocalHost::Off,
    })
}

fn validate_host_base(
    resolver: &Resolver,
    site: Site,
    host: &LocalHost,
    span: Span,
) -> syn::Result<()> {
    let labels = match host {
        LocalHost::Prepend(labels) | LocalHost::Replace(labels) => labels,
        LocalHost::Off => return Ok(()),
    };
    if labels.is_empty() {
        return Ok(());
    }
    let base = resolver.model.client.base.value();
    let parsed = url::Url::parse(&base).map_err(|_| {
        coded(
            span,
            Code::Atom,
            format!(
                "invalid base URL (subdomain of {})",
                resolver.site_label(site)
            ),
        )
    })?;
    if !matches!(parsed.host(), Some(url::Host::Domain(_))) {
        return Err(coded(
            span,
            Code::Atom,
            format!(
                "generated subdomain labels require a domain base host; the base is an IP literal (subdomain of {}), so remove the `subdomain` clause or declare a DNS base",
                resolver.site_label(site)
            ),
        ));
    }
    Ok(())
}

// ---- paths ----

fn resolve_path(
    resolver: &Resolver,
    site: Site,
    operation: &PathOperation,
) -> syn::Result<LocalPath> {
    Ok(match operation {
        PathOperation::Append(template) => {
            LocalPath::Append(resolve_template(resolver, site, template)?)
        }
        PathOperation::Replace(template) => {
            LocalPath::Replace(resolve_template(resolver, site, template)?)
        }
        PathOperation::Off => LocalPath::Off,
    })
}

fn resolve_template(
    resolver: &Resolver,
    site: Site,
    template: &Template,
) -> syn::Result<PathTemplate> {
    let text = template.literal.value();
    let mut segments = Vec::with_capacity(template.segments.len());
    for segment in &template.segments {
        let mut parts = Vec::with_capacity(segment.parts.len());
        let mut sensitive = false;
        for part in &segment.parts {
            match part {
                TemplatePart::Literal(text) => parts.push(SegPart::Literal(text.clone())),
                TemplatePart::Hole {
                    reference,
                    via,
                    range,
                } => {
                    let key = resolver.resolve_ref(site, reference).map_err(|error| {
                        let mut error = error;
                        error.combine(coded(
                            template.literal.span(),
                            Code::Path,
                            format!(
                                "in path template hole at bytes {}..{} of {}",
                                range.0,
                                range.1,
                                resolver.site_label(site)
                            ),
                        ));
                        error
                    })?;
                    let value = resolver.model.value(key);
                    if value.optional {
                        let error = coded(
                            template.literal.span(),
                            Code::Path,
                            format!(
                                "path hole `{}` (template bytes {}..{}) of {} names an outer-optional value; a hole names a total value and cannot select optional structure, so declare the value total or emit it as a query/header",
                                reference.display(),
                                range.0,
                                range.1,
                                resolver.site_label(site)
                            ),
                        );
                        return Err(related(
                            error,
                            value.name.span(),
                            "the value is declared here",
                        ));
                    }
                    sensitive |= value.sensitive;
                    parts.push(SegPart::Value {
                        value: key,
                        via: via.clone(),
                        span: template.literal.span(),
                    });
                }
            }
        }
        segments.push(Segment { parts, sensitive });
    }
    Ok(PathTemplate { text, segments })
}

// ---- programs ----

fn normalize_program(
    resolver: &Resolver,
    site: Site,
    query: Option<(&KeyBlock, Span)>,
    headers: Option<(&KeyBlock, Span)>,
) -> syn::Result<LocalProgram> {
    let mut ops = Vec::new();
    let query_mode = normalize_family(resolver, site, PlacementFamily::Query, query, &mut ops)?;
    let header_mode = normalize_family(resolver, site, PlacementFamily::Header, headers, &mut ops)?;
    Ok(LocalProgram {
        query_mode,
        header_mode,
        ops,
    })
}

fn normalize_family(
    resolver: &Resolver,
    site: Site,
    family: PlacementFamily,
    block: Option<(&KeyBlock, Span)>,
    ops: &mut Vec<HeadOp>,
) -> syn::Result<FamilyMode> {
    let (mode, statements, block_span) = match block {
        None => (FamilyMode::Inherit, &[][..], None),
        Some((KeyBlock::Modify(statements), span)) => {
            (FamilyMode::Inherit, statements.as_slice(), Some(span))
        }
        Some((KeyBlock::Replace(statements), span)) => {
            (FamilyMode::Replace, statements.as_slice(), Some(span))
        }
        Some((KeyBlock::Off, span)) => (FamilyMode::Off, &[][..], Some(span)),
    };
    // Step 1: the family fence clears the inherited emission program.
    if matches!(mode, FamilyMode::Replace | FamilyMode::Off) {
        ops.push(HeadOp {
            family,
            kind: None,
            key: String::new(),
            emission: None,
            presence: None,
            site: OpSite::Block,
            sensitive: false,
            span: block_span.unwrap_or_else(Span::call_site),
        });
    }
    // Step 2: direct placements in declaration order.
    let values = match site {
        Site::Owner(owner) => &resolver.model.owners[owner].values,
        Site::Endpoint(endpoint) => &resolver.model.endpoints[endpoint].values,
    };
    for (index, value) in values.iter().enumerate() {
        let Some(placement) = &value.placement else {
            continue;
        };
        if placement.family != family {
            continue;
        }
        if mode == FamilyMode::Off {
            let error = coded(
                block_span.unwrap_or(placement.span),
                Code::Place,
                format!(
                    "`{} off` in {} cannot coexist with the direct {} placement of `{}`; choose a replacement block (`{} replace {{ ... }}`) instead",
                    family_keyword(family),
                    resolver.site_label(site),
                    family_keyword(family),
                    value.logical,
                    family_keyword(family),
                ),
            );
            return Err(related(
                error,
                placement.span,
                "the direct placement is here",
            ));
        }
        let key = VRef { site, index };
        // A direct placement's obligations are reported at the value name
        // it places (DIAG-01).
        let emission = if placement.each {
            Emission::Each {
                value: key,
                via: value.via.clone(),
                span: value.name.span(),
            }
        } else {
            Emission::Atom(RAtom::Value {
                value: key,
                via: value.via.clone(),
                span: value.name.span(),
            })
        };
        ops.push(HeadOp {
            family,
            kind: Some(StatementKind::Introduce),
            key: placement.key.clone(),
            emission: Some(emission),
            presence: value.optional.then_some(key),
            site: OpSite::Direct,
            sensitive: value.sensitive,
            span: placement.span,
        });
    }
    // Step 3: block statements in source order.
    for statement in statements {
        ops.push(resolve_statement(resolver, site, family, statement)?);
    }
    Ok(mode)
}

fn family_keyword(family: PlacementFamily) -> &'static str {
    match family {
        PlacementFamily::Query => "query",
        PlacementFamily::Header => "headers",
    }
}

fn resolve_statement(
    resolver: &Resolver,
    site: Site,
    family: PlacementFamily,
    statement: &KeyStatement,
) -> syn::Result<HeadOp> {
    let key = statement.key.value();
    let (emission, presence, sensitive) = match &statement.emission {
        None => (None, None, false),
        Some(SourceEmission::Atom(atom)) => {
            let (atom, presence) = resolve_atom(resolver, site, atom, true)?;
            let sensitive = atom_sensitive(resolver, &atom);
            (Some(Emission::Atom(atom)), presence, sensitive)
        }
        Some(SourceEmission::Each { reference, via, .. }) => {
            let value = resolver.resolve_ref(site, reference)?;
            let entry = resolver.model.value(value);
            (
                Some(Emission::Each {
                    value,
                    via: via.clone(),
                    span: reference.span,
                }),
                entry.optional.then_some(value),
                entry.sensitive,
            )
        }
    };
    if statement.kind == StatementKind::WhenPresent && presence.is_none() {
        return Err(coded(
            statement.span,
            Code::Override,
            format!(
                "`when_present \"{key}\"` in {} requires an outer-optional source; a total source always replaces — write `replace \"{key}\" = ...` instead",
                resolver.site_label(site)
            ),
        ));
    }
    Ok(HeadOp {
        family,
        kind: Some(statement.kind),
        key,
        emission,
        presence,
        site: OpSite::Block,
        sensitive,
        span: statement.span,
    })
}

/// Applies a site's normalized program to the inherited potential ownership
/// and checks every structural precondition (PATCH-02) and singleton rule.
fn apply_program(
    resolver: &Resolver,
    mut potential: Potential,
    program: &LocalProgram,
    site: Site,
) -> syn::Result<Potential> {
    let display = resolver.site_display(site);
    let label = resolver.site_label(site);
    for op in &program.ops {
        let family = op.family;
        let describe = || {
            let site = match op.site {
                OpSite::Direct => "direct placement",
                OpSite::Block => "block statement",
            };
            format!("{display} {site} `{}`", op.key)
        };
        if family == PlacementFamily::Header
            && SINGLETON_HEADERS.contains(&op.key.to_ascii_lowercase().as_str())
        {
            let repeated = op.kind == Some(StatementKind::Append)
                || matches!(op.emission, Some(Emission::Each { .. }));
            if repeated {
                return Err(coded(
                    op.span,
                    Code::Override,
                    format!(
                        "`{}` is a singleton request field and cannot be repeated with `append` or `each` (in {label}); use one scalar value or adapter",
                        op.key
                    ),
                ));
            }
        }
        let set = potential.family_mut(family);
        let existing = set
            .iter()
            .position(|writer| same_key(family, &writer.key, &op.key));
        match op.kind {
            None => set.clear(),
            Some(StatementKind::Introduce) => {
                if let Some(existing) = existing {
                    let earlier = &set[existing];
                    let error = coded(
                        op.span,
                        Code::Override,
                        format!(
                            "{} `{}` introduced by {} already has a potential writer: {}; select explicitly: `replace` (drop earlier occurrences), `when_present` (replace only when the source is present), `append` (keep both) or `remove`",
                            family_name(family),
                            op.key,
                            describe(),
                            earlier.source
                        ),
                    );
                    return Err(related(
                        error,
                        earlier.span,
                        "the earlier potential writer is here",
                    ));
                }
                set.push(Writer {
                    key: op.key.clone(),
                    source: describe(),
                    span: op.span,
                });
            }
            Some(
                kind
                @ (StatementKind::Replace | StatementKind::WhenPresent | StatementKind::Remove),
            ) => {
                let Some(existing) = existing else {
                    let verb = match kind {
                        StatementKind::Replace => "replace",
                        StatementKind::WhenPresent => "when_present",
                        _ => "remove",
                    };
                    return Err(coded(
                        op.span,
                        Code::Override,
                        format!(
                            "`{verb} \"{}\"` in {label} has no earlier potential writer of this {} in the inherited program; check the key spelling or introduce it with `\"{}\" = ...`",
                            op.key,
                            family_name(family),
                            op.key
                        ),
                    ));
                };
                if kind == StatementKind::Remove {
                    set.remove(existing);
                } else {
                    set[existing] = Writer {
                        key: op.key.clone(),
                        source: describe(),
                        span: op.span,
                    };
                }
            }
            Some(StatementKind::Append) => {
                if existing.is_none() {
                    set.push(Writer {
                        key: op.key.clone(),
                        source: describe(),
                        span: op.span,
                    });
                }
            }
        }
    }
    Ok(potential)
}
