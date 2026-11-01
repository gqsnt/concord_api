# V4 decision log

Each decision lists the alternatives actually investigated, the evidence, the choice and how
reversible it is. Sources:

- `evidence/reviews/codex/blind_20261101T160427Z_a492d3678e83.md` — independent blind Codex
  GPT-6.1 Sol XHigh design challenge, run **before** any V4 design was written (cited "Codex").
- `evidence/research/claude_{v3_auditor,semantics,composition,ergonomics}.md` — independent
  Claude Opus 5.5 subagent tracks (cited "auditor", "semantics", "composition", "ergonomics").
  Their experiment crates are under the session scratchpad and are cited by path in those reports.

Status vocabulary: **accepted** (implemented against), **provisional** (implemented, open to
review), **rejected**, **deferred**.

---

## D1 — Architecture: endpoint-specialized code generation + thin runtime (accepted)

| Option | For | Against |
|---|---|---|
| **A. Specialize per endpoint** (macro resolves composition for each endpoint; generated code calls small runtime helpers) | small runtime; suppressed contributions simply are not in the endpoint's code path, so their encoders never run; no runtime interpreter | generated code grows with endpoints × inherited contributions |
| B. Interpreted recipe (generated static tables walked by a runtime evaluator) | shared composition machinery | a second language at runtime; type erasure/allocations; V3's program.rs shape |

Codex independently preferred A (blind review §1–2) with B as the alternative; composition's
algorithm model (Q1) shows every outcome of the chosen operations is statically decidable except
"is this optional source present", which is a plain `if let` in generated code. **Choice: A.**
Failure condition to watch (Codex): expansion/compile cost at scale — measured in Gate 5
(`evidence/PERF.md`). Two crates only: `concord` (runtime + re-export) and `concord-macros`.

## D2 — reqwest boundary: Concord materializes a native `reqwest::Request` (accepted)

| Option | Evidence |
|---|---|
| **A. Build `reqwest::Request` from the app's `reqwest::Client`; `send` = `Client::execute` + decode** | `Request` is plain data with `url_mut/headers_mut/body_mut/timeout_mut` (reqwest 0.13.4 `async_impl/request.rs:47-138`); inspectable with no I/O (M03) |
| B. Yield `reqwest::RequestBuilder` | no getters; inspection needs consume/rebuild; encode errors would hide in reqwest's opaque builder error (composition Q3) |

Both converge in A because `RequestBuilder::from_parts(client, request)` (request.rs:189) turns a
materialized request back into a builder. Surface:

- the application **always supplies the `reqwest::Client`** (`Api::new(http, ...)`); Concord never
  builds one and has no transport configuration of its own (timeouts, redirects, proxies, TLS,
  default headers, cookies, compression stay on the reqwest client);
- `endpoint.send().await` → `Result<Output, Error>`: prepare, one `execute`, 2xx check, decode;
- `endpoint.prepare()` → `Prepared<R>` (no I/O); `Prepared::{request, send, send_raw, into_parts}`;
  `into_parts()` → `(reqwest::Client, reqwest::Request, Decoder<R>)` so a natively modified request
  can still be decoded with the declared representation (Codex §4.1, composition Q3);
- no `IntoFuture`/bare `.await` (Codex, composition: an explicit verb marks I/O);
- no `request_mut()` on `Prepared`: Concord's redaction metadata would go stale; going native is a
  one-way door (`into_parts`).

One `send` is one `Client::execute`; it is **not** one TCP exchange — redirects and anything else
configured on the reqwest client still apply (Codex §4, composition Q4 redirect experiment).

## D3 — Values and generated controls (accepted, one provisional point)

Alternatives investigated by semantics (exp0–exp7): Model A positional required values + prefixed
controls; Model B pub fields; Model C typed keys with fixed verbs; plus typestate and runtime
aggregated "missing input" builders. Codex proposed A with a root builder accumulating missing
inputs.

**Choice (Model A):**

