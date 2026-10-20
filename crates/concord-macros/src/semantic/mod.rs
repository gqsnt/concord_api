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
use crate::syntax::diagnostics::{Code, coded};
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

    fn correction_ref(&self, key: VRef, from: Site) -> String {
        let value = self.model.value(key);
        let name = crate::suggestions::identifier_spelling(&value.logical);
        if key.site == from {
            return name;
        }
        match key.site {
            Site::Owner(0) => format!("client.{name}"),
            Site::Owner(owner) => format!(
                "{}.{name}",
                crate::suggestions::identifier_spelling(&self.model.owners[owner].logical)
            ),
            Site::Endpoint(_) => name,
        }
    }

    /// Valid value references paired with the name used for spelling distance.
    fn visible_values(&self, site: Site) -> Vec<(String, String)> {
        let mut sites = vec![site];
        sites.extend(
            self.owner_chain(site)
                .into_iter()
                .rev()
                .map(Site::Owner)
                .filter(|ancestor| *ancestor != site)
                .filter(|ancestor| match ancestor {
                    Site::Owner(0) => true,
                    Site::Owner(owner) => !matches!(
                        self.model.owners[*owner].logical.as_str(),
                        "client" | "endpoint"
                    ),
                    Site::Endpoint(_) => false,
                }),
        );
        sites
            .into_iter()
            .flat_map(|owner| {
                self.site_values(owner)
                    .iter()
                    .enumerate()
                    .map(move |(index, value)| {
                        (
                            value.logical.clone(),
                            self.correction_ref(VRef { site: owner, index }, site),
                        )
                    })
            })
            .collect()
    }

    fn value_hint(&self, site: Site, name: &str, candidates: &[(String, String)]) -> String {
        let names = crate::suggestions::best_matches(name, candidates.iter().map(|(name, _)| name));
        let mut corrections: Vec<_> = candidates
            .iter()
            .filter(|(name, _)| names.contains(name))
            .map(|(_, correction)| correction.clone())
            .collect();
        corrections.sort();
        corrections.dedup();
        if !corrections.is_empty() {
            return crate::suggestions::format_matches(&corrections);
        }
        self.invisible_hint(site, name)
    }

    fn invisible_hint(&self, site: Site, name: &str) -> String {
        if !crate::suggestions::best_matches(
            name,
            self.visible_values(site).iter().map(|(name, _)| name),
        )
        .is_empty()
        {
            return String::new();
        }
        let chain = self.owner_chain(site);
        let invisible: Vec<_> = (0..self.model.owners.len())
            .map(Site::Owner)
            .chain((0..self.model.endpoints.len()).map(Site::Endpoint))
            .filter(|candidate| {
                *candidate != site
                    && match candidate {
                        Site::Owner(owner) => !chain.contains(owner),
                        Site::Endpoint(_) => true,
                    }
            })
            .collect();
        let names = crate::suggestions::best_matches(
            name,
            invisible
                .iter()
                .flat_map(|owner| self.site_values(*owner).iter().map(|value| &value.logical)),
        );
        let mut hint = String::new();
        for owner in invisible {
            for value in self.site_values(owner) {
                if names.contains(&value.logical) {
                    hint.push_str(&format!(
                        "; `{}` is declared by {}, which is not visible here",
                        value.logical,
                        self.site_label(owner)
                    ));
                }
            }
        }
        hint
    }

    /// Resolves one reference in the context of `site` (LANG-03).
    pub(crate) fn resolve_ref(&self, site: Site, reference: &Reference) -> syn::Result<VRef> {
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
                if Site::Owner(ancestor) == site
                    || (ancestor != 0
                        && matches!(
                            self.model.owners[ancestor].logical.as_str(),
                            "client" | "endpoint"
                        ))
                {
                    continue;
                }
                if let Some(found) = find(Site::Owner(ancestor)) {
                    suggestions.push(self.correction_ref(found, site));
                }
            }
            let hint = if suggestions.is_empty() {
                let spelling = self.value_hint(site, &name, &self.visible_values(site));
                format!(
                    "; declare `{name}` as a value of this owner, or qualify an ancestor value as `client.{name}` or `<scope>.{name}`{spelling}"
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
            let hint = crate::suggestions::name_suggestion(
                &name,
                self.site_values(site).iter().map(|value| &value.logical),
            );
            return Err(coded(
                reference.span,
                Code::Migrate,
                format!(
                    "`endpoint.{name}` is redundant qualification of the current endpoint ({}); write the bare name `{name}`{hint}",
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
                let eligible: Vec<_> = chain
                    .iter()
                    .copied()
                    .filter(|ancestor| Site::Owner(*ancestor) != site)
                    .filter(|ancestor| {
                        *ancestor == 0
                            || !matches!(
                                self.model.owners[*ancestor].logical.as_str(),
                                "client" | "endpoint"
                            )
                    })
                    .filter(|ancestor| {
                        !crate::suggestions::best_matches(
                            &name,
                            self.model.owners[*ancestor]
                                .values
                                .iter()
                                .map(|value| &value.logical),
                        )
                        .is_empty()
                    })
                    .map(|ancestor| {
                        (
                            if ancestor == 0 {
                                "client".to_owned()
                            } else {
                                self.model.owners[ancestor].logical.clone()
                            },
                            ancestor,
                        )
                    })
                    .collect();
                let qualifiers = crate::suggestions::best_matches(
                    &qualifier,
                    eligible.iter().map(|(qualifier, _)| qualifier),
                );
                let eligible: Vec<_> = eligible
                    .into_iter()
                    .filter(|(qualifier, _)| qualifiers.contains(qualifier))
                    .collect();
                let names = crate::suggestions::best_matches(
                    &name,
                    eligible.iter().flat_map(|(_, ancestor)| {
                        self.model.owners[*ancestor]
                            .values
                            .iter()
                            .map(|value| &value.logical)
                    }),
                );
                let mut corrections = Vec::new();
                for (_, ancestor) in eligible {
                    for (index, value) in self.model.owners[ancestor].values.iter().enumerate() {
                        if names.contains(&value.logical) {
                            corrections.push(self.correction_ref(
                                VRef {
                                    site: Site::Owner(ancestor),
                                    index,
                                },
                                site,
                            ));
                        }
                    }
                }
                corrections.sort();
                corrections.dedup();
                let hint = if corrections.is_empty() {
                    self.invisible_hint(site, &name)
                } else {
                    crate::suggestions::format_matches(&corrections)
                };
                return Err(coded(
                    owner.span(),
                    Code::Name,
                    format!(
                        "`{qualifier}` is not the client or a visible ancestor scope of {}; qualify with `client.` or the name of an enclosing scope{hint}",
                        self.site_label(site)
                    ),
                ));
            };
            target
        };
        if Site::Owner(target) == site {
            let hint = crate::suggestions::name_suggestion(
                &name,
                self.site_values(site).iter().map(|value| &value.logical),
            );
            return Err(coded(
                reference.span,
                Code::Migrate,
                format!(
                    "`{qualifier}.{name}` redundantly qualifies the current owner ({}); write the bare name `{name}`{hint}",
                    self.site_label(site)
                ),
            ));
        }
        find(Site::Owner(target)).ok_or_else(|| {
            let candidates: Vec<_> = self.model.owners[target].values.iter().enumerate().map(|(index, value)| {
                (value.logical.clone(), self.correction_ref(VRef { site: Site::Owner(target), index }, site))
            }).collect();
            let hint = self.value_hint(site, &name, &candidates);
            coded(
                reference.name.span(),
                Code::Name,
                format!(
                    "`{name}` is not a value of {} (referenced from {}); declare it there or qualify the owner that declares it{hint}",
                    self.model.owner_label(target),
                    self.site_label(site)
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
