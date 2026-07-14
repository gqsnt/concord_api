use super::*;

#[derive(Clone, Debug, Default)]
pub(crate) struct ProfileResolved {
    pub auth_uses: Vec<NormAuthUse>,
    pub rate_limit_specs: Vec<RateLimitSpec>,
}

pub(super) fn resolve_profiles(
    block: Option<&ProfilesBlock>,
    rate_limit_profiles: &BTreeMap<String, RateLimitPlanTemplate>,
) -> Result<BTreeMap<String, ProfileResolved>> {
    let Some(block) = block else {
        return Ok(BTreeMap::new());
    };

    let mut raw_profiles: BTreeMap<String, &ProfileDef> = BTreeMap::new();
    for profile in &block.profiles {
        let key = profile.name.to_string();
        if raw_profiles.insert(key.clone(), profile).is_some() {
            return Err(syn::Error::new(
                profile.name.span(),
                format!("duplicate profile `{key}`"),
            ));
        }
    }

    let mut resolved = BTreeMap::new();
    let mut visiting = std::collections::BTreeSet::new();
    for name in raw_profiles.keys().cloned().collect::<Vec<String>>() {
        resolve_profile(
            &name,
            &raw_profiles,
            &mut visiting,
            &mut resolved,
            rate_limit_profiles,
        )?;
    }

    Ok(resolved)
}

// The profile maps are threaded through recursive resolution uniformly; some
// maps are consumed only after recursion depending on the directive kind.
#[allow(clippy::only_used_in_recursion)]
pub(super) fn resolve_profile(
    name: &str,
    raw_profiles: &BTreeMap<String, &ProfileDef>,
    visiting: &mut std::collections::BTreeSet<String>,
    resolved: &mut BTreeMap<String, ProfileResolved>,
    rate_limit_profiles: &BTreeMap<String, RateLimitPlanTemplate>,
) -> Result<ProfileResolved> {
    if let Some(value) = resolved.get(name) {
        return Ok(value.clone());
    }

    let profile = raw_profiles
        .get(name)
        .ok_or_else(|| syn::Error::new(Span::call_site(), format!("unknown profile `{name}`")))?;

    if !visiting.insert(name.to_string()) {
        return Err(syn::Error::new(
            profile.name.span(),
            format!("profile inheritance cycle involving `{name}`"),
        ));
    }

    let mut out = if let Some(parent) = &profile.extends {
        if parent == &profile.name {
            return Err(syn::Error::new(
                parent.span(),
                format!("profile inheritance cycle involving `{name}`"),
            ));
        }
        if !raw_profiles.contains_key(&parent.to_string()) {
            return Err(syn::Error::new(
                parent.span(),
                format!("unknown profile parent `{parent}`"),
            ));
        }
        resolve_profile(
            &parent.to_string(),
            raw_profiles,
            visiting,
            resolved,
            rate_limit_profiles,
        )?
    } else {
        ProfileResolved::default()
    };

    let mut rate_limit_specs = Vec::new();
    if let Some(spec) = profile.patch.rate_limit.clone() {
        rate_limit_specs.push(spec);
    }

    let current = ProfileResolved {
        auth_uses: normalize_auth_uses(profile.patch.auth_uses.clone())?,
        rate_limit_specs,
    };

    out = merge_profile(out, current);
    visiting.remove(name);
    resolved.insert(name.to_string(), out.clone());
    Ok(out)
}

pub(super) fn resolve_profile_uses(
    uses: &[ProfileUseSpec],
    profiles: &BTreeMap<String, ProfileResolved>,
) -> Result<ProfileResolved> {
    let mut out = ProfileResolved::default();
    for use_spec in uses {
        for name in &use_spec.names {
            let Some(profile) = profiles.get(&name.to_string()) else {
                return Err(syn::Error::new(
                    name.span(),
                    format!("unknown profile `{name}`"),
                ));
            };
            out = merge_profile(out, profile.clone());
        }
    }
    Ok(out)
}

pub(crate) fn validate_profile_uses_unique_at_site(uses: &[ProfileUseSpec]) -> Result<()> {
    let mut seen = std::collections::BTreeSet::new();

    for use_spec in uses {
        for name in &use_spec.names {
            if !seen.insert(name.to_string()) {
                return Err(syn::Error::new(
                    name.span(),
                    format!("duplicate profile `{name}` at this attachment site"),
                ));
            }
        }
    }

    Ok(())
}

pub(crate) fn profile_use_names(uses: &[ProfileUseSpec]) -> Vec<String> {
    uses.iter()
        .flat_map(|use_spec| use_spec.names.iter())
        .map(ToString::to_string)
        .collect()
}

pub(super) fn resolve_profile_rate_limit_specs(
    specs: &[RateLimitSpec],
    rate_limit_profiles: &BTreeMap<String, RateLimitPlanTemplate>,
    visible_keys: &BTreeMap<String, RateLimitKeyBindingResolved>,
    endpoint_vars: Option<&BTreeMap<String, VarInfo>>,
    ctx: RateLimitAttachmentContext,
) -> Result<Option<RateLimitResolved>> {
    let mut out = None;

    for spec in specs {
        let resolved = resolve_rate_limit_spec(
            Some(spec),
            rate_limit_profiles,
            visible_keys,
            endpoint_vars,
            ctx,
        )?;
        out = merge_rate_limit_resolved(out, resolved);
    }

    Ok(out)
}

pub(super) fn merge_profile(
    mut parent: ProfileResolved,
    child: ProfileResolved,
) -> ProfileResolved {
    parent.auth_uses.extend(child.auth_uses);
    parent.rate_limit_specs.extend(child.rate_limit_specs);
    parent
}

pub(super) fn merge_rate_limit_resolved(
    existing: Option<RateLimitResolved>,
    incoming: Option<RateLimitResolved>,
) -> Option<RateLimitResolved> {
    let Some(incoming) = incoming else {
        return existing;
    };
    match (existing, incoming) {
        (None, incoming) => Some(incoming),
        (Some(RateLimitResolved::Clear), RateLimitResolved::Clear) => {
            Some(RateLimitResolved::Clear)
        }
        (Some(RateLimitResolved::Clear), RateLimitResolved::Add(plan)) => {
            Some(RateLimitResolved::Add(plan))
        }
        (Some(RateLimitResolved::Clear), RateLimitResolved::Replace(plan)) => {
            Some(RateLimitResolved::Replace(plan))
        }
        (Some(RateLimitResolved::Add(mut parent)), RateLimitResolved::Add(mut child)) => {
            parent.buckets.append(&mut child.buckets);
            Some(RateLimitResolved::Add(parent))
        }
        (Some(RateLimitResolved::Add(_)), RateLimitResolved::Replace(plan)) => {
            Some(RateLimitResolved::Replace(plan))
        }
        (Some(RateLimitResolved::Add(_)), RateLimitResolved::Clear) => {
            Some(RateLimitResolved::Clear)
        }
        (Some(RateLimitResolved::Replace(mut parent)), RateLimitResolved::Add(mut child)) => {
            parent.buckets.append(&mut child.buckets);
            Some(RateLimitResolved::Replace(parent))
        }
        (Some(RateLimitResolved::Replace(_)), RateLimitResolved::Replace(plan)) => {
            Some(RateLimitResolved::Replace(plan))
        }
        (Some(RateLimitResolved::Replace(_)), RateLimitResolved::Clear) => {
            Some(RateLimitResolved::Clear)
        }
    }
}
