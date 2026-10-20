//! Rate registry, attachments, deduplication and identity (§12).

use std::collections::{HashMap, HashSet};

use quote::ToTokens;

use super::surface::{canonical_ident, public_ident};
use super::{
    BoundDim, FormalDim, Policy, RateSource, RateUse, Resolver, Site, Window, WindowSet, Windows,
    meta_from,
};
use crate::syntax::ast::{
    BoundDimension, FormalDimension, RateAttachment, WindowLiteral, WindowSource,
};
use crate::syntax::diagnostics::{Code, coded, related, suggestion};

fn windows_from(literals: &[WindowLiteral]) -> Vec<Window> {
    literals
        .iter()
        .map(|window| Window {
            capacity: window.capacity,
            duration: window.duration.value,
        })
        .collect()
}

fn validate_windows(
    windows: &[Window],
    cost: u32,
    span: proc_macro2::Span,
    what: &str,
) -> syn::Result<()> {
    if windows.is_empty() {
        return Err(coded(
            span,
            Code::Rate,
            format!("{what} declares no windows; write at least one `allow N per DURATION;`"),
        ));
    }
    if windows.len() > crate::MAX_RATE_LIMIT_WINDOWS_PER_POLICY {
        return Err(coded(
            span,
            Code::Rate,
            format!(
                "{what} declares {} windows; at most {} are allowed, so split the windows across several policies",
                windows.len(),
                crate::MAX_RATE_LIMIT_WINDOWS_PER_POLICY
            ),
        ));
    }
    let mut durations = HashSet::new();
    for window in windows {
        if !durations.insert(window.duration) {
            return Err(coded(
                span,
                Code::Rate,
                format!(
                    "{what} repeats a window duration; contradictory capacities under one duration are rejected, so keep one capacity per duration"
                ),
            ));
        }
        if cost > window.capacity {
            return Err(coded(
                span,
                Code::Rate,
                format!(
                    "{what} has cost {cost}, greater than a window capacity of {}; it could never be admitted, so lower the cost or raise the capacity",
                    window.capacity
                ),
            ));
        }
    }
    Ok(())
}

fn window_set_index(resolver: &Resolver, name: &syn::Ident) -> syn::Result<usize> {
    let logical = canonical_ident(name);
    let window_sets = &resolver.model.client.window_sets;
    window_sets
        .iter()
        .position(|set| set.logical == logical)
        .ok_or_else(|| {
            coded(
                name.span(),
                Code::Rate,
                format!(
                    "unknown window set `{logical}`; declare `windows {logical} {{ allow N per DURATION; }}` in the `rate_limits` registry of {}{}",
                    resolver.model.owner_label(0),
                    suggestion(
                        &logical,
                        window_sets.iter().map(|set| public_ident(&set.logical, name.span()).to_string())
                    )
                ),
            )
        })
}

