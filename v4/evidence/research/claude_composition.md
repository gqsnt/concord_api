<!-- Verbatim final report of a Claude subagent (Opus 5.5), extracted from its transcript. -->

# Concord V4: HTTP composition and reqwest boundary research

**Status:** done. I modified no repository file and did not read `v4/`.

**Environment.** The toolchain is actually rustc/cargo **1.99.0**, not 1.97.1. Experiments ran with `cargo --offline` and `CARGO_TARGET_DIR=$D/target`, where `D=/tmp/claude-1000/-home-canarit--claude-spares-spare-hcpckH/7613327e-d3b5-4082-a519-87dc15658283/scratchpad/agents/composition`. Crates:
- `$D/urlexp` (bins `urlexp`, `url2`, `url3`)
- `$D/reqexp` (bins `reqexp`, `redirect`; reqwest 0.13.4 with `default-features=false, features=["json"]`)
- `$D/compose` (a model of both composition algorithms)

**Labels.** **[F]** is a fact verified by experiment or by source path:line. **[H]** is a hypothesis or recommendation.

---

## Q1. Query/header composition

### Shared static vocabulary (both algorithms)

**Source kinds:**
- required `x: T`
- defaulted `x: T = V` (always present)
- optional `x?: T` or `x?: T = V` (outer present/absent)
- payload `x: Option<T>` (always present; the encoder decides 0 or 1 occurrences)
- `each xs` (0..n occurrences)

**Operations:**
- **plain `"k" = x`**: compile error if any earlier potential owner of `k` exists on the chain (`E-DUP`).
- **`append "k" = x`**: no precondition.
- **`override "k" = x`**: requires an inherited owner (`E-NOTARGET`, catches typos). Present means drop earlier occurrences and emit. Absent means keep the inherited occurrences untouched.
- **`remove "k"`**: requires an owner. Clears occurrences and ends ownership, so a later plain declaration is legal and a later override is `E-NOTARGET`.

This drops V3's split between exact `replace` and `when_present` (`docs/spec.md:455-456`). Exact replace becomes `remove "k"; "k" = x?`, which is visible and needs no extra op. `query off` becomes macro sugar for removing every inherited key, which is statically enumerable, so no runtime Clear op is needed. Header keys are compared lowercased; query keys are case-sensitive.

### Algorithm A: ordered occurrence program

This is V3's shape (`crates/concord-core/src/request/program.rs:331-366`). There is one global occurrence list, and an override emits at **its own position**.

### Algorithm B: keyed groups

Think `IndexMap<Key, Vec<Contribution>>`:
- A key's slot is fixed at its first *static* declaration, independent of runtime presence.
- `override` replaces the group in place.
- `append` extends the group.
- `remove` clears it.

### Experiment: `$D/compose`

Command: `cargo run --offline -q`. Verbatim, trimmed; "calls" is encoder invocations per contribution.

```
Q1 root k=a, endpoint k=b         static: E-DUP: `k` at endpoint already declared at root
Q2 root k=a,z=9; ep override k=b  A: z=9&k=b   B: k=b&z=9   calls [0,1,1]
Q4 root k=a,z=9; s1 override k?=absent      A,B: k=a&z=9  calls [1,1,0]
Q5 root k=a; s1 ovr k?=b; s2 ovr k?=absent  A: z=9&k=b  B: k=b&z=9
Q6 root k=a,z=9; s1 remove; s2 k=c; ep append k=d  A: z=9&k=c&k=d  B: k=c&k=d&z=9
Q7 root append each[a,b],z=9; s1 append c; s2 append absent; ep append each[]
                                  A: k=a&k=b&z=9&k=c   B: k=a&k=b&k=c&z=9
Q8 root k=FAILING; ep override k=b          A,B: ok, calls [0,1,1]
Q9 root k=FAILING; ep override k?=absent    A,B: E-ENCODE at root key `k` (value withheld)
Q10 root k=1(default); s1 override each[]   A,B: z=9
Q11/Q12 override/remove with no ancestor    static: E-NOTARGET
Q13 root k?; s1 remove; s2 override         static: E-NOTARGET
Q14 root append k; s1 plain k               static: E-DUP
Q15 root Tag=A; ep tag=b                    A,B: Tag=A&tag=b
Q17 each[a,b] + cursor=""                   tag=a&tag=b&cursor=
Q18 all optional absent                     (no query)
H1 header X-Tag then x-tag plain            static: E-DUP (case-insensitive)
H2 X-Tag=a, X-Other=o; ep append x-tag=b    A: X-Tag,X-Other,x-tag   B: X-Tag,x-tag,X-Other
```

