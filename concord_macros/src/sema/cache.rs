fn resolve_cache_profiles(
    block: Option<&CacheProfilesBlock>,
) -> Result<BTreeMap<String, CacheConfigResolved>> {
    let Some(block) = block else {
        return Ok(BTreeMap::new());
    };

    let defaults = block.default.iter().cloned().collect::<Vec<_>>();
    let patches = resolve_profile_set(
        "cache",
        block
            .profiles
            .iter()
            .map(|profile| {
                Ok((
                    profile.name.clone(),
                    profile.extends.clone(),
                    resolve_cache_patch(&profile.patch)?,
                ))
            })
            .collect::<Result<Vec<_>>>()?,
        defaults,
    )?;

    Ok(patches
        .into_iter()
        .map(|(name, patch)| (name, cache_config_from_patch(&patch)))
        .collect())
}

fn resolve_client_cache(
    spec: Option<&CacheSpec>,
    default: Option<&Ident>,
    profiles: &BTreeMap<String, CacheConfigResolved>,
) -> Result<Option<CacheResolved>> {
    if let Some(spec) = spec {
        return resolve_cache_spec(Some(spec), profiles).map(|resolved| {
            resolved.map(|cache| match cache {
                CacheResolved::Patch(patch) => CacheResolved::Set(cache_config_from_patch(&patch)),
                other => other,
            })
        });
    }
    let Some(default) = default else {
        return Ok(None);
    };
    let Some(config) = profiles.get(&default.to_string()) else {
        return Err(syn::Error::new(
            default.span(),
            format!("unknown default cache profile `{default}`"),
        ));
    };
    Ok(Some(CacheResolved::Set(config.clone())))
}

fn resolve_cache_spec(
    spec: Option<&CacheSpec>,
    profiles: &BTreeMap<String, CacheConfigResolved>,
) -> Result<Option<CacheResolved>> {
    let Some(spec) = spec else {
        return Ok(None);
    };
    match spec {
        CacheSpec::Off => Ok(Some(CacheResolved::Clear)),
        CacheSpec::Profile { only, profile } => {
            let _ = only;
            let Some(config) = profiles.get(&profile.to_string()) else {
                return Err(syn::Error::new(
                    profile.span(),
                    format!("unknown cache profile `{profile}`"),
                ));
            };
            Ok(Some(CacheResolved::Set(config.clone())))
        }
        CacheSpec::Patch { only, patch } => {
            let patch = resolve_cache_patch(patch)?;
            if *only {
                Ok(Some(CacheResolved::Set(cache_config_from_patch(&patch))))
            } else {
                Ok(Some(CacheResolved::Patch(patch)))
            }
        }
    }
}

fn resolve_cache_patch(patch: &CachePatch) -> Result<CacheConfigPatchResolved> {
    let mut out = CacheConfigPatchResolved::default();
    if patch.http.is_some() {
        out.http = Some(true);
    }
    if let Some(ttl) = &patch.ttl {
        out.default_ttl_secs = Some(resolve_cache_duration_secs(ttl)?);
    }
    if let Some(capacity) = &patch.capacity {
        out.capacity = Some(resolve_cache_capacity(capacity)?);
    }
    if let Some(max_body) = &patch.max_body {
        out.max_body_bytes = Some(resolve_cache_size_bytes(max_body)?);
    }
    if let Some(revalidate) = &patch.revalidate {
        out.revalidate = Some(revalidate.value);
    }
    if let Some(shared) = &patch.shared {
        out.shared = Some(shared.value);
    }
    if let Some(on_error) = patch.on_error {
        out.failure_mode = Some(match on_error {
            CacheOnErrorSpec::Ignore => CacheFailureModeResolved::Ignore,
            CacheOnErrorSpec::ServeStale => CacheFailureModeResolved::ServeStaleOnError,
        });
    }
    Ok(out)
}

fn apply_cache_patch_resolved(config: &mut CacheConfigResolved, patch: &CacheConfigPatchResolved) {
    if let Some(http) = patch.http {
        config.http = http;
    }
    if let Some(ttl) = patch.default_ttl_secs {
        config.default_ttl_secs = Some(ttl);
    }
    if let Some(capacity) = patch.capacity {
        config.capacity = Some(capacity);
    }
    if let Some(max_body_bytes) = patch.max_body_bytes {
        config.max_body_bytes = Some(max_body_bytes);
    }
    if let Some(revalidate) = patch.revalidate {
        config.revalidate = Some(revalidate);
    }
    if let Some(shared) = patch.shared {
        config.shared = Some(shared);
    }
    if let Some(failure_mode) = patch.failure_mode {
        config.failure_mode = Some(failure_mode);
    }
}

impl ProfileValue for CacheConfigPatchResolved {
    fn empty() -> Self {
        Self::default()
    }

    fn merge(mut parent: Self, child: Self) -> Self {
        if child.http.is_some() {
            parent.http = child.http;
        }
        if child.default_ttl_secs.is_some() {
            parent.default_ttl_secs = child.default_ttl_secs;
        }
        if child.capacity.is_some() {
            parent.capacity = child.capacity;
        }
        if child.max_body_bytes.is_some() {
            parent.max_body_bytes = child.max_body_bytes;
        }
        if child.revalidate.is_some() {
            parent.revalidate = child.revalidate;
        }
        if child.shared.is_some() {
            parent.shared = child.shared;
        }
        if child.failure_mode.is_some() {
            parent.failure_mode = child.failure_mode;
        }
        parent
    }

    fn validate(&self) -> Result<()> {
        Ok(())
    }
}

fn cache_config_from_patch(patch: &CacheConfigPatchResolved) -> CacheConfigResolved {
    let mut config = CacheConfigResolved::default();
    apply_cache_patch_resolved(&mut config, patch);
    config
}

fn resolve_cache_duration_secs(ttl: &CacheDurationSpec) -> Result<u64> {
    let amount = ttl.amount.base10_parse::<u64>()?;
    if amount == 0 {
        return Err(syn::Error::new(
            ttl.amount.span(),
            "cache ttl must be greater than zero",
        ));
    }
    let multiplier = match ttl.unit {
        RateLimitDurationUnit::Seconds => 1,
        RateLimitDurationUnit::Minutes => 60,
    };
    Ok(amount.saturating_mul(multiplier))
}

fn resolve_cache_capacity(capacity: &CacheCapacitySpec) -> Result<CacheCapacityResolved> {
    match capacity {
        CacheCapacitySpec::Entries { amount } => {
            let entries = amount.base10_parse::<u64>()?;
            if entries == 0 {
                return Err(syn::Error::new(
                    amount.span(),
                    "cache capacity entries must be greater than zero",
                ));
            }
            Ok(CacheCapacityResolved::Entries(entries))
        }
        CacheCapacitySpec::Bytes(size) => Ok(CacheCapacityResolved::Bytes(
            resolve_cache_size_bytes(size)?,
        )),
    }
}

fn resolve_cache_size_bytes(size: &CacheSizeSpec) -> Result<u64> {
    let amount = size.amount.base10_parse::<u64>()?;
    if amount == 0 {
        return Err(syn::Error::new(
            size.amount.span(),
            "cache size must be greater than zero",
        ));
    }
    let multiplier = match size.unit {
        CacheSizeUnit::Bytes => 1,
        CacheSizeUnit::KiB => 1024,
        CacheSizeUnit::MiB => 1024 * 1024,
        CacheSizeUnit::GiB => 1024 * 1024 * 1024,
    };
    amount
        .checked_mul(multiplier)
        .ok_or_else(|| syn::Error::new(size.amount.span(), "cache size is too large to represent"))
}

