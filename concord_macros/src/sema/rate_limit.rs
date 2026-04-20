fn resolve_rate_limit_profiles(
    block: Option<&RateLimitProfilesBlock>,
) -> Result<BTreeMap<String, RateLimitPlanResolved>> {
    let Some(block) = block else {
        return Ok(BTreeMap::new());
    };

    let mut raw: BTreeMap<String, &RateLimitProfileDef> = BTreeMap::new();
    for profile in &block.profiles {
        let key = profile.name.to_string();
        if raw.insert(key.clone(), profile).is_some() {
            return Err(syn::Error::new(
                profile.name.span(),
                format!("duplicate rate_limit profile `{key}`"),
            ));
        }
    }

    let mut resolved = BTreeMap::new();
    let mut stack = Vec::new();
    for profile in &block.profiles {
        resolve_rate_limit_profile(&profile.name, &raw, &mut resolved, &mut stack)?;
    }
    for default in &block.default {
        if !resolved.contains_key(&default.to_string()) {
            return Err(syn::Error::new(
                default.span(),
                format!("unknown default rate_limit profile `{default}`"),
            ));
        }
    }

    Ok(resolved)
}

fn resolve_rate_limit_profile(
    name: &Ident,
    raw: &BTreeMap<String, &RateLimitProfileDef>,
    resolved: &mut BTreeMap<String, RateLimitPlanResolved>,
    stack: &mut Vec<String>,
) -> Result<RateLimitPlanResolved> {
    let key = name.to_string();
    if let Some(plan) = resolved.get(&key) {
        return Ok(plan.clone());
    }
    if stack.iter().any(|item| item == &key) {
        return Err(syn::Error::new(
            name.span(),
            format!("rate_limit profile inheritance cycle involving `{key}`"),
        ));
    }

    let Some(profile) = raw.get(&key) else {
        return Err(syn::Error::new(
            name.span(),
            format!("unknown rate_limit profile `{key}`"),
        ));
    };

    stack.push(key.clone());
    let mut plan = if let Some(base) = &profile.extends {
        resolve_rate_limit_profile(base, raw, resolved, stack)?
    } else {
        RateLimitPlanResolved::default()
    };
    let mut own = resolve_rate_limit_plan_spec(&profile.plan, &key)?;
    plan.buckets.append(&mut own.buckets);
    stack.pop();

    resolved.insert(key, plan.clone());
    Ok(plan)
}

fn resolve_client_rate_limit(
    block: Option<&RateLimitProfilesBlock>,
    profiles: &BTreeMap<String, RateLimitPlanResolved>,
    visible_keys: &BTreeMap<String, RateLimitKeyBindingResolved>,
    endpoint_vars: Option<&BTreeMap<String, VarInfo>>,
) -> Result<Option<RateLimitResolved>> {
    let Some(block) = block else {
        return Ok(None);
    };
    if block.default.is_empty() {
        return Ok(None);
    }
    let plan = combine_rate_limit_profiles(&block.default, profiles)?;
    Ok(Some(RateLimitResolved::Add(materialize_rate_limit_plan(
        plan,
        visible_keys,
        endpoint_vars,
    )?)))
}

fn resolve_rate_limit_spec(
    spec: Option<&RateLimitSpec>,
    profiles: &BTreeMap<String, RateLimitPlanResolved>,
    visible_keys: &BTreeMap<String, RateLimitKeyBindingResolved>,
    endpoint_vars: Option<&BTreeMap<String, VarInfo>>,
) -> Result<Option<RateLimitResolved>> {
    let Some(spec) = spec else {
        return Ok(None);
    };
    match spec {
        RateLimitSpec::Off => Ok(Some(RateLimitResolved::Clear)),
        RateLimitSpec::Profiles {
            only,
            profiles: names,
        } => {
            let plan = combine_rate_limit_profiles(names, profiles)?;
            let plan = materialize_rate_limit_plan(plan, visible_keys, endpoint_vars)?;
            if *only {
                Ok(Some(RateLimitResolved::Replace(plan)))
            } else {
                Ok(Some(RateLimitResolved::Add(plan)))
            }
        }
        RateLimitSpec::Inline { only, plan } => {
            let plan = resolve_rate_limit_plan_spec(plan, "inline")?;
            let plan = materialize_rate_limit_plan(plan, visible_keys, endpoint_vars)?;
            if *only {
                Ok(Some(RateLimitResolved::Replace(plan)))
            } else {
                Ok(Some(RateLimitResolved::Add(plan)))
            }
        }
    }
}

fn combine_rate_limit_profiles(
    names: &[Ident],
    profiles: &BTreeMap<String, RateLimitPlanResolved>,
) -> Result<RateLimitPlanResolved> {
    let mut out = RateLimitPlanResolved::default();
    for name in names {
        let Some(plan) = profiles.get(&name.to_string()) else {
            return Err(syn::Error::new(
                name.span(),
                format!("unknown rate_limit profile `{name}`"),
            ));
        };
        out.buckets.extend(plan.buckets.clone());
    }
    Ok(out)
}

