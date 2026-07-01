fn resolve_rate_limit_profiles(
    block: Option<&RateLimitProfilesBlock>,
) -> Result<BTreeMap<String, RateLimitPlanTemplate>> {
    let Some(block) = block else {
        return Ok(BTreeMap::new());
    };

    resolve_profile_set(
        "rate_limit",
        block
            .profiles
            .iter()
            .map(|profile| {
                Ok((
                    profile.name.clone(),
                    profile.extends.clone(),
                    resolve_rate_limit_plan_spec(&profile.plan, &profile.name.to_string())?,
                ))
            })
            .collect::<Result<Vec<_>>>()?,
        block.default.clone(),
    )
}

fn resolve_client_rate_limit(
    block: Option<&RateLimitProfilesBlock>,
    profiles: &BTreeMap<String, RateLimitPlanTemplate>,
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
        RateLimitAttachmentContext::ClientBase,
    )?)))
}

fn resolve_rate_limit_spec(
    spec: Option<&RateLimitSpec>,
    profiles: &BTreeMap<String, RateLimitPlanTemplate>,
    visible_keys: &BTreeMap<String, RateLimitKeyBindingResolved>,
    endpoint_vars: Option<&BTreeMap<String, VarInfo>>,
    ctx: RateLimitAttachmentContext,
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
            let plan = materialize_rate_limit_plan(plan, visible_keys, endpoint_vars, ctx)?;
            if *only {
                Ok(Some(RateLimitResolved::Replace(plan)))
            } else {
                Ok(Some(RateLimitResolved::Add(plan)))
            }
        }
        RateLimitSpec::Inline { only, plan } => {
            let plan = resolve_rate_limit_plan_spec(plan, "inline")?;
            let plan = materialize_rate_limit_plan(plan, visible_keys, endpoint_vars, ctx)?;
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
    profiles: &BTreeMap<String, RateLimitPlanTemplate>,
) -> Result<RateLimitPlanTemplate> {
    let mut out = RateLimitPlanTemplate::default();
    for name in names {
        let Some(plan) = profiles.get(&name.to_string()) else {
            return Err(syn::Error::new(
                name.span(),
                unknown_name_message("rate_limit profile", name, profiles),
            ));
        };
        out.buckets.extend(plan.buckets.clone());
    }
    Ok(out)
}