pub(crate) fn resolve_registry(resolver: &mut Resolver) -> syn::Result<()> {
    let Some(registry) = resolver.registry_source.take() else {
        return Ok(());
    };
    let client = resolver.model.owner_label(0);
    let mut seen = HashMap::new();
    for set in &registry.window_sets {
        let logical = canonical_ident(&set.name);
        if let Some(first) = seen.insert(logical.clone(), set.name.span()) {
            return Err(related(
                coded(
                    set.name.span(),
                    Code::Rate,
                    format!(
                        "duplicate window set `{logical}` in the `rate_limits` registry of {client}; rename or remove one"
                    ),
                ),
                first,
                "the first window set is here",
            ));
        }
        let windows = windows_from(&set.windows);
        validate_windows(
            &windows,
            1,
            set.name.span(),
            &format!("window set `{logical}` of {client}"),
        )?;
        resolver.model.client.window_sets.push(WindowSet {
            logical,
            meta: meta_from(&set.attrs),
            windows,
        });
    }
    let mut names = HashMap::new();
    let mut ids: HashMap<String, proc_macro2::Span> = HashMap::new();
    for declaration in &registry.policies {
        let logical = canonical_ident(&declaration.name);
        if let Some(first) = names.insert(logical.clone(), declaration.name.span()) {
            return Err(related(
                coded(
                    declaration.name.span(),
                    Code::Rate,
                    format!(
                        "duplicate policy `{logical}` in the `rate_limits` registry of {client}; rename or remove one"
                    ),
                ),
                first,
                "the first policy is here",
            ));
        }
        let id = declaration.id.value();
        if let Some(first) = ids.insert(id.clone(), declaration.id.span()) {
            return Err(related(
                coded(
                    declaration.id.span(),
                    Code::Rate,
                    format!(
                        "policy ID `{id}` is declared by two policies of {client}; reference one named policy for intentional reuse"
                    ),
                ),
                first,
                "the first policy with this ID is here",
            ));
        }
        let formal_names: Vec<String> = declaration.formals.iter().map(canonical_ident).collect();
        let mut used = vec![false; formal_names.len()];
        let mut dims = Vec::with_capacity(declaration.key.len());
        let mut labels = HashSet::new();
        for dimension in &declaration.key {
            let (label, dim) = match dimension {
                FormalDimension::Origin(_) => ("origin".to_owned(), FormalDim::Origin),
                FormalDimension::Endpoint(_) => ("endpoint".to_owned(), FormalDim::Endpoint),
                FormalDimension::Custom { label, formal, via } => {
                    let name = canonical_ident(formal);
                    let Some(index) = formal_names.iter().position(|candidate| *candidate == name)
                    else {
                        return Err(coded(
                            formal.span(),
                            Code::Rate,
                            format!(
                                "`{name}` is not a formal of policy `{logical}` of {client}; bare names in a policy refer to its formals, so declare `policy {logical}(..., {name})` or correct the name{}",
                                suggestion(
                                    &name,
                                    declaration.formals.iter().map(ToString::to_string)
                                )
                            ),
                        ));
                    };
                    used[index] = true;
                    (
                        label.value(),
                        FormalDim::Custom {
                            label: label.value(),
                            formal: index,
                            via: via.clone(),
                        },
                    )
                }
            };
            if !labels.insert(label.clone()) {
                return Err(coded(
                    declaration.span,
                    Code::Rate,
                    format!(
                        "policy `{logical}` of {client} repeats key dimension label `{label}`; give each dimension a distinct label"
                    ),
                ));
            }
            dims.push(dim);
        }
        validate_dimension_count(
            dims.len(),
            declaration.span,
            &format!("policy `{logical}` of {client}"),
        )?;
        if let Some(index) = used.iter().position(|used| !used) {
            return Err(coded(
                declaration.formals[index].span(),
                Code::Rate,
                format!(
                    "formal `{}` of policy `{logical}` of {client} is unused; an unused formal falsely advertises a key relation, so remove it or use it in `key [...]`",
                    formal_names[index]
                ),
            ));
        }
        let windows = match &declaration.windows {
            WindowSource::Using(name) => Windows::Set(window_set_index(resolver, name)?),
            WindowSource::Inline(literals) => Windows::Inline(windows_from(literals)),
        };
        let cost = declaration.cost.map(|(cost, _)| cost);
        validate_windows(
            &materialize(resolver, &windows),
            cost.unwrap_or(1),
            declaration.span,
            &format!("policy `{logical}` of {client}"),
        )?;
        resolver.model.client.policies.push(Policy {
            logical,
            meta: meta_from(&declaration.attrs),
            formals: declaration.formals.clone(),
            id,
            dims,
            cost,
            windows,
        });
    }
    Ok(())
}

fn validate_dimension_count(count: usize, span: proc_macro2::Span, what: &str) -> syn::Result<()> {
    if count == 0 || count > crate::MAX_RATE_LIMIT_KEY_PARTS {
        return Err(coded(
            span,
            Code::Rate,
            format!(
                "a rate key has 1..={} dimensions ({what} has {count}); write `key [origin];` at minimum",
                crate::MAX_RATE_LIMIT_KEY_PARTS
            ),
        ));
    }
    Ok(())
}