| Declaration | Stored as | Initial | How the caller supplies/changes it |
|---|---|---|---|
| `x: T` | `T` | — | positional argument of the owner's entry (`Api::new`, scope method, endpoint method), in declaration order |
| `x: T = E` | `T` | `E` | `set_x(v)` |
| `x?: T` | `Option<T>` | absent | `set_x(v)` → present; `maybe_x(o)` → exactly `o` (`None` = absent) |
| `x?: T = E` | `Option<T>` | present `E` | same as above |
| `x: Option<T>` | `Option<T>` | — | positional; `None` is a payload, never "absent" |
| `x?: Option<T>` | `Option<Option<T>>` | absent | `set_x(None)` = present inner `None`; `maybe_x(None)` = absent |

- **Root inputs are positional too** (not a builder with a runtime error): rustc's E0061 names the
  missing parameter (semantics exp1). Accepted cost: same-typed required values can be swapped
  silently (semantics exp1 "SWAP COMPILED"); mitigation is newtypes. Reversible to a typestate
  entry if this proves common (semantics T-SWAP).
- **Two control verbs, not five.** `set_` and `maybe_` prefixes are injective, so value controls
  can never collide with each other or with built-ins (semantics exp4: bare names and `_opt`
  suffixes collide). V3's `replace_x` = `maybe_x`; `omit_x()` = `maybe_x(None)`;
  `set_x_if_some` is ordinary Rust (`if let`). `maybe_x` takes exactly `Option<T>` (Codex
  C4-R-009 found the spec claiming conversions there): `Option<impl IntoValue<T>>` would make a
  bare `None` ambiguous, so the contract was amended rather than the signature.
- **No `reset_x` (provisional).** Handles are immutable values, so "back to the default" is the
  original handle or a fresh entry call. Dropping reset removes retained-initializer storage.
  Codex and semantics kept it; none of the ergonomics shapes needs it (semantics T-RESET).
  V02/V08 reset clauses are recorded as *deliberately unsupported*. Reversible (additive).
- **Closed conversions** `IntoValue<T>`: identity, `&str → String`, `&String → String`
  (semantics exp3: `impl Into<u64>` rejects `7`; `impl Into<UserId>` accepts unintended raw ints;
  exact `T` rejects `"lit"`). Applies to positional args, setters, defaults and bodies.
- **Defaults** `= E` are arbitrary Rust expressions, evaluated exactly once per owner
  construction (`new`, each scope entry, each endpoint call), in declaration order, even if
  immediately overridden; never on clone, setter, prepare or send (semantics exp2 E1; lazy E2
  rejected because clone+send changed values). `E` is emitted inside a nested `fn` item so it
  cannot capture the owner's arguments or `self`.
- **Snapshots.** Client and scope handles are `Arc` snapshots with copy-on-write setters; a
  setter returns a new handle; existing children keep the values they captured (V10; semantics
  exp2 shared-mutable anti-model rejected). Every non-body value type must be `Clone`.
- **Names.** A bare name binds the current owner only; ancestors are `client.x` / `<scope>.x`.
  Scope names are unique along a chain. Every declared value must be used (liveness).

## D4 — Query/header composition: keyed slots, four operations (accepted; one point contested)

Algorithms compared by composition (`compose` experiment): A ordered occurrence list (V3) vs B
keyed slots. **Decisive fact:** `http::HeaderMap` groups values by name, so algorithm A's
cross-name header order cannot be materialized at all (composition Q1 "Decisive header facts").
**Choice: B for both families** — a key's slot is fixed at its first static appearance on the
root→endpoint chain; values within a slot keep contribution order. Accepted loss: interleaved
repeats across keys (`f=a&op=eq&f=b`) — native `url_mut()` is the escape.

Operations, all checked at compile time:

| Statement | Precondition | Effect |
|---|---|---|
| `"k" = v` (and direct placements) | no live writer of `k` earlier on the chain | new slot |
| `append "k" = v` | none | adds occurrences to `k`'s slot |
| `override "k" = v` | a live writer of `k` exists | if `v` is present: replaces the slot's occurrences in place; if `v` is an absent optional: inherited occurrences stay |
| `remove "k"` | a live writer exists | clears the slot; ends ownership (a later `"k" = v` is legal) |

