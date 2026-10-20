//! Endpoint contract facts: representations, status, effect, absence,
//! accepted-bytes exposure, identity and pagination (§9, §10, §12.4, §14).

use super::{BoundDim, EffectDecl, Emission, InitValue, Pagination, Resolver, Site, Status, VRef};
use crate::syntax::ast::{InitValue as SourceInit, StatusDecl};
use crate::syntax::diagnostics::{Code, coded, related};

pub(crate) fn resolve_contract(resolver: &mut Resolver, endpoint: usize) -> syn::Result<()> {
    let source = &resolver.endpoint_sources[endpoint];
    let owner = resolver.model.endpoints[endpoint].owner;
    let owner_entry = &resolver.model.owners[owner];
    let entry = &resolver.model.endpoints[endpoint];
    let label = resolver.site_label(Site::Endpoint(endpoint));

    // Response representation: inline `via` wins, else the nearest default.
    let (response, response_origin) = match &entry.output_via {
        Some(via) => (via.clone(), "inline `via`".to_owned()),
        None => match &owner_entry.response_default.ty {
            Some(ty) => (
                ty.clone(),
                owner_entry
                    .response_default
                    .origin
                    .clone()
                    .unwrap_or_default(),
            ),
            None => {
                let fence = if owner_entry.response_default.fenced {
                    format!(
                        " (`response explicit` in {} blocks inherited defaults)",
                        owner_entry
                            .response_default
                            .origin
                            .clone()
                            .unwrap_or_default()
                    )
                } else {
                    String::new()
                };
                return Err(coded(
                    entry.span,
                    Code::Rep,
                    format!(
                        "{label} has no response representation{fence}; declare `response R;` on an owner or write `-> T via R` (no codec is guessed from the result type)"
                    ),
                ));
            }
        },
    };
    let body_representation = match &entry.body {
        None => None,
        Some(body) => Some(match &body.via {
            Some(via) => (via.clone(), "inline `via`".to_owned()),
            None => match &owner_entry.request_default.ty {
                Some(ty) => (
                    ty.clone(),
                    owner_entry
                        .request_default
                        .origin
                        .clone()
                        .unwrap_or_default(),
                ),
                None => {
                    return Err(coded(
                        body.name.span(),
                        Code::Rep,
                        format!(
                            "request body `{}` of {label} has no request representation{}; declare `request R;` on an owner or write `body {}: T via R`",
                            body.logical,
                            if owner_entry.request_default.fenced {
                                format!(
                                    " (`request explicit` in {} blocks inherited defaults)",
                                    owner_entry
                                        .request_default
                                        .origin
                                        .clone()
                                        .unwrap_or_default()
                                )
                            } else {
                                String::new()
                            },
                            body.logical
                        ),
                    ));
                }
            },
        }),
    };

    let (status, status_explicit) = match source.status.as_ref().map(|clause| &clause.value) {
        None | Some(StatusDecl::Success) => (Status::Success, source.status.is_some()),
        Some(StatusDecl::Codes(codes)) => (Status::Ranges(ranges(codes)), true),
    };
    let (effect, effect_explicit) = match &source.effect {
        Some(clause) => (clause.value, true),
        None => (entry.method.default_effect(), false),
    };
    let absence = match &source.absence {
        None => None,
        Some(clause) => {
            let mut codes = clause.value.clone();
            codes.sort_unstable();
            if codes.windows(2).any(|pair| pair[0] == pair[1]) {
                return Err(coded(
                    clause.key_span,
                    Code::Rep,
                    format!("`absent on` of {label} repeats a status; list each status once"),
                ));
            }
            for code in &codes {
                if !(200..=599).contains(code) {
                    return Err(coded(
                        clause.key_span,
                        Code::Rep,
                        format!(
                            "absence statuses of {label} are final statuses in 200..=599; write a status such as `absent on 404;`"
                        ),
                    ));
                }
                if status.contains(*code) {
                    return Err(coded(
                        clause.key_span,
                        Code::Rep,
                        format!(
                            "absence status {code} of {label} overlaps the accepted payload statuses; the sets are disjoint, so remove {code} from `status` or from `absent on`"
                        ),
                    ));
                }
            }
            if source.pagination.is_some() {
                return Err(coded(
                    clause.key_span,
                    Code::Rep,
                    format!(
                        "`absent on` cannot combine with pagination in {label}; remove `absent on` or `paginate`"
                    ),
                ));
            }
            if source.expose_accepted_bytes.is_some() {
                return Err(coded(
                    clause.key_span,
                    Code::Rep,
                    format!(
                        "`absent on` cannot combine with `expose accepted_bytes` in {label}; use the buffered `NotFound<R>` representation when raw bytes are needed"
                    ),
                ));
            }
            Some(codes)
        }
    };
    let absence_span = source.absence.as_ref().map(|clause| clause.key_span);
    let (id, id_explicit, id_span) = match &source.id {
        Some(clause) => (clause.value.value(), true, clause.value.span()),
        None => (String::new(), false, entry.span),
    };

    let entry = &mut resolver.model.endpoints[endpoint];
    entry.response = response;
    entry.response_origin = response_origin;
    if let (Some(body), Some((representation, origin))) = (&mut entry.body, body_representation) {
        body.representation = representation;
        body.representation_origin = origin;
    }
    entry.status = status;
    entry.status_explicit = status_explicit;
    entry.effect = effect;
    entry.effect_explicit = effect_explicit;
    entry.absence = absence;
    if let Some(span) = absence_span {
        entry.absence_span = span;
    }
    if id_explicit {
        entry.id = id;
    }
    entry.id_explicit = id_explicit;
    entry.id_span = id_span;
    Ok(())
}