fn materialize(resolver: &Resolver, windows: &Windows) -> Vec<Window> {
    match windows {
        Windows::Set(index) => resolver.model.client.window_sets[*index].windows.clone(),
        Windows::Inline(windows) => windows.clone(),
    }
}

fn check_dimension_value(
    resolver: &Resolver,
    site: Site,
    value: super::VRef,
    span: proc_macro2::Span,
) -> syn::Result<()> {
    let entry = resolver.model.value(value);
    if entry.optional {
        return Err(coded(
            span,
            Code::Rate,
            format!(
                "rate key value `{}` is outer-optional; a custom dimension of {} uses a required ordinary value, so declare it total (`{}: T` or `{}: T = V`)",
                entry.logical,
                resolver.site_label(site),
                entry.logical,
                entry.logical
            ),
        ));
    }
    Ok(())
}

fn resolve_attachment(
    resolver: &Resolver,
    site: Site,
    attachment: &RateAttachment,
    span: proc_macro2::Span,
) -> syn::Result<RateUse> {
    let origin = resolver.site_display(site);
    let label = resolver.site_label(site);
    match attachment {
        RateAttachment::Named {
            name,
            bindings,
            cost,
        } => {
            let logical = canonical_ident(name);
            let Some(policy_index) = resolver
                .model
                .client
                .policies
                .iter()
                .position(|policy| policy.logical == logical)
            else {
                return Err(coded(
                    name.span(),
                    Code::Rate,
                    format!(
                        "unknown rate policy `{logical}` attached by {label}; declare `policy {logical}(...) {{ ... }}` in the client `rate_limits` registry{}",
                        suggestion(
                            &logical,
                            resolver
                                .model
                                .client
                                .policies
                                .iter()
                                .map(
                                    |policy| public_ident(&policy.logical, name.span()).to_string()
                                )
                        )
                    ),
                ));
            };
            let policy = &resolver.model.client.policies[policy_index];
            let mut bound = vec![None; policy.formals.len()];
            for (formal, reference) in bindings {
                let formal_name = canonical_ident(formal);
                let Some(index) = policy
                    .formals
                    .iter()
                    .position(|candidate| canonical_ident(candidate) == formal_name)
                else {
                    let candidates = policy.formals.iter().filter(|candidate| {
                        !bindings
                            .iter()
                            .any(|(bound, _)| canonical_ident(bound) == canonical_ident(candidate))
                    });
                    return Err(coded(
                        formal.span(),
                        Code::Rate,
                        format!(
                            "policy `{logical}` has no formal `{formal_name}`; bindings are exact (attachment in {label}), so bind only the policy's declared formals{}",
                            suggestion(&formal_name, candidates.map(ToString::to_string))
                        ),
                    ));
                };
                let value = resolver.resolve_ref_with(site, reference, |candidate| {
                    check_dimension_value(resolver, site, candidate, reference.span).is_ok()
                })?;
                check_dimension_value(resolver, site, value, reference.span)?;
                bound[index] = Some((value, reference.span));
            }
            if let Some(missing) = bound.iter().position(Option::is_none) {
                return Err(coded(
                    name.span(),
                    Code::Rate,
                    format!(
                        "policy `{logical}` formal `{}` is unbound; bindings are named, exact and total (attachment in {label}), so write `{} = value`",
                        canonical_ident(&policy.formals[missing]),
                        canonical_ident(&policy.formals[missing])
                    ),
                ));
            }
            let (bindings, binding_spans): (Vec<super::VRef>, Vec<proc_macro2::Span>) = bound
                .into_iter()
                .map(|value| value.expect("checked total"))
                .unzip();
            let dims = policy
                .dims
                .iter()
                .map(|dim| match dim {
                    FormalDim::Origin => BoundDim::Origin,
                    FormalDim::Endpoint => BoundDim::Endpoint,
                    FormalDim::Custom { label, formal, via } => BoundDim::Custom {
                        label: label.clone(),
                        value: bindings[*formal],
                        via: via.clone(),
                        span: binding_spans[*formal],
                    },
                })
                .collect();
            let attachment_cost = cost.map(|(cost, _)| cost);
            let effective_cost = attachment_cost.or(policy.cost).unwrap_or(1);
            let windows = materialize(resolver, &policy.windows);
            validate_windows(
                &windows,
                effective_cost,
                span,
                &format!("attachment of policy `{logical}` in {label}"),
            )?;
            Ok(RateUse {
                source: RateSource::Named {
                    policy: policy_index,
                    bindings,
                },
                id: policy.id.clone(),
                dims,
                windows,
                cost: effective_cost,
                attachment_cost,
                origin,
                span,
            })
        }
        RateAttachment::Inline {
            id,
            key,
            cost,
            windows,
        } => {
            let mut dims = Vec::with_capacity(key.len());
            let mut labels = HashSet::new();
            for dimension in key {
                let (label, dim) = match dimension {
                    BoundDimension::Origin(_) => ("origin".to_owned(), BoundDim::Origin),
                    BoundDimension::Endpoint(_) => ("endpoint".to_owned(), BoundDim::Endpoint),
                    BoundDimension::Custom {
                        label,
                        reference,
                        via,
                    } => {
                        let value = resolver.resolve_ref_with(site, reference, |candidate| {
                            check_dimension_value(resolver, site, candidate, reference.span).is_ok()
                        })?;
                        check_dimension_value(resolver, site, value, reference.span)?;
                        (
                            label.value(),
                            BoundDim::Custom {
                                label: label.value(),
                                value,
                                via: via.clone(),
                                span: reference.span,
                            },
                        )
                    }
                };
                if !labels.insert(label.clone()) {
                    return Err(coded(
                        span,
                        Code::Rate,
                        format!(
                            "inline policy `{}` of {} repeats key dimension label `{label}`; give each dimension a distinct label",
                            id.value(),
                            resolver.site_label(site)
                        ),
                    ));
                }
                dims.push(dim);
            }
            validate_dimension_count(
                dims.len(),
                span,
                &format!("inline policy `{}` of {label}", id.value()),
            )?;
            let windows = match windows {
                WindowSource::Using(name) => Windows::Set(window_set_index(resolver, name)?),
                WindowSource::Inline(literals) => Windows::Inline(windows_from(literals)),
            };
            let attachment_cost = cost.map(|(cost, _)| cost);
            let effective_cost = attachment_cost.unwrap_or(1);
            let materialized = materialize(resolver, &windows);
            validate_windows(
                &materialized,
                effective_cost,
                span,
                &format!("inline policy `{}` of {label}", id.value()),
            )?;
            Ok(RateUse {
                source: RateSource::Inline {
                    id: id.value(),
                    dims: dims.clone(),
                    windows,
                },
                id: id.value(),
                dims,
                windows: materialized,
                cost: effective_cost,
                attachment_cost,
                origin,
                span,
            })
        }
    }
}

