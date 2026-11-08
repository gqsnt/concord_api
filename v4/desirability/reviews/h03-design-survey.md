# H03 design survey — how generated methods take caller values (base `8b93d459`)

Read-only survey by a Claude subagent (Explore) for the coordinator; not a review or a decision.
Nothing compiled; statements about rustc output after a change are expectations.

## Mechanism
- `IntoValue<T>` (`crates/concord-core/src/value.rs`): sealed (`mod sealed` 7-15, trait 57); impls:
  `T` for `T` (62), `&str` → `String` (69), `&String` → `String` (76), `&HeaderValue` →
  `HeaderValue` (83). No `Into`, numeric widening, `Option` wrapping or newtype conversion.
  `#[diagnostic::on_unimplemented]` (52-56) applies to E0277 only; the E0283 limitation is
  documented with a `compile_fail,E0283` doctest (33-51).
- The macro (`crates/concord-macros/src/emit.rs`) emits `impl IntoValue<T>` for positional
  required values and bodies (`positional_params` 807-830), `set_x` on values (887) and bodies
  (928, 945), parameter-group setters (199, 202), and initializers
  (`<_ as IntoValue<T>>::into_value(expr)`, 791); `maybe_x` takes exactly `Option<T>`;
  `{Client}Config` fields take exactly `T` (1641), so `"k".into()` already infers there.
- Messages read `impl ::concord::__macro::concord_core::IntoValue<X>` because the macro refers to
  the runtime crate through `$crate::__macro::concord_core`.

## Why the I5 cases fail
All `default_body` and `into_body` cases (Cloudflare body, Kraken body, Notion body, GitHub
positional newtype, Nager positional `String`) get E0283 on the seeded argument; the
`default_body` label names `IntoValue`, `__macro`, `concord_core::`. `wrong_setter` gives E0277
with Concord's own headline; with a concrete `T` it would become E0308 ("expected `PerPage`,
found integer") and lose Concord's note.

## What the barrier buys
Only `&str`/`&String` → `String` and `&HeaderValue` → `HeaderValue`. Numbers, newtypes, `Option`s
and bodies are identity-only today (callers already write `ZoneId::from("z")`, `PerPage::new(50)?`,
`BalanceOptions::default()`). Rough counts of call sites relying on `&str`/`&String` → `String`:
SDK tests ≈ 470-620 positional + ≈ 97 setters; SDK docs ≈ 90-110; `docs/` + README ≈ 58; crate
tests ≈ 1,113 setters (+ positional); `&HeaderValue` ≈ 3.

## Options
- (a) exact `T` everywhere: all three spellings infer; breaks ≈ 1,900+ `&str` sites; docs and UI
  tests (`conversions_are_closed`, `newtype_is_not_converted`) change.
- (a′) **hybrid**: exact `T` except a syntactic `String`/`HeaderValue` (last path segment), which
  keep `IntoValue`: breaks ≈ 0 (aliases: `formats.rs:53` `Raw = String`, `native_headers.rs`
  `Hv`); fixes the GitHub, Cloudflare, Kraken, Notion cases and the Kraken/Frankfurter/part of
  Cloudflare friction; Nager unchanged.
- (a″) bodies only: fewer fixes at similar cost.
- (b1) `impl Into<T>`: still generic (no inference) and breaks bare integer literals; not
  recommended. (b2) a second `_exact` method: no breakage, doubles the surface. (b3) **named
  generic parameters** for the remaining `IntoValue` positions: the E0283 label names a type
  parameter instead of the internal path; no breakage; inference unchanged.
- (c) diagnostics only: cannot reach E0283.
- The I5 bank counts a seeded line that compiles as a failure: it needs relabelling (compiles
  with the intended value = resolved) or H03's gains do not show in R5.

Recommendation: pilot (a′) hybrid exact inputs, and (a′) + (b3).
