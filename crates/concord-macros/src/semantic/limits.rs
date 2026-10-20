//! Statically certain request-head structure bounds (§20.1, LIMIT-01).
//!
//! Core enforces the public head component and header-count ceilings on
//! every plan and execution. When the structure an endpoint emits on *every*
//! symbolic branch already exceeds one of those ceilings, no call can ever
//! succeed, so the declaration is rejected where it is written instead of
//! failing on first use. Optional, conditional and repeated (`each`)
//! emissions are not counted: they only contribute when present, and core
//! still bounds them at run time.

use super::head::same_key;
use super::{Emission, LocalHost, LocalPath, LocalProgram, Model, PlacementFamily, StatementKind};
use crate::syntax::diagnostics::{Code, coded};

// Core owns these hard limits (`MAX_REQUEST_HEAD_COMPONENTS`,
// `MAX_REQUEST_HEADER_COUNT`); this crate does not depend on core.
const MAX_REQUEST_HEAD_COMPONENTS: usize = 256;
const MAX_REQUEST_HEADER_COUNT: usize = 256;

/// One potentially emitted query/header occurrence.
struct Occurrence {
    family: PlacementFamily,
    key: String,
    /// Emitted on every symbolic branch (total scalar source, not removed or
    /// made conditional by a later operation).
    certain: bool,
}

pub(crate) fn check_static_head_bounds(model: &Model) -> syn::Result<()> {
    for (index, endpoint) in model.endpoints.iter().enumerate() {
        let label = model.site_label(crate::semantic::Site::Endpoint(index));
        let chain = model.chain(endpoint.owner);
        let mut labels = 0_usize;
        let mut segments = 0_usize;
        let mut occurrences = Vec::new();
        for owner in &chain {
            let owner = &model.owners[*owner];
            labels = match &owner.host {
                None => labels,
                Some(LocalHost::Prepend(local)) => labels + local.len(),
                Some(LocalHost::Replace(local)) => local.len(),
                Some(LocalHost::Off) => 0,
            };
            segments = apply_path(segments, owner.path.as_ref());
            apply_program(&mut occurrences, &owner.program);
        }
        segments = apply_path(segments, endpoint.path.as_ref());
        apply_program(&mut occurrences, &endpoint.program);

        let certain = |family: PlacementFamily| {
            occurrences
                .iter()
                .filter(|occurrence| occurrence.family == family && occurrence.certain)
                .count()
        };
        let headers = certain(PlacementFamily::Header);
        if headers > MAX_REQUEST_HEADER_COUNT {
            return Err(coded(
                endpoint.span,
                Code::Place,
                format!(
                    "{label} always emits {headers} public headers; one effective request head permits at most {MAX_REQUEST_HEADER_COUNT} public headers, so drop inherited headers with `headers replace {{ ... }}` or combine values"
                ),
            ));
        }
        let components = labels
            + segments
            + certain(PlacementFamily::Query)
            + headers
            + endpoint.effective_auth.len();
        if components > MAX_REQUEST_HEAD_COMPONENTS {
            return Err(coded(
                endpoint.span,
                Code::Place,
                format!(
                    "{label} always retains {components} request head components (DNS labels, path segments, query pairs, headers and auth placements); one effective request head permits at most {MAX_REQUEST_HEAD_COMPONENTS} retained components, so replace inherited contributions (`path replace`, `query replace`, `headers replace`) or combine values"
                ),
            ));
        }
    }
    Ok(())
}

fn apply_path(segments: usize, path: Option<&LocalPath>) -> usize {
    match path {
        None => segments,
        Some(LocalPath::Append(template)) => segments + template.segments.len(),
        Some(LocalPath::Replace(template)) => template.segments.len(),
        Some(LocalPath::Off) => 0,
    }
}