#[derive(Eq, PartialEq)]
enum RateSourceKey {
    Named(usize, Vec<super::VRef>),
    Inline,
}

#[derive(Eq, PartialEq)]
enum RateDimensionKey {
    Origin,
    Endpoint,
    Custom(String, super::VRef, Option<String>),
}

#[derive(Eq, PartialEq)]
struct RateStructuralKey {
    source: RateSourceKey,
    id: String,
    dimensions: Vec<RateDimensionKey>,
    windows: Vec<(std::time::Duration, u32)>,
    cost: u32,
}

/// Framed structural identity; arbitrary valid labels cannot impersonate
/// another dimension, value reference or encoder. Canonical window ordering
/// preserves the prior schema equivalence, while dimension order is exact.
fn structural_key(rate: &RateUse) -> RateStructuralKey {
    let dimensions = rate
        .dims
        .iter()
        .map(|dim| match dim {
            BoundDim::Origin => RateDimensionKey::Origin,
            BoundDim::Endpoint => RateDimensionKey::Endpoint,
            BoundDim::Custom {
                label, value, via, ..
            } => RateDimensionKey::Custom(
                label.clone(),
                *value,
                via.as_ref().map(|via| via.to_token_stream().to_string()),
            ),
        })
        .collect();
    let mut windows = rate
        .windows
        .iter()
        .map(|window| (window.duration, window.capacity))
        .collect::<Vec<_>>();
    windows.sort();
    let source = match &rate.source {
        RateSource::Named { policy, bindings } => RateSourceKey::Named(*policy, bindings.clone()),
        RateSource::Inline { .. } => RateSourceKey::Inline,
    };
    RateStructuralKey {
        source,
        id: rate.id.clone(),
        dimensions,
        windows,
        cost: rate.cost,
    }
}

