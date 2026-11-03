# Review leads reproduced on the baseline (`concord-v4-final` @ 625ac49, code 1d9ffe7)

| Lead | Status | Evidence |
|---|---|---|
| L1 Regex migration rewrites strings/comments/header literals and crosses endpoints on one line | CONFIRMED | `migration/input.rs` → `work.rs`, `diff1.txt`: `"-> Item via Json"` const, a `//` comment and the `"x-note"` header literal are rewritten; `GET ready() -> text String;  GET old() -> Item via Json;` becomes `-> Json text String; GET old() -> Item` (invalid). Idempotent on rerun. |
| L2 Dual-use guard disappears when the value's header contribution is removed | CONFIRMED (contract violation) | `dualuse/`: the UI case `header_text_dual_use.rs` is a compile error, but adding `headers { remove "x-signature"; }` on the endpoint compiles and `Prepared` prints `?s=NATIVE-SECRET` (`run.txt`). |
| L3 `TextHeader` allowlist is publicly implementable | see below | |

L3 detail: `concord::__private::TextHeader` (`crates/concord/src/__private.rs:64`) is a plain `pub trait`
in a `#[doc(hidden)] pub mod`; downstream `impl concord::__private::TextHeader for MyType {}` would
compile (it needs `ToHeaderValue + Text`, also implementable). Not sealed. This is use of
unsupported internals, not an ordinary-public-API exploit; the redesign decides whether the trait
survives at all.

Native constructor facts (http 1.5.0, `~/.cargo/registry/src/*/http-1.5.0/src/header/`):
- `HeaderName::from_static` (const): standard names, else every byte must map through
  `HEADER_CHARS_H2` (lowercase token chars), 1..=65535 bytes; uppercase panics.
- `HeaderName::from_bytes`: normalizes through `HEADER_CHARS` (uppercase accepted, lowercased).
- `HeaderValue::from_static` (const): bytes 32..=126 or tab only.
- `HeaderValue::from_str` / `from_bytes`: any byte >= 32 except 127, or tab — obs-text 128..=255
  (hence non-ASCII UTF-8) accepted, despite `from_str`'s doc saying visible ASCII.
- So static lowering is narrower than the runtime constructors for values; the baseline macro's
  own fragment check (visible ASCII, `model.rs:760`) and name check (`model.rs:832`) are
  competing validators, and the value check narrows acceptance relative to `from_str`.