fn apply_program(occurrences: &mut Vec<Occurrence>, program: &LocalProgram) {
    for op in &program.ops {
        let family = op.family;
        let total_scalar = op.presence.is_none() && matches!(op.emission, Some(Emission::Atom(_)));
        let matches = |occurrence: &Occurrence| {
            occurrence.family == family && same_key(family, &occurrence.key, &op.key)
        };
        match op.kind {
            // A family fence clears the inherited emission program.
            None => occurrences.retain(|occurrence| occurrence.family != family),
            Some(StatementKind::Introduce | StatementKind::Append) => {
                occurrences.push(Occurrence {
                    family,
                    key: op.key.clone(),
                    certain: total_scalar,
                });
            }
            Some(StatementKind::Replace) => {
                occurrences.retain(|occurrence| !matches(occurrence));
                occurrences.push(Occurrence {
                    family,
                    key: op.key.clone(),
                    certain: total_scalar,
                });
            }
            // Either the earlier occurrences survive (absent source) or the
            // new one replaces them: neither side is certain.
            Some(StatementKind::WhenPresent) => {
                for occurrence in occurrences
                    .iter_mut()
                    .filter(|occurrence| matches(occurrence))
                {
                    occurrence.certain = false;
                }
                occurrences.push(Occurrence {
                    family,
                    key: op.key.clone(),
                    certain: false,
                });
            }
            Some(StatementKind::Remove) => occurrences.retain(|occurrence| !matches(occurrence)),
        }
    }
}

#[cfg(test)]
mod tests {
    use proc_macro2::TokenStream;
    use quote::quote;

    fn resolve(tokens: TokenStream) -> syn::Result<super::Model> {
        crate::syntax::parse(tokens).and_then(super::super::resolve)
    }

    fn literals(prefix: &str, count: usize) -> Vec<String> {
        (0..count)
            .map(|index| format!("{prefix}{index:03}"))
            .collect()
    }

    fn path_of(count: usize) -> String {
        literals("p", count).join("/")
    }

    fn headers_of(count: usize) -> TokenStream {
        let statements = literals("x-h", count)
            .into_iter()
            .map(|name| quote!(#name = "v";));
        quote!(#(#statements)*)
    }

    #[test]
    fn certain_path_segments_are_bounded_at_the_declaration() {
        let at_limit = path_of(256);
        assert!(
            resolve(quote! {
                client A {
                    base "https://a.test/";
                    response Json;
                    GET e() -> T { path #at_limit; }
                }
            })
            .is_ok()
        );
        let over = path_of(257);
        let error = resolve(quote! {
            client A {
                base "https://a.test/";
                response Json;
                GET e() -> T { path #over; }
            }
        })
        .err()
        .expect("257 certain segments are rejected")
        .into_compile_error()
        .to_string();
        assert!(
            error.contains("[C3-PLACE]") && error.contains("at most 256 retained components"),
            "{error}"
        );
    }

    #[test]
    fn inherited_structure_counts_and_fences_release_it() {
        // 200 inherited segments plus 57 local ones exceed the ceiling ...
        let scope = path_of(200);
        let local = path_of(57);
        let error = resolve(quote! {
            client A {
                base "https://a.test/";
                response Json;
                scope s() {
                    path #scope;
                    GET e() -> T { path #local; }
                }
            }
        })
        .err()
        .expect("inherited segments count");
        assert!(
            error.to_string().contains("257 request head components"),
            "{error}"
        );
        // ... but `path replace` drops the inherited generated segments.
        assert!(
            resolve(quote! {
                client A {
                    base "https://a.test/";
                    response Json;
                    scope s() {
                        path #scope;
                        GET e() -> T { path replace #local; }
                    }
                }
            })
            .is_ok()
        );
    }

    #[test]
    fn only_certain_header_occurrences_count() {
        let over = headers_of(257);
        let error = resolve(quote! {
            client A {
                base "https://a.test/";
                response Json;
                GET e() -> T { headers { #over } }
            }
        })
        .err()
        .expect("257 certain headers are rejected")
        .into_compile_error()
        .to_string();
        assert!(error.contains("at most 256 public headers"), "{error}");
        // Optional and repeated sources are not certain: 256 literal headers
        // plus an optional and an `each` emission remain valid statically.
        let at_limit = headers_of(256);
        assert!(
            resolve(quote! {
                client A {
                    base "https://a.test/";
                    response Json;
                    GET e(extra?: String, many: Vec<String>) -> T {
                        headers { #at_limit "x-extra" = extra; "x-many" = each many; }
                    }
                }
            })
            .is_ok()
        );
        // A removed or fenced header no longer counts.
        assert!(
            resolve(quote! {
                client A {
                    base "https://a.test/";
                    response Json;
                    headers { #over }
                    GET e() -> T { headers replace { "x-only" = "1"; } }
                }
            })
            .is_ok()
        );
    }
}