/// `label` is the diagnostic owner label of the composing site.
fn compose(
    inherited: &[RateUse],
    local: Vec<RateUse>,
    label: &str,
) -> syn::Result<(Vec<RateUse>, Vec<RateUse>)> {
    let mut effective = inherited.to_vec();
    let mut kept = Vec::new();
    for rate in local {
        if let Some(existing) = effective.iter().find(|existing| existing.id == rate.id) {
            if structural_key(existing) == structural_key(&rate) {
                // Structurally identical repeated attachment: deduplicated.
                continue;
            }
            let error = coded(
                rate.span,
                Code::Rate,
                format!(
                    "{label} would carry rate identity `{}` twice with different bindings, schema or cost (the other attachment is from {}); a conflicting repeated identity on one endpoint is an error, so give one policy a distinct ID or attach it identically",
                    rate.id, existing.origin
                ),
            );
            return Err(related(
                error,
                existing.span,
                "the other attachment is here",
            ));
        }
        if effective.len() >= crate::MAX_RATE_LIMIT_POLICIES_PER_PLAN {
            // Report the first excess attachment itself (DIAG-01 primary span).
            return Err(coded(
                rate.span,
                Code::Rate,
                format!(
                    "{label} has more than {} effective rate policies; attach broad policies on fewer owners",
                    crate::MAX_RATE_LIMIT_POLICIES_PER_PLAN
                ),
            ));
        }
        effective.push(rate.clone());
        kept.push(rate);
    }
    Ok((kept, effective))
}

pub(crate) fn resolve_owner(resolver: &mut Resolver, owner: usize) -> syn::Result<()> {
    let site = Site::Owner(owner);
    let sources = std::mem::take(&mut resolver.owner_sources[owner].site.rate_limits);
    let local = sources
        .iter()
        .map(|clause| resolve_attachment(resolver, site, &clause.value, clause.key_span))
        .collect::<syn::Result<Vec<_>>>()?;
    let inherited = resolver.model.owners[owner]
        .parent
        .map(|parent| resolver.model.owners[parent].effective_rates.clone())
        .unwrap_or_default();
    let label = resolver.site_label(site);
    let (kept, effective) = compose(&inherited, local, &label)?;
    let entry = &mut resolver.model.owners[owner];
    entry.rates = kept;
    entry.effective_rates = effective;
    Ok(())
}

pub(crate) fn resolve_endpoint(resolver: &mut Resolver, endpoint: usize) -> syn::Result<()> {
    let site = Site::Endpoint(endpoint);
    let sources = std::mem::take(&mut resolver.endpoint_sources[endpoint].site.rate_limits);
    let local = sources
        .iter()
        .map(|clause| resolve_attachment(resolver, site, &clause.value, clause.key_span))
        .collect::<syn::Result<Vec<_>>>()?;
    let owner = resolver.model.endpoints[endpoint].owner;
    let label = resolver.site_label(site);
    let (kept, effective) = compose(&resolver.model.owners[owner].effective_rates, local, &label)?;
    let entry = &resolver.model.endpoints[endpoint];
    if !entry.id_explicit {
        if let Some(rate) = effective.iter().find(|rate| {
            rate.dims
                .iter()
                .any(|dim| matches!(dim, BoundDim::Endpoint))
        }) {
            let error = coded(
                entry.span,
                Code::Rate,
                format!(
                    "{label} is covered by rate policy `{}` with an `endpoint` dimension, which consumes the operational endpoint ID; pin it with an explicit `id \"...\";`",
                    rate.id
                ),
            );
            return Err(related(error, rate.span, "the attachment is here"));
        }
    }
    let entry = &mut resolver.model.endpoints[endpoint];
    entry.rates = kept;
    entry.effective_rates = effective;
    Ok(())
}