### Decisive header facts [F]

- `http::HeaderMap` groups values by name; cross-name interleaving is lost. Appending b=1, a=2, b=3 iterates as `[("b","1"),("b","3"),("a","2")]` (`urlexp`).
- On the wire, appending `X-Tag:t1`, `b-second:2`, `x-tag:t2` produced `x-tag: t1 / x-tag: t2 / b-second: 2`, with lowercase names (`reqexp` §2).
- Algorithm A's header order (H2) therefore cannot be materialized. Under A, headers would need a second algorithm. B *is* `HeaderMap`'s model.
- `insert` replaces all values of a name; `append` keeps them.
- reqwest client `default_headers` only fill **vacant** names (`reqwest-0.13.4/src/async_impl/client.rs:2617`). Verified: a client default `x-tag: client-tag` was **not sent** when the request had `x-tag`. The default `accept: */*` is a client default (`client.rs:284`).
- `HeaderValue::from_str("café")` is accepted as obs-text, while CR/LF and the name `"a b"` are rejected.

### Matrix (recommended B)

The prior state of `k` when the later level contributes:
- **N**: no owner.
- **P**: owner present with values V.
- **A**: owner whose source is absent.

| Later contribution | N | P | A |
|---|---|---|---|
| plain, any source kind | new slot; emit if present | E-DUP | E-DUP |
| append, required/defaulted/present | new slot; x | V then x (same slot) | x |
| append, absent `x?` | slot, nothing | V | nothing |
| override, required/defaulted/present | E-NOTARGET | x in V's slot (V's encoder never runs) | x |
| override, absent `x?` | E-NOTARGET | V kept | nothing |
| override, present with 0 occurrences (`each []`, `Option::None` via an omit encoder) | E-NOTARGET | removed | nothing |
| remove | E-NOTARGET | removed; ownership ends | nothing |

Q1–Q18 are the four-level chains for this matrix. This also settles C11: an absent `x?` keeps the inherited value, while `x: Option<T> = None` via an omit encoder removes it.

### Encoders and sensitivity

**Encoder rule [F, model].** Selection reads only outer presence. Encoders run only for survivors, in final order (Q8: 0 calls; Q9: the failing encoder runs and the error names placement and key, not the value). This keeps V3's property (`program.rs:8-11`).

**Sensitivity under replacement.** Value-level marking drops sensitivity on replacement. Key-level marking (the sticky union) does not:

```
S1 root api_key=SECRET(sensitive); ep override api_key=b(unmarked)
   debug(sticky-key) api_key=<redacted>   debug(value-level) api_key=b
S2 ... s1 remove; s2 api_key=c(unmarked)   sticky: <redacted>   value-level: c
S3 append pub; append SECRET(sensitive)    sticky: k=<redacted>&k=<redacted>  value: k=pub&k=<redacted>
```

### Recommendation [H]

- **Use B for both families.** It has a single algorithm, a static key order, and is predictable ("keys in first-declaration order; values per key in contribution order").
- **Accepted loss:** interleaved repeats across keys (`f=a&op=eq&f=b`) cannot be expressed. Native `url_mut().query_pairs_mut()` is the escape.
- **Sensitivity** is the static union per key along the chain, computed by the macro. There is no un-sensitive marker (S1–S3 as above).
- **Header extras:**
  - Compile-error reserved names: host, content-length, transfer-encoding, connection, te, trailer, upgrade, keep-alive.
  - `Authorization`, `Cookie` and `Proxy-Authorization` are ordinary and allowed (C10). V3 rejected them (`crates/concord-core/src/request/head.rs:1516-1534`).
  - Compile error for `append`/`each` on authorization, proxy-authorization, cookie, content-type.
  - The default header encoder rejects non-ASCII values.
- **Query extras:**
  - Always form-urlencode via `Url::query_pairs_mut().append_pair`, the same encoding reqwest's `.query()` uses (`request.rs:366-386`).
  - Call it only if at least one occurrence survives. Otherwise the URL keeps a stray `?` [F]: `query_pairs_mut with no pairs -> http://h.test/p?`.

---

## Q2. Path and subdomain

### Experiments (`$D/urlexp`, `url2`, `url3`) [F]