**Contested point — what an absent optional does under the override operation.** Codex proposed
exact `replace` (absent ⇒ inherited removed) and lists "implicit conditional fallback" as a
predictability risk; composition proposed `override` (absent ⇒ inherited kept) with exact
replacement spelled `remove "k"; "k" = v`. Both are statically compiled. **Choice: override +
remove**, because it expresses both behaviours with one non-overlapping operation each, whereas
exact replace cannot express "fall back to the parent's current value" (ergonomics M11). Tests
`override_absent_keeps_inherited` and `remove_then_introduce_absent_removes` pin both meanings.
Raised explicitly with Codex at the Gate 3 review.

Other rules: header names compare case-insensitively and are emitted lowercase; query keys are
case-sensitive; header names `host`, `content-length`, `transfer-encoding`, `connection`, `te`,
`trailer`, `upgrade`, `keep-alive` are rejected (transport-owned framing); `Authorization` and
`Cookie` are ordinary (C10). Only surviving contributions are formatted/encoded. A query string
is written only if at least one pair survives (composition: an empty `query_pairs_mut` leaves `?`).

## D5 — URL composition (accepted)

- Base URL: `http`/`https`, host, optional port and path; no userinfo, query or fragment
  (composition: userinfo behaves differently between `client.get(url)` and
  `Request::new`+`execute`). Declared with `base "..."`, overridable at runtime with
  `with_base_url(..)` (ergonomics M1: tests/staging; V3 forbade it). The base path is a prefix
  kept verbatim minus one trailing slash; it is never resolved with `Url::join` (loses base path,
  allows `//host` authority injection — composition Q2).
- Path templates: relative, `/`-separated, no empty/`.`/`..` segments; literal characters must be
  RFC 3986 `pchar` without `%`, emitted verbatim (so `{name}:undelete` works). A trailing `/` is
  allowed only on an endpoint's own path.
