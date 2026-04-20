use crate::ast::SetOp;
use crate::emit_helpers;
use crate::sema::*;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{Ident, LitStr};

#[inline]
fn client_prefixed_ident(client: &Ident, suffix: &str) -> Ident {
    // Example: RiotClient + "Vars" => RiotClientVars
    emit_helpers::ident(&format!("{}{}", client, suffix), client.span())
}

#[inline]
fn value_uses_auth(v: &ValueKind) -> bool {
    match v {
        ValueKind::AuthField(_) => true,
        ValueKind::Fmt(fmt) => fmt.pieces.iter().any(|p| {
            matches!(
                p,
                FmtResolvedPiece::Var {
                    source: FmtVarSource::Auth,
                    ..
                }
            )
        }),
        _ => false,
    }
}

#[inline]
fn policy_uses_auth(policy: &PolicyBlocksResolved) -> bool {
    let ops_use = |op: &PolicyOp| match op {
        PolicyOp::Set { value, .. } => value_uses_auth(value),
        _ => false,
    };
    policy.headers.iter().any(ops_use)
        || policy.query.iter().any(ops_use)
        || policy.timeout.as_ref().is_some_and(value_uses_auth)
}

fn ep_optionals(ep: &EndpointIr) -> std::collections::BTreeMap<String, bool> {
    ep.vars
        .iter()
        .map(|v| (v.rust.to_string(), v.optional))
        .collect()
}

pub fn emit(ir: Ir) -> TokenStream2 {
    let mod_name = ir.mod_name.clone();
    let scheme = emit_scheme(ir.scheme);
    let domain = ir.domain.clone();

    let vars_ty = client_prefixed_ident(&ir.client_name, "Vars");
    let auth_inner_ty = client_prefixed_ident(&ir.client_name, "AuthInner");
    let auth_vars_ty = client_prefixed_ident(&ir.client_name, "AuthVars");
    let auth_state_ty = client_prefixed_ident(&ir.client_name, "AuthState");
    let cx_ty = client_prefixed_ident(&ir.client_name, "Cx");

    let vars_struct = emit_client_vars(&ir.client_vars, &vars_ty);
    let auth_vars_struct =
        emit_client_auth_vars(&ir.client_auth_vars, &auth_inner_ty, &auth_vars_ty);
    let auth_state_struct = emit_client_auth_state(&ir, &auth_state_ty, &cx_ty);
    let cx_struct = emit_client_context(ClientContextEmit {
        scheme: &scheme,
        domain: &domain,
        ir: &ir,
        policy: &ir.client_policy,
        vars_ty: &vars_ty,
        auth_vars_ty: &auth_vars_ty,
        auth_state_ty: &auth_state_ty,
        cx_ty: &cx_ty,
    });
    let client_wrapper = emit_client_wrapper(&ir, &vars_ty, &auth_vars_ty, &cx_ty);
    let internal_mod = emit_internal(&ir, &vars_ty, &auth_vars_ty, &cx_ty);
    let endpoints_mod = emit_endpoints(&ir, &cx_ty);

    quote! {
        mod #mod_name {
            use super::*;

            #vars_struct
            #auth_vars_struct
            #auth_state_struct
            #cx_struct

            #client_wrapper

            #endpoints_mod
            #internal_mod
        }
    }
}

// Keep feature-domain macro chunks in separate files without widening helper visibility.
include!("client.rs");
include!("endpoints/mod.rs");
include!("policy/mod.rs");