fn resolve_rate_limit_plan_spec(
    plan: &RateLimitPlanSpec,
    default_bucket_name: &str,
) -> Result<RateLimitPlanResolved> {
    const NANOS_PER_SECOND: u128 = 1_000_000_000;
    let mut out = RateLimitPlanResolved::default();
    for (idx, bucket) in plan.buckets.iter().enumerate() {
        if bucket.windows.is_empty() {
            return Err(syn::Error::new(
                bucket.kind.span(),
                "rate_limit bucket must contain at least one `limit`",
            ));
        }
        let cost = if let Some(cost_lit) = &bucket.cost {
            let cost = cost_lit.base10_parse::<u32>()?;
            if cost == 0 {
                return Err(syn::Error::new(
                    cost_lit.span(),
                    "rate_limit bucket cost must be greater than zero",
                ));
            }
            cost
        } else {
            1
        };
        let mut windows = Vec::new();
        for window in &bucket.windows {
            let max = window.max.base10_parse::<u32>()?;
            if max == 0 {
                return Err(syn::Error::new(
                    window.max.span(),
                    "rate_limit max must be greater than zero",
                ));
            }
            let amount = window.every.base10_parse::<u64>()?;
            if amount == 0 {
                return Err(syn::Error::new(
                    window.every.span(),
                    "rate_limit duration must be greater than zero",
                ));
            }
            let multiplier = match window.unit {
                RateLimitDurationUnit::Seconds => 1,
                RateLimitDurationUnit::Minutes => 60,
            };
            let per_secs = amount.checked_mul(multiplier).ok_or_else(|| {
                syn::Error::new(window.every.span(), "rate_limit duration is too large")
            })?;
            let per_nanos = (per_secs as u128)
                .checked_mul(NANOS_PER_SECOND)
                .ok_or_else(|| {
                    syn::Error::new(window.every.span(), "rate_limit duration is too large")
                })?;
            if max as u128 > per_nanos {
                return Err(syn::Error::new(
                    window.max.span(),
                    "rate_limit window is too small for max; reduce `limit` or increase `every`",
                ));
            }
            windows.push(RateLimitWindowResolved { max, per_secs });
        }
        out.buckets.push(RateLimitBucketResolved {
            kind: bucket.kind.to_string(),
            name: format!("{default_bucket_name}_{idx}"),
            key: bucket.key.iter().map(resolve_rate_limit_key_spec).collect(),
            cost,
            windows,
        });
    }
    Ok(out)
}

fn resolve_rate_limit_key_spec(spec: &RateLimitKeySpec) -> RateLimitKeyResolved {
    match spec {
        RateLimitKeySpec::RouteHost => RateLimitKeyResolved::RouteHost,
        RateLimitKeySpec::Endpoint => RateLimitKeyResolved::Endpoint,
        RateLimitKeySpec::Method => RateLimitKeyResolved::Method,
        RateLimitKeySpec::Named(name) => RateLimitKeyResolved::Named {
            name: name.to_string(),
            span: name.span(),
        },
        RateLimitKeySpec::Static(value) => RateLimitKeyResolved::Static {
            name: "static".to_string(),
            value: value.value(),
        },
    }
}

fn materialize_rate_limit_plan(
    mut plan: RateLimitPlanResolved,
    visible_keys: &BTreeMap<String, RateLimitKeyBindingResolved>,
    endpoint_vars: Option<&BTreeMap<String, VarInfo>>,
) -> Result<RateLimitPlanResolved> {
    for bucket in &mut plan.buckets {
        for key in &mut bucket.key {
            let RateLimitKeyResolved::Named { name, span } = key else {
                continue;
            };
            if let Some(binding) = visible_keys.get(name) {
                *key = RateLimitKeyResolved::EpField {
                    name: name.clone(),
                    field: binding.field.clone(),
                };
                continue;
            }
            let Some(vars) = endpoint_vars else {
                return Err(syn::Error::new(
                    *span,
                    format!("rate_limit key `{name}` requires endpoint/scope params"),
                ));
            };
            let Some(var) = vars.get(name) else {
                return Err(syn::Error::new(
                    *span,
                    format!("unknown rate_limit key `{name}`"),
                ));
            };
            if var.optional {
                return Err(syn::Error::new(
                    var.rust.span(),
                    format!("rate_limit key `{name}` cannot use optional param"),
                ));
            }
            *key = RateLimitKeyResolved::EpField {
                name: name.clone(),
                field: var.rust.clone(),
            };
        }
    }
    Ok(plan)
}

fn resolve_rate_limit_key_bindings(
    bindings: &[RateLimitKeyBindingSpec],
    decls: &[VarInfo],
) -> Result<Vec<RateLimitKeyBindingResolved>> {
    let decl_map: BTreeMap<String, &VarInfo> = decls
        .iter()
        .map(|decl| (decl.rust.to_string(), decl))
        .collect();
    let mut seen = BTreeMap::new();
    let mut out = Vec::new();
    for binding in bindings {
        let name = binding.name.to_string();
        if seen.insert(name.clone(), binding.name.span()).is_some() {
            return Err(syn::Error::new(
                binding.name.span(),
                format!("duplicate rate_limit key `{name}`"),
            ));
        }
        let Some(target) = decl_map.get(&binding.value.to_string()) else {
            return Err(syn::Error::new(
                binding.value.span(),
                format!(
                    "unknown scope param `{}` in rate_limit key binding",
                    binding.value
                ),
            ));
        };
        if target.optional {
            return Err(syn::Error::new(
                binding.value.span(),
                "rate_limit key binding cannot target an optional param",
            ));
        }
        out.push(RateLimitKeyBindingResolved {
            name,
            field: binding.value.clone(),
        });
    }
    Ok(out)
}

fn rate_limit_key_bindings_for_ancestry(
    ancestry: &[usize],
    layers: &[LayerIr],
) -> BTreeMap<String, RateLimitKeyBindingResolved> {
    let mut out = BTreeMap::new();
    for &lid in ancestry {
        for binding in &layers[lid].rate_limit_keys {
            out.insert(binding.name.clone(), binding.clone());
        }
    }
    out
}