- Dynamic path values are `Display`-formatted then percent-encoded with a strict set (everything
  except unreserved `A-Za-z0-9-._~`): `a/b` → `a%2Fb`, `%2F` → `%252F`, `a;b` → `a%3Bb`. A
  segment that ends up empty, `.` or `..` is a build error (V3 rejected `a/b`; composition showed
  `url`'s `push` silently drops `.`/`..`).
- Subdomain templates: `.`-separated labels prepended to the base host, deepest owner leftmost.
  Labels are ASCII letter/digit/hyphen, 1–63 bytes, no edge hyphen, lowercased; host ≤ 253;
  an IP-literal base with labels is a build error. IDNA input is deferred (UTS-46 folds `。`).
- Query values are `application/x-www-form-urlencoded` (space → `+`), the same as reqwest's
  `.query()`.

## D6 — Value formatting: `Display`, no adapters (provisional)

Scalar placements use `Display`; `each` placements iterate `&T` and `Display` each item.
`Option<T>`/`Vec<T>` have no `Display`, so placing them without `?`/`each` is a compile error —
Concord never infers how `None` is written (semantics exp5 shows the same property with
explicit `via` encoders). Rejected for now: a `via Encoder` adapter syntax (one more concept; a
newtype implementing `Display` does the same). Reversible (additive).

## D7 — Bodies and responses (accepted)

- Representations are types implementing public traits: `Encode` (bodies) and `Decode`
  (responses). Built in: `Json<T>` (feature `json`), `Text`, `Binary`, and `()` (response only).
  Declared explicitly: `body issue: Json<NewIssue>`, `-> Json<Issue>`; an endpoint without `->`
  decodes to `()`. No type-name inspection; no inherited representation defaults.
- A body sets `Content-Type` only if the composed headers do not already contain one. Concord
  never sets `Accept`.
- `send()` accepts any 2xx; other statuses are `Error::Status` carrying the **unread**
  `reqwest::Response` (`into_response()`), so API error bodies stay available without Concord
  reading or printing them. No `absent on`/404→`None`: `Option<T>` is just a Rust type.
- `Json<T>` decoding ignores `Content-Type`; `Text` requires valid UTF-8; `Binary` returns the
  bytes; `()` reads and discards whatever body arrives (204 or 200-with-body both succeed).
  Codex and the auditor preferred a strict "empty" decoder that rejects non-empty bodies; we
  chose the lenient form because `-> ()` means "no typed output" and ergonomics friction #8 shows
  the cost of 204/empty traps. A strict variant can be added as a separate type.

## D8 — Sensitivity (accepted)

Threat model: output **produced by Concord** — `Debug` of generated handles/endpoints/`Prepared`,
`Display`/`Debug` of `concord::Error`. Values are always sent.

- `#[sensitive]` marks a value. A query/header **key** becomes sensitive for an endpoint if any
  contribution on its chain is marked, or if its normalized name is in a small exact-name safety
  list (sticky: a later public replacement does not un-mark it — composition S1–S3, auditor).
  Path segments and host labels are sensitive if any hole references a sensitive value.
- Sensitive header values get `HeaderValue::set_sensitive(true)` (reqwest/`http` Debug prints
  `Sensitive`; h2 never-indexes) — a courtesy, not a guarantee.
- `concord::Error` never contains URLs, header values, request values or response bodies in its
  `Display`; transport errors keep `reqwest::Error::without_url()` as `source()` (composition:
  reqwest's own `Display` prints the full URL including query secrets) — unless the endpoint's
  URL has a sensitive part, in which case `source()` is `None`: nested reqwest errors keep URLs
  (Codex C4-R-008, reproduced in `tests/tls.rs`) and TLS errors print host names (ADV-1).
- Explicitly outside the guarantee (S06): the native `reqwest::Request`/`Response`/`Url` Debug,
  reqwest errors from user-driven execution, redirects forwarding custom headers and `Referer`
  with query, tracing, middleware and server logs.
- Safety-list (exact match after lowercasing and `_`→`-`): `authorization`,
  `proxy-authorization`, `cookie`, `set-cookie`, `x-api-key`, `api-key`, `apikey`,
  `x-auth-token`, `auth-token`, `access-token`, `refresh-token`, `id-token`, `token`,
  `client-secret`, `secret`, `password`, `signature`, `session`. Exact match, not substring:
  the auditor found V3's substring rule redacts `sessionless-mode` and would redact `page_token`.

## D9 — Generated names (accepted)

All generated types live flat in the invoking module, named by concatenating PascalCase owner
names (`GitHub`, `GitHubRepo`, `GitHubRepoIssue`). Rejected: per-client submodules (user types
written as `self::X`/`super::X` would need path rebasing, as V3 does); collisions are reported
by the macro. Endpoint built-ins are `send` and `prepare`; client built-ins `new` and
`with_base_url`. Navigation, control and built-in names are checked for collisions per type.

## D10 — Excluded (rejected, with reason)

| V3 concept | Reason |
|---|---|
| credentials/auth blocks, `SecretString`, challenges | product exclusion; a token is a sensitive header/query value |
| rate limits, retries, pagination, `effect`, endpoint `id` | product exclusions; no hooks |
| managed HTTP config (`configure_http`), timeouts, cancellation, call caps | reqwest client owns transport |
| `absent on`, `status` sets, `expose accepted_bytes` | ordinary Rust on `Error::status()` / `send_raw` |
| `path replace/off`, `subdomain replace/off`, `query off` | not needed by any studied shape; deferred (additive) |
| request/response representation defaults and `explicit` fences | one more inheritance concept; each endpoint names its representation |
| unbound intents, runtime endpoints, contract export | no studied use case |
| reserved public headers (Authorization, Accept, Cookie, Content-Type...) | existed for the auth/representation owners (auditor; C10) |
