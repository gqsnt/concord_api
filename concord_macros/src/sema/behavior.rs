#[derive(Clone, Debug, Default)]
pub(crate) struct BehaviorResolved {
    pub auth_uses: Vec<NormAuthUse>,
    pub retry: Option<RetryDirectiveResolved>,
    pub rate_limit_specs: Vec<RateLimitSpec>,
}

fn resolve_behavior_profiles(
    block: Option<&BehaviorProfilesBlock>,
    retry_profiles: &BTreeMap<String, RetryConfigResolved>,
) -> Result<BTreeMap<String, BehaviorResolved>> {
    let Some(block) = block else {
        return Ok(BTreeMap::new());
    };

    let mut raw_profiles: BTreeMap<String, &BehaviorProfileDef> = BTreeMap::new();
    for profile in &block.profiles {
        let key = profile.name.to_string();
        if raw_profiles.insert(key.clone(), profile).is_some() {
            return Err(syn::Error::new(
                profile.name.span(),
                format!("duplicate behavior `{key}`"),
            ));
        }
    }

    let mut resolved = BTreeMap::new();
    let mut visiting = std::collections::BTreeSet::new();
    for name in raw_profiles.keys().cloned().collect::<Vec<String>>() {
        resolve_behavior_profile(
            &name,
            &raw_profiles,
            &mut visiting,
            &mut resolved,
            retry_profiles,
        )?;
    }

    Ok(resolved)
}

fn resolve_behavior_profile(
    name: &str,
    raw_profiles: &BTreeMap<String, &BehaviorProfileDef>,
    visiting: &mut std::collections::BTreeSet<String>,
    resolved: &mut BTreeMap<String, BehaviorResolved>,
    retry_profiles: &BTreeMap<String, RetryConfigResolved>,
) -> Result<BehaviorResolved> {
    if let Some(value) = resolved.get(name) {
        return Ok(value.clone());
    }

    let profile = raw_profiles.get(name).ok_or_else(|| {
        syn::Error::new(
            Span::call_site(),
            format!("unknown behavior `{name}`"),
        )
    })?;

    if !visiting.insert(name.to_string()) {
        return Err(syn::Error::new(
            profile.name.span(),
            format!("behavior inheritance cycle involving `{name}`"),
        ));
    }

    let mut out = if let Some(parent) = &profile.extends {
        if parent == &profile.name {
            return Err(syn::Error::new(
                parent.span(),
                format!("behavior inheritance cycle involving `{name}`"),
            ));
        }
        if !raw_profiles.contains_key(&parent.to_string()) {
            return Err(syn::Error::new(
                parent.span(),
                format!("unknown behavior parent `{parent}`"),
            ));
        }
        resolve_behavior_profile(
            &parent.to_string(),
            raw_profiles,
            visiting,
            resolved,
            retry_profiles,
        )?
    } else {
        BehaviorResolved::default()
    };

    let mut rate_limit_specs = Vec::new();
    if let Some(spec) = profile.patch.rate_limit.clone() {
        rate_limit_specs.push(spec);
    }

    let current = BehaviorResolved {
        auth_uses: normalize_auth_uses(profile.patch.auth_uses.clone())?,
        retry: resolve_retry_spec(profile.patch.retry.as_ref(), retry_profiles)?,
        rate_limit_specs,
    };

    out = merge_behavior(out, current);
    visiting.remove(name);
    resolved.insert(name.to_string(), out.clone());
    Ok(out)
}

fn resolve_behavior_uses(
    uses: &[BehaviorUseSpec],
    profiles: &BTreeMap<String, BehaviorResolved>,
) -> Result<BehaviorResolved> {
    let mut out = BehaviorResolved::default();
    for use_spec in uses {
        for name in &use_spec.names {
            let Some(profile) = profiles.get(&name.to_string()) else {
                return Err(syn::Error::new(
                    name.span(),
                    format!("unknown behavior `{name}`"),
                ));
            };
            out = merge_behavior(out, profile.clone());
        }
    }
    Ok(out)
}

pub(crate) fn validate_behavior_uses_unique_at_site(uses: &[BehaviorUseSpec]) -> Result<()> {
    let mut seen = std::collections::BTreeSet::new();

    for use_spec in uses {
        for name in &use_spec.names {
            if !seen.insert(name.to_string()) {
                return Err(syn::Error::new(
                    name.span(),
                    format!("duplicate behavior `{name}` at this attachment site"),
                ));
            }
        }
    }

    Ok(())
}

pub(crate) fn behavior_use_names(uses: &[BehaviorUseSpec]) -> Vec<String> {
    uses.iter()
        .flat_map(|use_spec| use_spec.names.iter())
        .map(ToString::to_string)
        .collect()
}

fn resolve_behavior_rate_limit_specs(
    specs: &[RateLimitSpec],
    rate_limit_profiles: &BTreeMap<String, RateLimitPlanResolved>,
    visible_keys: &BTreeMap<String, RateLimitKeyBindingResolved>,
    endpoint_vars: Option<&BTreeMap<String, VarInfo>>,
) -> Result<Option<RateLimitResolved>> {
    let mut out = None;

    for spec in specs {
        let resolved = resolve_rate_limit_spec(
            Some(spec),
            rate_limit_profiles,
            visible_keys,
            endpoint_vars,
        )?;
        out = merge_rate_limit_resolved(out, resolved);
    }

    Ok(out)
}

fn merge_behavior(mut parent: BehaviorResolved, child: BehaviorResolved) -> BehaviorResolved {
    parent.auth_uses.extend(child.auth_uses);
    if child.retry.is_some() {
        parent.retry = child.retry;
    }
    parent.rate_limit_specs.extend(child.rate_limit_specs);
    parent
}

fn merge_rate_limit_resolved(
    existing: Option<RateLimitResolved>,
    incoming: Option<RateLimitResolved>,
) -> Option<RateLimitResolved> {
    let Some(incoming) = incoming else {
        return existing;
    };
    match (existing, incoming) {
        (None, incoming) => Some(incoming),
        (Some(RateLimitResolved::Clear), RateLimitResolved::Clear) => Some(RateLimitResolved::Clear),
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
        (Some(RateLimitResolved::Add(_)), RateLimitResolved::Clear) => Some(RateLimitResolved::Clear),
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
