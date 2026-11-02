# Native headers and representation design — decisions (N01–N12)

Campaign of 2026-11-02, branch `concord-v4-native-repr`, starting from `a915c8d` (code identical to
`b0fe26e`). Owner request: native `HeaderValue` inputs and a substantively more natural
body/response DSL, plus the localized findings of the previous review. Evidence: four independent
specialist tracks (Claude Opus 5.5; effort requested, not observable from inside the sessions),
each in its own snapshot of `a915c8d` (`evidence/tracks/`), and a blind Codex design challenge
(`gpt-6.1-sol`, `xhigh`, verified in the session `turn_context`) written from the owner charter,
the comparison corpus and the source only (`reviews/codex/design_pass1_*`). Agent trials are
simulations, not human studies.

| ID | Question | Disposition |
|---|---|---|
| N01 | Header value input boundary | **CHANGE**: open trait `ToHeaderValue`, concrete impls (native, text, integers, `bool`, pointers); no `Display` fallback; templates are the formatting route |
| N02 | Native sensitivity | **CHANGE**: outgoing flag = incoming native flag OR Concord's classification, per value; never cleared |
| N03 | Visible-ASCII policy | **SPLIT**: text (literals, templates, `str`/`String`) stays visible ASCII; native values keep their bytes |
| N04 | Literal header values | **CHANGE**: lowered to compile-time-validated `static HeaderValue`s |
| N05 | Native value in a text placement | **CHANGE**: Concord diagnostic at the value's type; no implicit decoding |
| N06 | Body/response DSL | **CHANGE**: formats written in place (`json item: T`, `-> text String`) or inherited from an owner default (`request json;` / `response json;`); `via` removed |
| N07 | Body media type | **CHANGE**: belongs to the body (`json("application/merge-patch+json") p: T`), sent exactly when the body is |
| N08 | Form encoding | **ADD** built-in `form` format behind a non-default `form` feature |
| N09 | Placement-note deduplication | **FIXED** (linear membership, order kept) |
| N10 | Conditional `Clone` | **KEEP** strategy; **ADD** downstream positive/negative checks on 1.85, stable and non-blocking nightly |
| N11 | Multipart recipe | **CHANGE**: reqwest's own multipart in a separate example crate; parts + decoder kept |
| N12 | Runtime performance infrastructure | **CHANGE**: Criterion suite + allocation counter replace the `Instant` harness |

## N01 — header value input boundary

**Problem.** Every header value went through `Display` → `String` → visible-ASCII check →
`HeaderValue::from_str`. A `HeaderValue` (no `Display`) did not compile — neither in the placement
nor in the generated `Debug` (reproduced at `a915c8d`: 3 × E0277, `evidence/baseline_headervalue.txt`).

**Compiled alternatives** (Track H lab, both toolchains; Codex lab independently):

| Boundary | Result |
|---|---|
| `impl<T: Display> H for T` + `impl H for HeaderValue` | E0119 ("upstream crates may add a new impl of `Display` for `HeaderValue`") — the fallback cannot coexist |
| opt-in marker (`impl<T: HeaderText> H for T`) | coherent with native impls, E0119 with `&T`/`Box<T>` forwarding (needed for `&'static str`, `Vec<&str>`) |
| http's `TryFrom<_> for HeaderValue` | fails on borrowed integers (`&u64`, generated code reads by reference), no `u8`/`bool`; silently sends `"café"` as UTF-8 (text policy lost) |
| autoref dispatch in generated code | falls back to `Display` silently for integer literals and generic code; specialization emulation |
| syntax keyword (`header("k") native x: HeaderValue`) | works, no break; but every native placement and block reference restates what the type already says |
| Codex: concrete trait + `Format<T>` wrapper for `Display` types | coherent; puts a transport wrapper into caller signatures (`set_ip(Format(addr))`) |
| **concrete open trait + template escape** | coherent on 1.85 and 1.97.1; callers keep their own types |