```
path_segments_mut().push:  "a/b"->a%2Fb  "%2F"->%252F  "a b"->a%20b  "café"->caf%C3%A9
  "a;b"->a;b  "a+b"->a+b  "a:b"->a:b  "a&b=c"->a&b=c  "{x}"->%7Bx%7D  "100%"->100%25
middle push /users/{id}/posts:  id=""  -> /api/users//posts
                                id="." -> /api/users/posts      <- silently dropped
                                id=".."-> /api/users/posts      <- silently dropped
Url::join: base /root join "a/b" -> /a/b (base path lost); join "//evil.test/x" -> http://evil.test/x
set_path "/a/%2e%2e/b" -> /b ;  set_path keeps %XX verbatim: "/v1/projects/my%20p:undelete" unchanged
base http://h.test/root?x=1 + push -> /root/a/b?x=1 ; base ...#frag -> /root/a/b#frag
set_host(label + ".api.example.com"):
  "EU"->eu.api...  "a%2eb"->a.b.api...  "a\u{3002}b"->a.b.api...  "\u{ff45}\u{ff55}"->eu.api...
  "evil.test:1" -> Ok, host=Some("evil.test")      <- ORIGIN ESCAPE
  "a_b","-a","" (".api.example.com"), 64-char label -> Ok   "a b" -> Err
  "eu.127.0.0.1" on IP base -> Err("invalid IPv4 address")
```

From `reqexp`:
- **Raw request line** with the path built with strict encoding: `GET /api/users/a%2Fb/x%252F/a%3Bb/caf%C3%A9?tag=a&tag=b&q=a+b`. hyper sends it verbatim.
- **Fragment:** a URL with `#secretfrag` executes, and the fragment is silently stripped from the wire.
- **Userinfo:** behavior diverges by path. `client.get("http://user:pw@…")` sends `authorization: Basic dXNlcjpwdw==` (`request.rs:582` `extract_authority`). `Request::new` + `execute` silently drops it.

### Path alternatives

- **P1:** url's segment set via `push`, with pre-validation.
- **P2:** a Concord strict set (everything except RFC 3986 unreserved `A-Za-z0-9-._~`), with the whole path built and one `set_path`.

`url2` shows the P2 outputs: `a;b->a%3Bb`, `a+b->a%2Bb`, `a:b@c->a%3Ab%40c`, `~x.y_z-` unchanged.

**Rejected:**
- `Url::join`, because it loses the base path and allows authority injection.
- Unvalidated `push`, because it silently drops `.`/`..`.

### Recommendation [H]

**Dynamic values:**
- Use P2. Data never acts as a delimiter; `;` matrix-parameter stripping in Java stacks is an external claim I did not verify.
- Reject `""`, `.` and `..` with `Build(InvalidPathSegment{placement})`, without the value.

**Static literals:**
- Validated at compile time as pchar and emitted verbatim, so Google-style `{p}:undelete` and `@scope` work.
- Compile errors: leading `/`, empty segment, `.`, `..`, `%`, `?`, `#`.

**Base URL** (runtime value, validated at client construction):
- http/https with a host.
- **No** userinfo (because of the divergence above), query or fragment.
- Port kept.
- The path is kept verbatim minus one trailing slash, so `/root` and `/root/` are equivalent and `/root//` stays `/root//a`.

**Subdomains:**
- ASCII LDH labels, 1–63 bytes, no edge hyphen, lowercased before `set_host`, total ≤ 253.
- Reject `.`, `_`, `%`, `:`, non-ASCII and empty.
- `xn--…` is accepted as ordinary ASCII.
- IDNA input is deferred, because UTS46 folds `。` and full-width characters into dots and letters.
- The deepest scope's label is leftmost (`{region}.api.example.com`).
- An IP base with a subdomain is a construction error.

---

## Q3. reqwest boundary

### reqwest 0.13.4 API facts [F]

- **`Request` is pure data with accessors:** `Request::new` and `url_mut`/`headers_mut`/`body_mut`/`timeout_mut`/`version_mut` (`request.rs:47-138`).
- **`RequestBuilder` has no getters.** Inspection requires the consuming `build()`/`build_split()` (`request.rs:483-494`), and `from_parts(client, req)` goes back (`:189`). Verified: the round trip kept client default headers on the wire. `RequestBuilder::header` *appends* (`:228`).
- **Feature gates:** `query`, `json` and `form` are feature-gated, but `bytes()` is not. So Concord needs no reqwest features if it serializes and decodes JSON itself.
- **`Response::json` attaches the URL to decode errors** (`response.rs:269-273`).
- **Send + Sync:** `Response`, `Error` and `Request` are all `Send + Sync + 'static` (`redirect` bin).

### Alternative A: Concord materializes `reqwest::Request`

