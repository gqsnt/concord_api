use std::collections::BTreeMap;

fn endpoint_internal_ident(ep: &ResolvedEndpoint) -> Ident {
    let mut name = String::from("Ep");
    for scope in &ep.scope_modules {
        name.push_str(&pascalize(&scope.to_string()));
    }
    name.push_str(&pascalize(&ep.name.to_string()));
    name.push('H');
    name.push_str(&stable_endpoint_hash(&endpoint_qualified_name(ep)));
    emit_helpers::ident(&name, ep.name.span())
}

fn endpoint_pending_ext_trait_ident(ep: &ResolvedEndpoint) -> Ident {
    emit_helpers::ident(
        &crate::model::facade::generated_endpoint_request_ext_trait_type_name(ep),
        ep.name.span(),
    )
}

fn pascalize(raw: &str) -> String {
    raw.split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            let Some(first) = chars.next() else {
                return String::new();
            };
            let mut out = String::new();
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
            out
        })
        .collect::<String>()
}

fn stable_endpoint_hash(value: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn endpoint_qualified_name(ep: &ResolvedEndpoint) -> String {
    if ep.scope_modules.is_empty() {
        ep.name.to_string()
    } else {
        let mut qualified = ep
            .scope_modules
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("::");
        qualified.push_str("::");
        qualified.push_str(&ep.name.to_string());
        qualified
    }
}

fn emit_endpoints(resolved_api: &ResolvedApi, facade_ir: &FacadeIr, cx_ty: &Ident) -> TokenStream2 {
    let endpoint_defs = resolved_api.endpoints.iter().map(|ep| {
        let internal = endpoint_internal_ident(ep);
        let Some(facade) = facade_ir_for_endpoint(facade_ir, ep) else {
            return emit_helpers::compile_error_tokens(
                "FacadeIr must contain one public endpoint entry per resolved endpoint",
                ep.name.span(),
            );
        };
        emit_endpoint_def(resolved_api, facade, ep, &internal, cx_ty)
    });
    let root_endpoint_reexports = resolved_api.endpoints.iter().filter_map(|ep| {
        if !ep.scope_modules.is_empty() {
            return None;
        }
        let public = &ep.name;
        let internal = endpoint_internal_ident(ep);
        Some(quote! { pub use super::__endpoints::#internal as #public; })
    });
    let pending_ext_reexports = resolved_api
        .endpoints
        .iter()
        .filter_map(|ep| {
            let facade = facade_ir_for_endpoint(facade_ir, ep)?;
            if facade.setters.is_empty() {
                return None;
            }
            let ext = endpoint_pending_ext_trait_ident(ep);
            Some(quote! { pub use __endpoints::#ext; })
        });
    let scope_modules = emit_endpoint_scope_modules(resolved_api);
    quote! {
        mod __endpoints {
            use super::*;
            #( #endpoint_defs )*
        }

        pub mod endpoints {
            #( #root_endpoint_reexports )*
            #scope_modules
        }

        #( #pending_ext_reexports )*
    }
}

#[derive(Clone)]
struct EndpointScopeAlias {
    public: Ident,
    internal: Ident,
}

struct EndpointScopeModule {
    name: Ident,
    endpoints: Vec<EndpointScopeAlias>,
    children: Vec<EndpointScopeModule>,
    child_index: BTreeMap<String, usize>,
}

fn insert_endpoint_scope_module(
    modules: &mut Vec<EndpointScopeModule>,
    index: &mut BTreeMap<String, usize>,
    path: &[Ident],
    public: &Ident,
    internal: &Ident,
) -> syn::Result<()> {
    if path.len() > crate::limits::MAX_DSL_SCOPE_DEPTH {
        return Err(syn::Error::new(
            public.span(),
            format!(
                "DSL scope nesting exceeds maximum supported depth of {}",
                crate::limits::MAX_DSL_SCOPE_DEPTH
            ),
        ));
    }
    let Some((head, tail)) = path.split_first() else {
        return Ok(());
    };

    let module_index = if let Some(existing) = index.get(&head.to_string()).copied() {
        existing
    } else {
        modules.push(EndpointScopeModule {
            name: head.clone(),
            endpoints: Vec::new(),
            children: Vec::new(),
            child_index: BTreeMap::new(),
        });
        let next_index = modules.len() - 1;
        index.insert(head.to_string(), next_index);
        next_index
    };

    if tail.is_empty() {
        modules[module_index].endpoints.push(EndpointScopeAlias {
            public: public.clone(),
            internal: internal.clone(),
        });
    } else {
        let module = &mut modules[module_index];
        insert_endpoint_scope_module(
            &mut module.children,
            &mut module.child_index,
            tail,
            public,
            internal,
        )?;
    }
    Ok(())
}

fn emit_endpoint_scope_modules(resolved_api: &ResolvedApi) -> TokenStream2 {
    let mut modules = Vec::new();
    let mut module_index = BTreeMap::new();
    for endpoint in &resolved_api.endpoints {
        if endpoint.scope_modules.is_empty() {
            continue;
        }
        let internal = endpoint_internal_ident(endpoint);
        if let Err(err) = insert_endpoint_scope_module(
            &mut modules,
            &mut module_index,
            &endpoint.scope_modules,
            &endpoint.name,
            &internal,
        ) {
            return err.to_compile_error();
        }
    }

    let tokens = modules
        .iter()
        .map(|module| emit_endpoint_scope_module(module, 1));
    quote! { #( #tokens )* }
}

fn emit_endpoint_scope_module(module: &EndpointScopeModule, depth: usize) -> TokenStream2 {
    if depth > crate::limits::MAX_DSL_SCOPE_DEPTH {
        return emit_helpers::compile_error_tokens(
            &format!(
                "DSL scope nesting exceeds maximum supported depth of {}",
                crate::limits::MAX_DSL_SCOPE_DEPTH
            ),
            module.name.span(),
        );
    }
    let name = &module.name;
    let endpoint_reexports = module.endpoints.iter().map(|alias| {
        let public = &alias.public;
        let internal = &alias.internal;
        let supers = (0..=depth).map(|_| quote! { super:: });
        quote! { pub use #( #supers )* __endpoints::#internal as #public; }
    });
    let children = module
        .children
        .iter()
        .map(|child| emit_endpoint_scope_module(child, depth + 1));

    quote! {
        pub mod #name {
            #( #endpoint_reexports )*
            #( #children )*
        }
    }
}