**Choice.** `concord::ToHeaderValue { fn to_header_value(&self) -> Option<HeaderValue> }`, open,
implemented for `HeaderValue` (clone: bytes and flag preserved), `str`/`String` (visible ASCII),
`u8`–`u64`/`usize`/`i8`–`i64`/`isize`, `bool`, and `&T`/`Box`/`Rc`/`Arc`/`Cow` of those. A direct
header placement (`header("k") x`, `"k" = x`, `"k" = each xs`) uses it. `IntoValue<HeaderValue>`
also accepts `&HeaderValue`, so a `static` is an initializer or argument without `.clone()`.

**Accepted break.** A custom or foreign `Display` type placed directly as a header no longer
compiles; the error names the two fixes (implement the trait for an own type, or a template
`"{value}"`). Repository census of direct header value types: `String` ×15, `u32` ×1, a generic
`T` ×1, the deliberate `Display`-failure fixture ×1.

## N02 — sensitivity is part of the value

Outgoing flag = the value's own flag OR Concord's compile-time classification (marker, sticky
composition, well-known names), applied with `set_sensitive(true)` on Concord's clone only. Per
value: `each` items keep their own flags; an appended value does not flag its siblings; a replaced
or removed inherited value is not sent; `override … else inherit` with an absent value sends the
inherited value with its own flag. No runtime taint propagation: key-level protection is
`#[sensitive]` or a well-known name. Generated `Debug` (config, client, scope, request) and
`Prepared`'s `Debug` redact a flagged value. A value the application converts itself (e.g.
`to_str()` into a `String`) is new data protected by the declaration only.

**Dual use (amended after the executable review of `66a4478`, P1).** A value placed directly as a
header value and also read as text (path, subdomain, query value, template), and not statically
sensitive, must be text itself: `str`, `String`, the integers, `bool` and pointers to them (a hidden
`TextHeader` bound), whose `ToHeaderValue` never sets the native flag. A type's own
`ToHeaderValue` may flag its header value, and that runtime flag cannot reach the text placements
of the same value — `Prepared`'s `Debug` printed it in the URL and in a template header (Codex
reproduction). Rejected alternatives: propagating the runtime flag to text placements (a runtime
taint mechanism with dynamic redaction lists), and statically classifying every dual-use value
sensitive (over-redacts ordinary `String` values used in a path and a header). The earlier
`DebugTextHeader` route (design pass 2, finding 1) is subsumed and removed.

## N03 — visible ASCII, split by source

Text a Rust string produces (literals, templates, `str`/`String`) must be visible ASCII, space or
tab: a Rust string is Unicode, and sending its UTF-8 bytes as obs-text (RFC 9110 §5.5) is almost
always a bug that receivers decode as ISO-8859-1 or reject. A native `HeaderValue` is bytes the
application chose (`HeaderValue::from_bytes`): sent unchanged. Migration: "to send other bytes
deliberately, build a `HeaderValue` and declare the value as `HeaderValue`".

## N04 — literal header values

A header template without holes becomes one `static HeaderValue` per distinct literal
(`HeaderValue::from_static`, evaluated at compile time) and is cloned per request: 0 payload
allocations (was a format, a check and a copy per request). Query literals, templates with holes,
initializers and other expressions are never evaluated or hoisted by the macro.

## N05 — native values are not text

Path segments, subdomain labels, query values and template holes keep `Display`, behind a hidden
`Text` bound whose diagnostic says that a `HeaderValue` can only be a whole header value and must be
converted explicitly to be reused as text.

## N06 — body and response formats

See `CONTRACT.md` for the full truth table. Evidence: Track RA (owner defaults, `request R;` /
`response R;` as statements), Track RB (format words in place, three spellings compared), Codex
pass 1 (prefers defaults; local annotations the strongest rejected alternative). All three
prototypes produce byte-identical caller signatures and pass the same wire tests as the control.

