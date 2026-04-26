fn emit_cache_op(cache: &Option<CacheResolved>) -> Option<TokenStream2> {
    let cache = cache.as_ref()?;
    Some(match cache {
        CacheResolved::Clear => quote! {
            policy.clear_cache();
        },
        CacheResolved::Set(config) => {
            let config = emit_cache_config(config);
            quote! {
                policy.set_cache(#config);
            }
        }
        CacheResolved::Patch(patch) => {
            let ops = emit_cache_patch_ops(patch);
            quote! {
                let mut __cache = policy.cache().cloned().unwrap_or_default();
                #( #ops )*
                policy.set_cache(__cache);
            }
        }
    })
}

fn emit_cache_config(config: &CacheConfigResolved) -> TokenStream2 {
    let mut ops = Vec::new();
    if config.http {
        ops.push(quote! {
            __cache = __cache.with_http();
        });
    }
    if let Some(ttl_secs) = config.default_ttl_secs {
        ops.push(quote! {
            __cache = __cache.with_default_ttl(::std::time::Duration::from_secs(#ttl_secs));
        });
    }
    if let Some(revalidate) = config.revalidate {
        ops.push(quote! {
            __cache = __cache.with_revalidate(#revalidate);
        });
    }
    if let Some(failure_mode) = config.failure_mode {
        let failure_mode = emit_cache_failure_mode(failure_mode);
        ops.push(quote! {
            __cache = __cache.with_failure_mode(#failure_mode);
        });
    }
    quote! {{
        let mut __cache = ::concord_core::prelude::CacheConfig::new();
        #( #ops )*
        __cache
    }}
}

fn emit_cache_patch_ops(patch: &CacheConfigPatchResolved) -> Vec<TokenStream2> {
    let mut ops = Vec::new();
    if patch.http == Some(true) {
        ops.push(quote! {
            __cache = __cache.with_http();
        });
    }
    if let Some(ttl_secs) = patch.default_ttl_secs {
        ops.push(quote! {
            __cache = __cache.with_default_ttl(::std::time::Duration::from_secs(#ttl_secs));
        });
    }
    if let Some(revalidate) = patch.revalidate {
        ops.push(quote! {
            __cache = __cache.with_revalidate(#revalidate);
        });
    }
    if let Some(failure_mode) = patch.failure_mode {
        let failure_mode = emit_cache_failure_mode(failure_mode);
        ops.push(quote! {
            __cache = __cache.with_failure_mode(#failure_mode);
        });
    }
    ops
}

fn emit_cache_failure_mode(mode: CacheFailureModeResolved) -> TokenStream2 {
    match mode {
        CacheFailureModeResolved::Ignore => {
            quote! { ::concord_core::prelude::CacheFailureMode::Ignore }
        }
        CacheFailureModeResolved::ServeStaleOnError => {
            quote! { ::concord_core::prelude::CacheFailureMode::ServeStaleOnError }
        }
    }
}

