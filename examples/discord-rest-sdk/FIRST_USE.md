# SDK24 Discord REST — fresh consumer

Current API note (P4.2): this evidence records the earlier API. Paging `_from` twins now use the base factory followed by `.starting_at(Resume::At(key))`; Datadog/Notion traversal cycles use `PagesError::Repeated`. See [migration](../../docs/MIGRATION.md#runtime-operations-and-paging-p42).

Date: 2026-11-07. Live provider calls: **NOT_RUN**. All application calls use
`examples/testserver` on loopback, with synthetic bot credentials. SDK implementation,
existing SDK tests, `FRICTION.md`, and `PROVENANCE.md` have not been read.

## Predictions

Written before application code or a first SDK call, from the README and generated
public rustdoc.

### Workflow 1: guild channels → message pages → one message

1. Construct `Discord::new(http, DiscordConfig { token, limiter: RateLimiter::new() })`
   and use `with_base_url(server_url + "/api/v10")`. Configure a transport timeout
   in the supplied reqwest client. No I/O occurs during construction.
2. `discord.guild(GuildId::new(...)).channels().send().await` sends
   `GET /api/v10/guilds/{guild}/channels`, no query/body, returning `Vec<Channel>`.
   Select a guild text channel using `kind.is_guild_text()`.
3. `channel.history(MessageLimit::new(2)?).items().collect_complete().await`
   sends `GET /api/v10/channels/{channel}/messages?limit=2`, then
   `GET .../messages?before={oldest previous message ID}&limit=2` (query pair
   order is immaterial). Returns newest-first `Vec<Message>` only after a short
   page, including an empty page. A full page alone does not prove completion.
4. `channel.message(messages[0].id).send().await` sends
   `GET /api/v10/channels/{channel}/messages/{message}`, no query/body, returning
   `Message` with that ID and channel.

All requests send `Authorization: Bot {token}` (token supplied without `Bot `),
the documented default `User-Agent`, and JSON `Accept`. No token is placed in
paths, queries, or ordinary SDK debug/error output.

Failure predictions: non-2xx/non-429 is `Error::Api`, with authoritative status,
route resource, response metadata, and parsed Discord code/document (or a document
decode/media/limit error). In particular 403/50013 means missing permissions,
404/10008 unknown message, 401 means stop using the credential. A malformed 2xx
JSON body is `Error::Concord` with `Decode` and a retained response head;
incorrect success Content-Type is `Metadata`. Missing history permission can
produce an empty successful history and cannot be distinguished from an empty
channel. Unordered or oversized history pages produce `Error::Protocol`.

`max_pages(1)` with a full first page produces an incomplete result retaining
that page's items and `Resume::At(oldest_id)`, not successful exhaustion. A
later failed page likewise retains the prefix, failed cursor, counters, and
typed provider error. Resume with `history_from(resume, limit)` deliberately.

Two channels reporting the same `X-RateLimit-Bucket` have independent windows:
exhausting channel A must not gate B, and learning B's window must not unblock
A. A 429 reports scope/bucket/headers/body and uses the larger usable
`Retry-After` or fractional JSON `retry_after`. A global 429 gates every route
for this bot, including other channels and guilds and cloned handles, before I/O.
The SDK never sleeps or retries; a local refusal is `Error::Gated`.

### Workflow 2: fake message creation → read returned ID

1. `channel.create_message(CreateMessage::text("CRM case 427 resolved")?
   .with_nonce(Nonce::new("crm-case-427-v1")?)).send().await` sends
   `POST /api/v10/channels/{channel}/messages`, no query, with JSON
   `content`, string `nonce`, and `enforce_nonce: true`; ordinary text is
   non-TTS. It sends the common headers above plus JSON `Content-Type`.
   Successful 2xx returns a `Message` with the created ID and nonce.
2. `channel.message(created.id).send().await` reads that exact returned ID via
   the GET above. Assert content, author, channel, ID, and nonce agree.

Failure predictions: invalid content/nonce is rejected locally by constructors;
an invalid token header fails before I/O without echoing the token. Permission
or validation refusals preserve status and Discord error code/document.
If the fake accepts the POST but delays its response past the transport timeout,
the result is `Error::AmbiguousWrite` with `accepted == None`, channel/operation,
and a timeout transport error. There is exactly one POST: timeout is not authority
to resend. Reconcile by listing messages, identifying the same nonce, then
fetching its ID. A 2xx POST with an unreadable/undecodable body is also ambiguous,
with `accepted == Some(status)`. Debug/display must not expose message content,
provider error text, URL, body, or token; raw fields are deliberate inspection.

## Public documentation read before implementation

- `examples/discord-rest-sdk/README.md`.
- Generated `target/doc/concord_sdk_discord_rest/` crate index and public client,
  scope, endpoint, model, error, limiter, and history type pages; no rustdoc source
  links were opened.
- Generated public `concord-pagination` rustdoc for `Pages`, `Incomplete`,
  `PagesError`, `Resume`, `Page`, `Progress`, `SequentialReport`, and `Outcome`;
  testserver crate index, `TestServer`, `Recorded`, `Reply`, and `Cleanup` for
  the fixture API. After the first working calls, generated `concord-core`
  public rustdoc for `Error`, `ErrorKind`, `ResponseMeta`, and `Operation` to
  inspect response metadata and timeout classification.
- Required repository instructions and package/workspace manifests only for
  build/scope setup. No SDK implementation or existing test source.

Official read-only research, all read **2026-11-07**:

- [Discord rate limits](https://docs.discord.com/developers/topics/rate-limits):
  top-level resource isolation, bucket hashes, global gates, fractional delays.
- [Guild resource — Get Guild Channels](https://docs.discord.com/developers/resources/guild#get-guild-channels):
  channel listing excludes threads; viewability change is scheduled for 2026-11-16.
- [Message resource](https://docs.discord.com/developers/resources/message):
  message ordering/pagination, permissions, creation, nonce enforcement.
- [API reference](https://docs.discord.com/developers/reference):
  v10, bot Authorization, User-Agent, snowflakes, error documents.

## Setup observations

- Initial working tree was clean. Package was already a workspace member.
- `CARGO_BUILD_JOBS=4 cargo doc --offline -p concord-sdk-discord-rest --no-deps`
  passed without `--locked`; `Cargo.lock` compared byte-for-byte equal to the
  pre-command snapshot. Subsequent Cargo commands use `--locked`.
- Default worktree `target/` is used; no `CARGO_TARGET_DIR` override. No new crates.
- Resource snapshot: workspace filesystem 357 GiB available; `target/` 1.4 GiB;
  memory 21 GiB available, 20 reported CPUs. One Cargo process at a time, four jobs.
- Cloud runtime skill was read; its `environment_status` tool is unavailable in
  this session and `/etc/codex/network-policy.json` is absent. Offline Cargo and
  loopback fixtures need no provider credentials.

## First working call

The first application run was
`CARGO_BUILD_JOBS=4 cargo test --offline --locked -p concord-sdk-discord-rest --test first_use`:
**2 passed, 0 failed**, first compile, no warnings (0.02 s test execution).
Each caller's first `send()` worked: guild channel listing in workflow 1 and
message creation in workflow 2. Tests execute independently, so no ordering is
claimed between them. Workflow 1 selected a text channel, read two history pages
and fetched one ID (four wire requests). Workflow 2 stored a POST in the fixture
and fetched the returned ID (two wire requests). Headers and response fields
matched the predictions. No SDK or existing test source lookup was needed.

What it took: existing dev dependencies, the README's construction/send examples,
public model rustdoc for realistic JSON fixtures, and public testserver rustdoc
for `start`, `Reply::json`, request inspection, and shutdown. No extension traits,
explicit generated endpoint types, `prepare`, raw transport execution, or
advanced SDK APIs were needed.

## Friction incidents (in observation order)

1. The generated endpoint `send()` rustdoc displays
   `<RateLimiter as SendWith<Vec<Message>>>::Output` instead of directly stating
   `Result<Vec<Message>, Error>`. The README and client prose resolved this;
   the first compile succeeded without importing `SendWith` or naming that type.
   Suggested macro/docs improvement only: show the policy result alias/concrete
   signature when available. Shared crates remain frozen.
2. `MessageHistory` is a `Pages<Message, MessageId, (), Error>` alias. Its page
   does not enumerate the pagination methods; checking incomplete results,
   resumption, caps and counters required following the public pagination docs
   (`Pages`, `Incomplete`, `PagesError`, `Resume`, `Outcome`, `Progress`). This is
   documentation navigation cost, not a failed call. Suggested SDK docs
   improvement: include a concrete cap/resume example and link the relevant
   completion/report methods. No unneeded imports/types or compiler diagnostics
   occurred in the first application attempt.
3. The full package test command briefly reported
   `Blocking waiting for file lock on package cache`, then completed normally.
   No other Cargo command from this consumer was running. This was an environment
   diagnostic, not an SDK failure; no code change or source lookup was needed.

No further application friction occurred. The expanded failure cases also
compiled and passed on their first run. `ErrorKind`, `Resource`, `GateScope`,
`PagesError`, `Resume`, and `Outcome` were needed for explicit failure/completion
assertions; none were required to make an ordinary successful `send()` call.
No misleading compiler diagnostic or surprising generated call name was observed.

## Predictions compared with observations

| Prediction | Observed evidence in `tests/first_use.rs` |
|---|---|
| Construction accepts caller-owned transport and a loopback v10 base, with no I/O | Both tasks use the documented constructor and `with_base_url`; exact final request counts include only endpoint calls. The transport timeout is caller configured. |
| Guild → text channel → history → fetched ID uses the documented method/path/query/result types | Passed. Voice channel was skipped, text channel selected, a two-item page followed by a one-item page returned IDs newest first, and fetching the first ID preserved channel/content/author. No generated request type or trait import was necessary. |
| Bot Authorization, default User-Agent, JSON Accept, no GET body | Every success-path wire request asserted these; the bucket and ambiguous-write fixtures also asserted them. No token occurred in any target. |
| Short pages complete; a full page alone is insufficient | Passed. Three messages took exactly two history requests. An empty history completed with one request. A resumed short page yielded `Ok(None)` repeatedly without I/O and a report with `Outcome::Exhausted`, no resume and no failed cursor. |
| A page cap retains a prefix and a continuation instead of reporting completion | Passed. `max_pages(1)` retained two messages, reported `Bounded`, admitted/succeeded one page and failed none, and kept `Resume::At(MIDDLE)`. `history_from` requested only the remaining page. |
| A failed page retains items, cursor, counters and provider error | Passed. A second-page 429 retained the first two messages and the failed `before=MIDDLE` position; counters were two admitted, one succeeded, one failed. `provider_error()` returned the typed SDK rate error. |
| Same bucket hash does not merge channel resources or let a later response overwrite another channel's gate | Passed. Sales was rate limited, Support succeeded under `same-hash`, and Sales remained locally gated afterwards with its own resource and bucket. Wire count stayed at three after the refused call. |
| Fractional delays, scope and metadata survive 429 handling | Passed. Shared 429 chose JSON 60.25 s over header 0.125 s (and reset-after 30 s); global 429 chose header 60.5 s over JSON 0.125 s. Scope, bucket, response status, raw headers and operation metadata were inspected deliberately. |
| A global 429 gates the bot across routes/clones and performs no retry/sleep | Passed. A cloned guild handle, another channel's fetch, and a POST were all `Gated(Global)` with their own resources. Only the four intended provider requests arrived; the locally gated write was never sent. |
| Non-429 failures preserve HTTP status, Discord code/document and resource | Passed for 400/50035, 401/50014, 403/50013, 404/10008, 503/0. Assertions covered error class, code, status, resource and `X-Request-Id`. Each fixture saw exactly one GET. The 401 client was discarded. |
| A malformed/non-JSON error document cannot override status | Passed for HTML 403: `Api(Forbidden)` retained its status; code was absent and document failure was `Metadata`. |
| Malformed 2xx reads are `Concord(Decode)`; wrong media is `Metadata`; heads survive | Passed for invalid JSON and HTML 200. Both retained HTTP 200 and `X-Request-Id` through `head()`; SDK `status()` also reported 200. |
| Invalid history pages fail explicitly | Passed for newest/oldest ordering violation and more items than the requested limit. Both yielded `Protocol`, retained `Resume::Start`, contributed no items and made exactly one request. A violation specifically relative to a previous page's anchor was not separately exercised. |
| Missing `READ_MESSAGE_HISTORY` may look like an empty channel | The documented empty-array response produced successful exhaustion. A fixture cannot establish actual Discord permissions; this test makes no authorization inference from emptiness. |
| Text creation sends JSON content, string nonce and enforce_nonce, then reads the returned ID | Passed. Fixture asserted POST route, no query, JSON Content-Type and payload fields, stored the message, then served GET by the returned ID. Caller checked ID/channel/content/author/nonce; two requests total. |
| A timeout after accepted creation is ambiguous, with no automatic resend | Passed. Stateful fixture stored the message before delaying its head for 5 s; caller's 2 s timeout returned `AmbiguousWrite`, `accepted=None`, `Transport`, `is_timeout()`. Exactly one POST arrived. Listing by limit=10 found the nonce; GET fetched that ID; three total requests. |
| An undecodable successful POST is ambiguous with accepted status | Passed. Malformed JSON 200 yielded `AmbiguousWrite`, `accepted=Some(200)`, `Decode`, retained `X-Request-Id`, and exactly one POST. |
| Invalid input/headers fail locally and safe formatting excludes secrets | Passed for empty/oversized content/nonce, out-of-range limits and a token containing CRLF. Invalid header produced `Concord(Build)` with no added request. Client/body debug and SDK errors were checked for synthetic tokens, message content, provider text and URL disclosure as applicable. |
| Ordinary calls return values, while errors expose metadata deliberately | Passed. Successes returned `Vec<Channel>`, `Vec<Message>` or `Message`; no success response-head wrapper was expected. History's metadata parameter is `()`. Error metadata was accessible through the documented public fields/methods without `prepare()` or raw execution. |

**Prediction mismatches: none among the exercised predictions.** Broader limits
and error-body interruption/size cases mentioned in the documentation were not
independently exercised by this consumer. General provider permissions and nonce
deduplication are documented contracts, not claims verified against live Discord.
This caller never deliberately resends an ambiguous write.

## Final verification and scope

Frozen baseline: `d188ef771dd5c593ebae665f498b660436f2ad88`.

All Cargo commands used `CARGO_BUILD_JOBS=4`, the default worktree `target/`,
offline mode, and sequential invocation. Generated docs/builds reused the
existing cache; these are not clean-build measurements.

| Command / check | Result |
|---|---|
| Initial `cargo doc --offline -p concord-sdk-discord-rest --no-deps` (without `--locked`) | Passed; lockfile unchanged |
| `cargo doc --offline --locked -p concord-example-testserver -p concord-pagination --no-deps` | Passed; public fixture and completion docs |
| `cargo doc --offline --locked -p concord-core --no-deps` | Passed after first working calls; public error/metadata docs |
| Initial `cargo test --offline --locked -p concord-sdk-discord-rest --test first_use` | 2 passed, 0 failed; success paths, 0.02 s |
| Expanded `cargo test --offline --locked -p concord-sdk-discord-rest --test first_use` | 2 passed, 0 failed; full consumer cases, 2.05 s |
| `rustfmt --edition 2024 examples/discord-rest-sdk/tests/first_use.rs` | Passed; only the new caller file formatted |
| `cargo test --offline --locked -p concord-sdk-discord-rest` | 25 passed (2 unit, 2 fresh caller, 15 protocol, 6 existing workflow); 4 doctests passed; 0 failed |
| `cargo clippy --offline --locked -p concord-sdk-discord-rest --all-targets -- -D warnings` | Passed |
| `RUSTDOCFLAGS='-D warnings' cargo doc --offline --locked -p concord-sdk-discord-rest --no-deps` | Passed |
| Final `cmp Cargo.lock /tmp/discord-first-use-Cargo.lock.before` | Identical bytes; no dependency or package version changes |

Only `FIRST_USE.md` and `tests/first_use.rs` were added. No SDK `src/` changes,
shared crate changes, workspace edits, provenance edits, or commits. Existing
tests were executed as final verification, but their source was never read.
No new crate was needed. Every fixture runs through
`TestServer::start(|request: &Recorded| -> Reply)` (some closures capture state).
Both caller tasks use ordinary generated `send()` and public pagination methods.
Servers are explicitly shut down with zero sockets remaining after shutdown.

Live provider calls: **NOT_RUN**.

## Assistance label

**docs-only**. No SDK `src/`, pre-existing SDK `tests/`, `FRICTION.md`,
`PROVENANCE.md`, or rustdoc source page was read at any point. The only test
source authored/inspected was this consumer's new `tests/first_use.rs`. Public
rustdoc navigation and fixture infrastructure are identified above.
