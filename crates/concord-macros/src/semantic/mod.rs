//! Semantic resolution of the v3 declaration language.
//!
//! Resolution order:
//!
//! 1. flatten the owner/endpoint tree (preorder, declaration order) and
//!    register every value with its structural state;
//! 2. resolve credential sources, secrets and the rate registry;
//! 3. for every owner (parents first) and then every endpoint, resolve local
//!    attachments against LANG-03 reference rules, normalize query/header
//!    programs (inherited → family fence → direct placements → block
//!    statements), check structural preconditions (PATCH-02), header
//!    ownership (HEAD-01), auth (AUTH-01) and rates (RATE-01..03), and
//!    compute effective views;
//! 4. select representations, status/effect/absence/pagination facts and
//!    operational identities;
//! 5. compute liveness (VALUE-06), generated names and collisions (CALL-01)
//!    and the limiter requirement.

pub(crate) mod model;
pub(crate) mod surface;

mod auth;
mod endpoint;
mod head;
mod limits;
mod names;
mod rates;
mod tree;

#[cfg(test)]
mod tests;

use crate::syntax::ast::{Attrs, Reference, SourceApi};
use crate::syntax::diagnostics::{Code, coded, edit_distance};
pub(crate) use model::*;
use surface::canonical_ident;

/// Resolves one parsed declaration into the semantic model.
pub(crate) fn resolve(api: SourceApi) -> syn::Result<Model> {
    let mut resolver = tree::build(api)?;
    auth::resolve_credentials(&mut resolver)?;
    rates::resolve_registry(&mut resolver)?;
    for owner in 0..resolver.model.owners.len() {
        head::resolve_owner(&mut resolver, owner)?;
        auth::resolve_owner(&mut resolver, owner)?;
        rates::resolve_owner(&mut resolver, owner)?;
    }
    for endpoint in 0..resolver.model.endpoints.len() {
        endpoint::resolve_contract(&mut resolver, endpoint)?;
        head::resolve_endpoint(&mut resolver, endpoint)?;
        auth::resolve_endpoint(&mut resolver, endpoint)?;
        rates::resolve_endpoint(&mut resolver, endpoint)?;
        endpoint::resolve_pagination(&mut resolver, endpoint)?;
    }
    auth::validate_credential_graph(&resolver)?;
    rates::validate_identities(&resolver)?;
    limits::check_static_head_bounds(&resolver.model)?;
    names::resolve_liveness(&mut resolver)?;
    names::resolve_identities(&mut resolver)?;
    names::resolve_surfaces(&mut resolver)?;
    names::check_shadowing(&resolver)?;
    resolver.model.client.requires_limiter = resolver
        .model
        .owners
        .iter()
        .any(|owner| !owner.effective_rates.is_empty())
        || resolver
            .model
            .endpoints
            .iter()
            .any(|endpoint| !endpoint.effective_rates.is_empty());
    Ok(resolver.model)
}

/// Syntax retained per owner/endpoint until its attachments are resolved.
pub(crate) struct Resolver {
    pub(crate) model: Model,
    pub(crate) owner_sources: Vec<tree::OwnerSource>,
    pub(crate) endpoint_sources: Vec<crate::syntax::ast::SourceEndpoint>,
    pub(crate) credentials_source: Option<crate::syntax::ast::CredentialsBlock>,
    pub(crate) registry_source: Option<crate::syntax::ast::RateRegistry>,
    /// Final potential key writers per owner (for structural checks).
    pub(crate) owner_potential: Vec<head::Potential>,
    /// Final potential key writers per endpoint.
    pub(crate) endpoint_potential: Vec<head::Potential>,
}

impl Resolver {
    /// Diagnostic owner label of `site` (``endpoint `Api::item` ``).
    pub(crate) fn site_label(&self, site: Site) -> String {
        self.model.site_label(site)
    }

    pub(crate) fn site_display(&self, site: Site) -> String {
        match site {
            Site::Owner(owner) => self.model.owners[owner].display.clone(),
            Site::Endpoint(endpoint) => self.model.endpoints[endpoint].display.clone(),
        }
    }

    fn site_values(&self, site: Site) -> &[Value] {
        match site {
            Site::Owner(owner) => &self.model.owners[owner].values,
            Site::Endpoint(endpoint) => &self.model.endpoints[endpoint].values,
        }
    }

    /// Owner chain (root first) visible from `site`, excluding an endpoint.
    pub(crate) fn owner_chain(&self, site: Site) -> Vec<usize> {
        match site {
            Site::Owner(owner) => self.model.chain(owner),
            Site::Endpoint(endpoint) => self.model.chain(self.model.endpoints[endpoint].owner),
        }
    }

    /// Only references that obey the current-owner/ancestor visibility rule.
    fn reference_candidates(&self, site: Site) -> Vec<(String, String, VRef)> {
        let mut candidates = self
            .site_values(site)
            .iter()
            .enumerate()
            .map(|(index, value)| {
                (
                    value.logical.clone(),
                    value.name.to_string(),
                    VRef { site, index },
                )
            })
            .collect::<Vec<_>>();
        for ancestor in self.owner_chain(site) {
            if Site::Owner(ancestor) == site {
                continue;
            }
            let qualifier = if ancestor == 0 {
                "client".to_owned()
            } else {
                surface::public_ident(
                    &self.model.owners[ancestor].logical,
                    proc_macro2::Span::call_site(),
                )
                .to_string()
            };
            for (index, value) in self.site_values(Site::Owner(ancestor)).iter().enumerate() {
                candidates.push((
                    value.logical.clone(),
                    format!("{qualifier}.{}", value.name),
                    VRef {
                        site: Site::Owner(ancestor),
                        index,
                    },
                ));
            }
        }
        candidates
    }