/// Definitions sharing a policy ID must agree on key schema and windows
/// within the closed declaration.
pub(crate) fn validate_identities(resolver: &Resolver) -> syn::Result<()> {
    let mut schemas: HashMap<String, (String, proc_macro2::Span)> = HashMap::new();
    let all = resolver
        .model
        .owners
        .iter()
        .flat_map(|owner| owner.rates.iter())
        .chain(
            resolver
                .model
                .endpoints
                .iter()
                .flat_map(|endpoint| endpoint.rates.iter()),
        );
    for rate in all {
        let labels = rate
            .dims
            .iter()
            .map(|dim| match dim {
                BoundDim::Origin => "origin".to_owned(),
                BoundDim::Endpoint => "endpoint".to_owned(),
                BoundDim::Custom { label, via, .. } => format!(
                    "{label}/{}",
                    via.as_ref()
                        .map(|via| via.to_token_stream().to_string())
                        .unwrap_or_default()
                ),
            })
            .collect::<Vec<_>>();
        let mut windows = rate
            .windows
            .iter()
            .map(|window| (window.duration, window.capacity))
            .collect::<Vec<_>>();
        windows.sort();
        let schema = format!("{labels:?}|{windows:?}");
        if let Some((first, span)) = schemas.get(&rate.id) {
            if *first != schema {
                let error = coded(
                    rate.span,
                    Code::Rate,
                    format!(
                        "rate policy ID `{}` is defined with a different key schema or window set elsewhere in {}; one ID has one schema, so give the policies distinct IDs or reuse one named policy",
                        rate.id,
                        resolver.model.owner_label(0)
                    ),
                );
                return Err(related(error, *span, "the other definition is here"));
            }
        } else {
            schemas.insert(rate.id.clone(), (schema, rate.span));
        }
    }
    Ok(())
}

#[cfg(test)]
mod s4_rate_identity_regressions {
    fn resolve(keys: &str) -> Result<crate::semantic::Model, syn::Error> {
        let source = format!(
            "client RateIdentity {{ base \"https://fixture.test/\"; values {{ tenant: String = \"fixture\"; }} {keys} GET read() -> () via Empty; }}"
        );
        crate::semantic::resolve(crate::syntax::parse(source.parse().unwrap()).unwrap())
    }
    #[test]
    fn conflicting_dimension_labels_cannot_impersonate_structural_key_syntax() {
        let first =
            "rate_limit { id \"s4.collision\"; key [\"A\"=tenant,\"B\"=tenant]; allow 10 per 1s; }";
        for second in [
            "rate_limit { id \"s4.collision\"; key [\"A=VRef { site: Owner(0), index: 0 }/,B\"=tenant]; allow 10 per 1s; }",
            "rate_limit { id \"s4.collision\"; key [\"C\"=tenant]; allow 10 per 1s; }",
            "rate_limit { id \"s4.collision\"; key [\"B\"=tenant,\"A\"=tenant]; allow 10 per 1s; }",
        ] {
            let error = resolve(&format!("{first} {second}"))
                .err()
                .expect("conflicting schema must reject");
            assert!(error.to_string().contains("C3-RATE"));
        }
        let identical = resolve(&format!("{first} {first}")).expect("identical attachment dedup");
        assert_eq!(identical.endpoints[0].effective_rates.len(), 1);
        let single=resolve("rate_limit { id \"s4.single\"; key [\"A=VRef { site: Owner(0), index: 0 }/,B\"=tenant]; allow 10 per 1s; }").unwrap();
        assert_eq!(single.endpoints[0].effective_rates[0].dims.len(), 1);
        // Canonical window ordering is intentionally preserved.
        resolve("rate_limit { id \"s4.windows\"; key [\"tenant\"=tenant]; allow 10 per 1s; allow 20 per 2s; } rate_limit { id \"s4.windows\"; key [\"tenant\"=tenant]; allow 20 per 2s; allow 10 per 1s; }").unwrap();
    }
}