fn resolve_rate_limit_plan_spec(
    plan: &RateLimitPlanSpec,
    default_bucket_name: &str,
) -> Result<RateLimitPlanTemplate> {
    const NANOS_PER_SECOND: u128 = 1_000_000_000;
    let mut out = RateLimitPlanTemplate::default();
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
            if cost > max {
                return Err(syn::Error::new(
                    bucket
                        .cost
                        .as_ref()
                        .map_or(window.max.span(), syn::LitInt::span),
                    "rate_limit bucket cost must not exceed the window max",
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
        out.buckets.push(RateLimitBucketTemplate {
            kind: bucket.kind.to_string(),
            name: format!("{default_bucket_name}_{idx}"),
            key: bucket.key.iter().map(resolve_rate_limit_key_spec).collect(),
            cost,
            windows,
        });
    }
    Ok(out)
}

impl ProfileValue for RateLimitPlanTemplate {
    fn empty() -> Self {
        Self::default()
    }

    fn merge(mut parent: Self, mut child: Self) -> Self {
        parent.buckets.append(&mut child.buckets);
        parent
    }

    fn validate(&self) -> Result<()> {
        Ok(())
    }
}

fn resolve_rate_limit_key_spec(spec: &RateLimitKeySpec) -> RateLimitKeyTemplate {
    match spec {
        RateLimitKeySpec::RouteHost => RateLimitKeyTemplate::RouteHost,
        RateLimitKeySpec::Endpoint => RateLimitKeyTemplate::Endpoint,
        RateLimitKeySpec::Method => RateLimitKeyTemplate::Method,
        RateLimitKeySpec::Named(name) => RateLimitKeyTemplate::Named {
            name: name.to_string(),
            span: name.span(),
        },
        RateLimitKeySpec::Static(value) => RateLimitKeyTemplate::Static {
            name: "static".to_string(),
            value: value.value(),
        },
    }
}

fn materialize_rate_limit_plan(
    mut plan: RateLimitPlanTemplate,
    visible_keys: &BTreeMap<String, RateLimitKeyBindingResolved>,
    endpoint_vars: Option<&BTreeMap<String, VarInfo>>,
    ctx: RateLimitAttachmentContext,
) -> Result<RateLimitPlanResolved> {
    let mut out = RateLimitPlanResolved::default();
    for bucket in plan.buckets.drain(..) {
        out.buckets.push(RateLimitBucketResolved {
            kind: bucket.kind,
            name: bucket.name,
            key: bucket
                .key
                .iter()
                .map(|key| {
                    materialize_rate_limit_key(
                        key,
                        visible_keys,
                        endpoint_vars,
                        ctx,
                    )
                })
                .collect::<Result<Vec<_>>>()?,
            cost: bucket.cost,
            windows: bucket.windows,
        });
    }
    Ok(out)
}

fn materialize_rate_limit_key(
    key: &RateLimitKeyTemplate,
    visible_keys: &BTreeMap<String, RateLimitKeyBindingResolved>,
    endpoint_vars: Option<&BTreeMap<String, VarInfo>>,
    ctx: RateLimitAttachmentContext,
) -> Result<RateLimitKeyResolved> {
    match key {
        RateLimitKeyTemplate::RouteHost => Ok(RateLimitKeyResolved::RouteHost),
        RateLimitKeyTemplate::Endpoint => Ok(RateLimitKeyResolved::Endpoint),
        RateLimitKeyTemplate::Method => Ok(RateLimitKeyResolved::Method),
        RateLimitKeyTemplate::Static { name, value } => Ok(RateLimitKeyResolved::Static {
            name: name.clone(),
            value: value.clone(),
        }),
        RateLimitKeyTemplate::Named { name, span } => {
            if let Some(binding) = visible_keys.get(name) {
                return Ok(RateLimitKeyResolved::EpField {
                    name: name.clone(),
                    field: binding.field.clone(),
                });
            }
            if matches!(ctx, RateLimitAttachmentContext::ClientBase) {
                return Err(syn::Error::new(
                    *span,
                    "endpoint/scope rate_limit key cannot be used in client base policy",
                ));
            }
            let Some(vars) = endpoint_vars else {
                return Err(syn::Error::new(
                    *span,
                    unknown_name_message_from_keys(
                        "rate_limit key",
                        name,
                        visible_keys.keys().cloned(),
                    ),
                ));
            };
            let Some(var) = vars.get(name) else {
                let available = visible_keys
                    .keys()
                    .cloned()
                    .chain(vars.keys().cloned());
                return Err(syn::Error::new(
                    *span,
                    unknown_name_message_from_keys("rate_limit key", name, available),
                ));
            };
            if var.optional {
                return Err(syn::Error::new(
                    var.rust.span(),
                    format!("rate_limit key `{name}` cannot use optional param"),
                ));
            }
            Ok(RateLimitKeyResolved::EpField {
                name: name.clone(),
                field: var.rust.clone(),
            })
        }
    }
}

fn resolve_rate_limit_key_bindings(
    bindings: &[RateLimitKeyBindingSpec],
    decls: &[VarInfo],
) -> Result<Vec<RateLimitKeyBindingResolved>> {
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for binding in bindings {
        let name = binding.name.to_string();
        if !seen.insert(name.clone()) {
            return Err(syn::Error::new(
                binding.name.span(),
                format!("duplicate rate_limit key `{name}`"),
            ));
        }
        let Some(decl) = decls.iter().find(|d| d.rust == binding.value) else {
            return Err(syn::Error::new(
                binding.value.span(),
                format!("unknown rate_limit key binding `{}`", binding.value),
            ));
        };
        if decl.optional {
            return Err(syn::Error::new(
                binding.value.span(),
                format!(
                    "rate_limit key binding `{name}` cannot reference optional parameter `{}`",
                    binding.value
                ),
            ));
        }
        out.push(RateLimitKeyBindingResolved {
            name,
            field: decl.rust.clone(),
        });
    }
    Ok(out)
}