**What each alternative proved.**
- Defaults (RA): representation mentions in the 20-endpoint corpus 26 → 10; the 8 remaining are
  exactly the non-JSON operations; no measurable compile cost; rustdoc states the origin. Weakest
  point: a forgotten exception for a payload both formats accept (`String`, bytes) is not a compile
  error; an endpoint's statement block is a poor place to read an exception.
- Format in place (RB): the endpoint line is complete (`-> text String`, `form creds: LoginForm`,
  `json("application/merge-patch+json") patch: ProductPatch`); errors point at the word; media type
  bound to the body removes a confirmed control trap (a declared `content-type` header is sent when
  an optional body is absent). Weakest point: a mostly-JSON API still writes `json` on every
  endpoint (17 times in the corpus) — the repetition the owner asked to remove.

**Choice: one rule, two positions.** Every body and typed output has a format. It is written where
the body or output is declared (`json item: T`, `-> text String`), or inherited from the nearest
owner that declares `request FORMAT;` / `response FORMAT;`; `body item: T` and a bare `-> T` take the
inherited one. Owners declare defaults; endpoints declare exceptions in place (there is no endpoint
`request`/`response` statement). Without owner defaults the language is exactly the local design;
with them, a mostly-JSON API states JSON once and every exception is visible on its endpoint line.
Built-in formats are the words `json`, `text`, `bytes`, `form` (renamed-dependency safe, no
imports); any other format is a type path implementing `Encode<T>`/`Decode<T>`.

**Rejected.** `via` (control) — the format follows the payload on every endpoint, twice for JSON
in/out, with imports. Endpoint `request`/`response` statements (RA) — a second place for the same
fact, far from the payload it governs. Format attributes per endpoint (RB γ) — detached from the
body/output. Clearing a default — no corpus case; under a client default a scope cannot demand
explicit formats, so a heterogeneous API puts defaults on its uniform scopes or writes every
exception in place. A media type on an output — Concord sends no `Accept` and does not check the
response `Content-Type`. Rejecting `-> ()` under an inherited format (RA's diagnostic) — `-> Unit`
for `type Unit = ()` would compile while `-> ()` did not; acceptance must not depend on spelling
(Codex pass 2, finding 4).

**Accepted risk.** Under a JSON default `-> String` means a JSON string, `body note: String` a
JSON string body and `-> ()` a JSON `null`; forgetting `-> text String` (or writing `-> ()` for an
endpoint that returns nothing) is a runtime decode error (or a server-side rejection), not a
compile error. Mitigations: the rustdoc of every endpoint names the effective format and its
origin; an endpoint that returns nothing omits `->`; authors who prefer locality declare no
defaults.

## N07 — media type belongs to the body

`FORMAT("type/subtype; params") name: T` and `body("…") name: T` (inherited format) set the body's
media type, validated at compile time and sent exactly when the body is. A `request` default may
carry one (`request json("application/vnd.api+json");`). A `content-type` header contribution
that survives composition on an endpoint still replaces a format's built-in media type (dynamic
values need it) and is sent whether or not a body is present; such a contribution together with an
explicit media type on the same endpoint is a compile error. The inserted `Content-Type` carries
the `content-type` key's static sensitivity classification (Codex pass 2, findings 2–3; the
four runtime cases are rows 8b of `CONTRACT.md`).

## N08 — form encoding

`form` (marker `concord::Form`) encodes `T: Serialize` with `serde_urlencoded` as
`application/x-www-form-urlencoded` and decodes it, behind the non-default `form` feature
(reqwest 0.13 likewise makes `form` opt-in). Downstream the same codec is ~13 lines plus a direct
dependency for every user; OAuth-style token endpoints need it.

## N09–N12

See `STATUS.md` and `PERF.md`: N09 is commit `01774a9`; N10/N11 are `checks/clone-compat`,
`examples/multipart`; N12 is `benches/criterion`.