`send` = `client.execute` + decode.

- Inspectable with no network (M03).
- Interconverts with a builder in one call (`from_parts`).
- Concord build errors stay in Concord's type.

### Alternative B: Concord yields a native `RequestBuilder`

- Still needs `Result<RequestBuilder, concord::Error>`, because encode failures must not be hidden in reqwest's opaque `Kind::Builder`.
- Cannot be inspected without a consume-and-rebuild round trip.

### Recommendation [H]: A

```rust
pub trait Endpoint {
    type Output;
    const META: &'static EndpointMeta; // name, Method, route template
    fn decode(resp: reqwest::Response) -> impl Future<Output = Result<Self::Output, Error>> + Send;
}
pub struct Prepared<E: Endpoint> { client: reqwest::Client, request: reqwest::Request, view: SanitizedView, _e: PhantomData<fn() -> E> }
impl<E: Endpoint> Prepared<E> {
    pub fn request(&self) -> &reqwest::Request;
    pub fn into_native(self) -> (reqwest::Client, reqwest::Request); // one-way door
    pub async fn send(self) -> Result<E::Output, Error>;              // exactly one execute
    pub async fn send_raw(self) -> Result<reqwest::Response, Error>;  // no status check
}
// generated: api.users().get(id).prepare()? / .send().await ; no IntoFuture
```

Generated calls use `.prepare()?` and `.send().await`; there is no `IntoFuture`, because a bare `.await` hides that this does I/O and can fail to build. There is deliberately no `request_mut()`: once a request goes native, Concord's sanitized view would be stale. After `into_native`, the user calls `client.execute(req)` and then `GetUser::decode(resp)`.

**Decoding after the user modifies the request.** Typed decoding is a pure function of (declared response contract, `Response`). It still runs, but Concord cannot tell whether the modified request still targets that endpoint. A changed `Accept` header yields an honest `Decode` error.

**Concord must know:** base, path template and encoders, labels, the key program and sensitivity, body representation and content-type, decoder, accepted statuses, and static endpoint metadata.

**reqwest already knows (do not restate):** timeouts, proxy, TLS, redirects, referer, default headers, UA, cookies, compression, HTTP version, pool, DNS.

---

## Q4. Sensitivity

### Facts [F] (`reqexp`, `urlexp`, `redirect`)

```
Request Debug: url: Url {.. query: Some("api_key=QSECRET") ..}, headers: {"authorization": Sensitive, "x-api-key": "PLAINSECRET"}
HeaderValue set_sensitive Debug: Sensitive ; clone keeps sensitive: true
Url Debug: password: Some("pw") .. query: Some("token=SECRET")
reqwest::Error Display: error sending request for url (http://127.0.0.1:40299/users/SECRETPATH?token=QSECRET)
  timeout / error_for_status / decode Display: likewise "for url (...?token=QSECRET)"
  without_url Display: error sending request   (Debug source keeps only 127.0.0.1:port)
  decode Debug source: Error("invalid type: string \"SECRET-BODY\", expected u32", line: 1, column: 19)
Response Debug: url ..?token=QSECRET .. headers {"set-cookie": "sid=COOKIESECRET"}
cross-origin 302, default client, request had authorization + x-api-key(set_sensitive):
  target saw: x-api-key: APIKEYSECRET
              referer: http://127.0.0.1:40667/start?token=QSECRET
```

The redirect stripping list is fixed to Authorization, Cookie, Cookie2, Proxy-Authorization and WWW-Authenticate (`reqwest/src/redirect.rs:239-252`). The Referer keeps the query (`redirect.rs:294-304`), and Referer is on by default (`client.rs:312`). h2 never-indexes sensitive values (`h2-0.4.20/src/hpack/table.rs:231`).

### Recommendation [H]: both levels

- **Key-level sticky** for query/header keys, computed statically per endpoint.
- **Value-level** for keyless placements: path holes, host labels, and the base URL. The base can itself be a secret, e.g. a webhook path.
- **Name safety net:** a header/query name-based list (authorization, proxy-authorization, cookie, plus normalized substrings token/secret/password/api-key/apikey/signature/credential/session). Over-redaction in Debug is cheap.
- **Smallest owned surface:**
  - `Debug` for client, scope and endpoint values and `Prepared`. Rendering uses the route template plus non-sensitive values; body shows kind and length only.
  - Concord `Error` Display/Debug.
  - `set_sensitive(true)` on sticky header values in the native request, which is free.