    fn reference_suggestion(
        &self,
        site: Site,
        reference: &Reference,
        eligible: &impl Fn(VRef) -> bool,
    ) -> String {
        let qualified = reference.owner.is_some();
        let spelling = reference.display();
        let mut best = None;
        let mut matches = Vec::new();
        for (name, replacement, key) in self.reference_candidates(site) {
            if !eligible(key) {
                continue;
            }
            // A bare misspelling is compared with leaf names, but an ancestor
            // replacement must include its qualifier to be usable as written.
            let candidate = if qualified {
                replacement.replace("r#", "")
            } else {
                name
            };
            let Some(distance) = edit_distance(&spelling, &candidate) else {
                continue;
            };
            if best.is_none_or(|best| distance < best) {
                matches.clear();
                best = Some(distance);
            }
            if best == Some(distance) {
                matches.push(replacement);
            }
        }
        matches.sort();
        matches.dedup();
        if matches.is_empty() {
            String::new()
        } else {
            format!("; did you mean `{}`?", matches.join("` or `"))
        }
    }

    /// Resolves one reference in the context of `site` (LANG-03).
    pub(crate) fn resolve_ref(&self, site: Site, reference: &Reference) -> syn::Result<VRef> {
        self.resolve_ref_with(site, reference, |_| true)
    }

    /// Filters spelling candidates by the semantic requirements of a use site.
    /// Exact references keep their existing validation and diagnostics.
    pub(crate) fn resolve_ref_with(
        &self,
        site: Site,
        reference: &Reference,
        eligible: impl Fn(VRef) -> bool,
    ) -> syn::Result<VRef> {
        let name = canonical_ident(&reference.name);
        let find = |site: Site| {
            self.site_values(site)
                .iter()
                .position(|value| value.logical == name)
                .map(|index| VRef { site, index })
        };
        let Some(owner) = &reference.owner else {
            if let Some(found) = find(site) {
                return Ok(found);
            }
            let mut suggestions = Vec::new();
            for ancestor in self.owner_chain(site).into_iter().rev() {
                if Site::Owner(ancestor) == site {
                    continue;
                }
                if find(Site::Owner(ancestor)).is_some() {
                    suggestions.push(if ancestor == 0 {
                        format!("client.{name}")
                    } else {
                        format!("{}.{name}", self.model.owners[ancestor].logical)
                    });
                }
            }
            let hint = if suggestions.is_empty() {
                format!(
                    "; declare `{name}` as a value of this owner, or qualify an ancestor value as `client.{name}` or `<scope>.{name}`{}",
                    self.reference_suggestion(site, reference, &eligible)
                )
            } else {
                format!(
                    "; a bare name binds only the current owner's values — did you mean `{}`?",
                    suggestions.join("` or `")
                )
            };
            return Err(coded(
                reference.span,
                Code::Name,
                format!("`{name}` is not a value of {}{hint}", self.site_label(site)),
            ));
        };
        let qualifier = canonical_ident(owner);
        if qualifier == "endpoint" {
            return Err(coded(
                reference.span,
                Code::Migrate,
                format!(
                    "`endpoint.{name}` is redundant qualification of the current endpoint ({}); write the bare name `{name}`",
                    self.site_label(site)
                ),
            ));
        }
        let chain = self.owner_chain(site);
        let target = if qualifier == "client" {
            0
        } else {
            let Some(target) = chain.iter().copied().find(|ancestor| {
                *ancestor != 0 && self.model.owners[*ancestor].logical == qualifier
            }) else {
                return Err(coded(
                    owner.span(),
                    Code::Name,
                    format!(
                        "`{qualifier}` is not the client or a visible ancestor scope of {}; qualify with `client.` or the name of an enclosing scope{}",
                        self.site_label(site),
                        self.reference_suggestion(site, reference, &eligible)
                    ),
                ));
            };
            target
        };
        if Site::Owner(target) == site {
            return Err(coded(
                reference.span,
                Code::Migrate,
                format!(
                    "`{qualifier}.{name}` redundantly qualifies the current owner ({}); write the bare name `{name}`",
                    self.site_label(site)
                ),
            ));
        }
        find(Site::Owner(target)).ok_or_else(|| {
            coded(
                reference.name.span(),
                Code::Name,
                format!(
                    "`{name}` is not a value of {} (referenced from {}); declare it there or qualify the owner that declares it{}",
                    self.model.owner_label(target),
                    self.site_label(site),
                    self.reference_suggestion(site, reference, &eligible)
                ),
            )
        })
    }
}

pub(crate) fn meta_from(attrs: &Attrs) -> Meta {
    Meta {
        docs: attrs.docs.iter().map(|doc| doc.value()).collect(),
        deprecated: attrs.deprecated.as_ref().map(|deprecation| Deprecation {
            note: deprecation.note.as_ref().map(syn::LitStr::value),
            since: deprecation.since.as_ref().map(syn::LitStr::value),
        }),
    }
}