fn ranges(codes: &[u16]) -> Vec<(u16, u16)> {
    let mut out: Vec<(u16, u16)> = Vec::new();
    for code in codes {
        match out.last_mut() {
            Some((_, end)) if *end + 1 == *code => *end = *code,
            _ => out.push((*code, *code)),
        }
    }
    out
}

pub(crate) fn resolve_pagination(resolver: &mut Resolver, endpoint: usize) -> syn::Result<()> {
    let Some(clause) = resolver.endpoint_sources[endpoint].pagination.take() else {
        return Ok(());
    };
    let declaration = clause.value;
    let site = Site::Endpoint(endpoint);
    let label = resolver.site_label(site);
    let entry = &resolver.model.endpoints[endpoint];
    if entry.effect != EffectDecl::Safe {
        return Err(coded(
            clause.key_span,
            Code::Page,
            format!(
                "{label} is not `effect safe`; page eligibility requires a safe effect (pagination replays continuations), so write `effect safe;` when every page request is a read"
            ),
        ));
    }
    let mut fields = Vec::with_capacity(declaration.init_fields.len());
    for (field, value) in &declaration.init_fields {
        let value = match value {
            SourceInit::Stable(stable) => InitValue::Stable(stable.clone()),
            SourceInit::Reference(reference) => {
                InitValue::Value(resolver.resolve_ref(site, reference)?)
            }
        };
        fields.push((field.clone(), value));
    }
    let mut cursor = Vec::with_capacity(declaration.cursor.len());
    for destination in &declaration.cursor {
        let name = super::surface::canonical_ident(destination);
        let Some(index) = entry.values.iter().position(|value| value.logical == name) else {
            if entry.body.as_ref().is_some_and(|body| body.logical == name) {
                return Err(coded(
                    destination.span(),
                    Code::Page,
                    format!(
                        "a cursor destination of {label} cannot be the request body; map the cursor to an endpoint value slot"
                    ),
                ));
            }
            return Err(coded(
                destination.span(),
                Code::Page,
                format!(
                    "cursor destination `{name}` is not an endpoint-owned value of {label}; destinations are bare endpoint slots, never client or scope values, so declare `{name}` as a parameter of the endpoint{}",
                    crate::syntax::diagnostics::suggestion(
                        &name,
                        entry.values.iter().enumerate().filter(|(index, value)| {
                            let key = VRef { site, index: *index };
                            !declaration.cursor.iter().any(|destination| {
                                super::surface::canonical_ident(destination) == value.logical
                            })
                                && !value.placement.as_ref().is_some_and(|placement| placement.each)
                                && !entry.program.ops.iter().any(|op| {
                                    matches!(&op.emission, Some(Emission::Each { value, .. }) if *value == key)
                                })
                                && !entry.effective_rates.iter().any(|rate| {
                                    rate.dims.iter().any(|dim| {
                                        matches!(dim, BoundDim::Custom { value, .. } if *value == key)
                                    })
                                })
                        }).map(|(_, value)| value.name.to_string()),
                    )
                ),
            ));
        };
        if cursor.contains(&index) {
            return Err(coded(
                destination.span(),
                Code::Page,
                format!("cursor destination `{name}` of {label} is repeated; name each slot once"),
            ));
        }
        let key = VRef { site, index };
        let value = &entry.values[index];
        let each_placed = value
            .placement
            .as_ref()
            .is_some_and(|placement| placement.each);
        let each_used =
            entry.program.ops.iter().any(
                |op| matches!(&op.emission, Some(Emission::Each { value, .. }) if *value == key),
            );
        if each_placed || each_used {
            return Err(coded(
                destination.span(),
                Code::Page,
                format!(
                    "cursor destination `{name}` of {label} is an `each` source; destinations are scalar slots, so map the cursor to a scalar value"
                ),
            ));
        }
        if let Some(rate) = entry.effective_rates.iter().find(|rate| {
            rate.dims
                .iter()
                .any(|dim| matches!(dim, BoundDim::Custom { value, .. } if *value == key))
        }) {
            let error = coded(
                destination.span(),
                Code::Page,
                format!(
                    "cursor destination `{name}` of {label} participates in the effective rate key of `{}`; page-live values cannot be quota dimensions, so key the rate policy on a value the cursor does not change",
                    rate.id
                ),
            );
            return Err(related(error, rate.span, "the rate attachment is here"));
        }
        cursor.push(index);
    }
    let pagination = Pagination {
        controller: declaration.controller,
        init_type: declaration.init_type,
        fields,
        cursor,
        tuple: declaration.tuple,
        span: clause.key_span,
        cursor_span: declaration.cursor_span,
    };
    let entry = &mut resolver.model.endpoints[endpoint];
    for index in &pagination.cursor {
        entry.values[*index].page_live = true;
    }
    entry.pagination = Some(pagination);
    Ok(())
}