- **Disclaimer (S06):** once native, `Request`/`Url`/`Response` Debug, reqwest errors from user-driven execution, redirect forwarding of custom headers, Referer-with-query, tracing/middleware and server logs are outside the guarantee. Users with query or custom-header secrets should consider `redirect::Policy::none()`/`.referer(false)`.

---

## Q5. Errors [H]

```rust
pub struct Error { endpoint: Option<&'static EndpointMeta>, kind: ErrorKind }
#[non_exhaustive] pub enum ErrorKind {
    Build(BuildError),         // {placement: Path(i)|Host(i)|Query(key)|Header(name)|Body|Base, reason: &'static str}
    Transport(reqwest::Error), // always stored .without_url(); is_timeout()/is_connect() still work
    Status(StatusError),       // status + canonical reason; holds unread Response; Debug prints status only
    Decode(DecodeError),       // {category: Syntax|Data|Eof|ContentType|EmptyBody, line, column}
}
```

- **Display** shows `users.get GET /users/{id}: status 404 Not Found`: static metadata only, never values, URL, body or the server reason phrase.
- **Status bodies:** `StatusError::into_response()` lets the user read the body explicitly.
- **Decode errors:** `DecodeError` has no `source()` by default, because serde text echoes the body. An opt-in `serde_error()` accessor is available.
- **Custom encoders** return `EncodeError::new(&'static str)`, so values cannot be formatted into the message.
- **Caveat:** Concord strips reqwest's top-level URL but does not rewrite deeper hyper source chains. Proxy-error contents are unverified.

---

## Discriminating test corpus (expected wire results)

**Composition (B):**
- `q_redeclare_compile_fail`: Q1, Q14, Q16, H1 (trybuild).
- `q_override_target_compile_fail`: Q11–Q13.
- `q_override_present_in_place`: Q2 → `?k=b&z=9`.
- `q_override_absent_keeps`: Q4 → `?k=a&z=9`.
- `q_override_empty_each_removes`: Q10 → `?z=9`.
- `q_remove_redeclare_append`: Q6 → `?k=c&k=d&z=9`.
- `q_append_chain`: Q7 → `?k=a&k=b&k=c&z=9`.
- `q_each_repeat`: `["a","b","a"]` → `?tag=a&tag=b&tag=a`.
- `q_empty_string`: `?cursor=`.
- `q_all_absent_no_qmark`: the URL ends `/p` exactly.
- `q_key_case`: `?Tag=A&tag=b`.
- `q_option_payload_vs_absent` (C11).
- `suppressed_encoder_zero_calls`: Q8. `surviving_encoder_error_no_value`: Q9.
- `h_append_grouped`: wire `x-tag: a`, `x-tag: b`, `x-other: o`.
- `h_shadows_client_default`: only `x-tag: t1` is sent.
- `h_authorization_ordinary`: sent verbatim and marked sensitive.
- `h_reserved_compile_fail`.
- `h_non_ascii_build_error`.
- Sticky sensitivity: `s_sticky_override` (S1), `s_sticky_remove_redeclare` (S2), `s_sticky_append` (S3).

**Paths:**
- `path_base_variants`: `http://h`, `/`, `/root`, `/root/`, `:8080/root/v1` produce `/a/42`, `/a/42`, `/root/a/42`, `/root/a/42`, `:8080/root/v1/a/42`.
- `path_dynamic_strict`: the P2 table, asserted on the loopback request line.
- `path_dot_empty_reject`.
- `path_static_pchar_verbatim`: `/projects/my%20p:undelete`.
- `base_reject_userinfo_query_fragment_scheme`.

**Hosts:**
- `host_labels`: `EU` → `eu.`. These are rejected: `a%2eb`, `a。b`, `ｅｕ`, `evil.test:1`, `a_b`, `""`, `-a`, a 64-char label, non-ASCII.
- `host_order`: `eu.api.example.com`.
- `host_ip_base_error`.

**Boundary:**
- `prepare_no_network`: a listener asserts zero accepts.
- `custom_client_preserved`: default header plus timeout kind.
- `into_native_then_decode`.
- `send_exactly_once`: one connection even on 500.

**Redaction:**
- `transport_display_no_url`: the Concord error lacks `QSECRET`. The test also asserts the raw reqwest Display contains it, so the baseline stays honest.
- `status_no_body`.
- `decode_no_body_echo`: `Data` at line 1, col 19, with no `SECRET-BODY`.
- `prepared_debug_redacts_all_placements`.
- `native_header_is_sensitive`.
- `native_debug_leaks_query_documented`.
