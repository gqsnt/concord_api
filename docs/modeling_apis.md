# Modeling APIs in Concord v3

Concord makes the domain tree primary while keeping every consequential wire,
security, effect and resource fact inspectable. This guide explains how to
choose owners and declarations, then walks through the canonical extended
Catalog (§22), which is the coverage reference for the language. It is not the
first-contact example; start with the [quick start](quick_start.md).

## Start from caller tasks

Before declaring a large API, write representative tasks:

- build a client for one deployment and security context;
- select one stable domain context (a region, a tenant, an account);
- call a common endpoint and configure an optional or defaulted value;
- handle one expected domain outcome (absence) and one uncertain failure;
- inspect a request without executing it;
- add one exceptional binary, record or streaming endpoint;
- add traversal without changing what one page means;
- identify every endpoint affected by a parent change.

## Choose stable owners

**Client.** Facts stable for the whole contract and security context: the
fixed base, the root timeout, client values, the credential source graph,
reusable rate policies and window sets, common representations and root
query/header/auth attachments.

**Scope.** One selected domain context and its shared subtree behavior:
context values (usually positional), subdomain/path segments, query/header,
auth, rate and timeout deltas, and representation defaults or fences for a
coherent subtree. A scope is a navigation owner the caller holds, not a folder.

**Endpoint.** One operation: exact required inputs and body, the exact awaited
output, a local representation exception, an explicit ID when continuity
matters, local route/query/header changes, accepted and absent statuses, effect,
local auth/rate/timeout changes, pagination and capability terminals.

## Values and presence

Use the Rust type for domain meaning and the DSL markers for Concord semantics:

```concord-pseudocode
mode: Option<Mode>          // required domain decision; None is a payload
mode?: Mode                 // optional slot; absent omits its emissions
mode?: Option<Mode>         // optional slot around an optional payload
#[default] page_size: PageSize   // retained Default, restored by reset
page_size: PageSize = DEFAULT    // retained constant initializer
```

Do not choose `Option<T>` or `Vec<T>` to trigger wire behavior: `?` controls
presence and `each` controls repetition. Place a value directly
(`query category?: Category`) when its key is its name or one fixed string;
add explicit block uses when the same value also feeds other keys. Every value
needs a semantic use, so remove inputs the provider ignores (or keep an optional
one as an explicitly `#[deprecated]` no-op bridge).

## Public atoms

A domain type used in a path, host, query or header either implements
`RequestAtom` (its one canonical protocol spelling) or is adapted explicitly
with `via`:

```concord-pseudocode
path "items/{id via Base32Id}";
query { "at" = time via Rfc3339; }
query { "fields" = fields via CommaSeparated; }
query { append "tag" = each tags via TagSlug; }
```

A scalar adapter joins a collection into one value; `each` emits one
occurrence per item.

## Keys owned by ancestors

A plain `"k" = e` introduces a key and conflicts with any earlier potential
owner. Say what you mean when a parent already owns a key:

- `replace "k" = e` clears the inherited occurrences and emits here, even when
  the new source is absent;
- `when_present "k" = e` overrides only when the local optional is present and
  otherwise keeps the inherited fallback in place;
- `append "k" = e` adds occurrences;
- `remove "k"` removes them;
- `query replace { ... }` / `headers replace { ... }` start the family afresh,
  and `query off;` / `headers off;` drop it.

## Representations

Declare a default where a representation is stable across a coherent owner
(`request Json; response Json;`), write an inline `via` for one exception, and
fence heterogeneous subtrees with `request explicit;` / `response explicit;` so
descendants must choose locally. `explicit` means "choose here", not raw,
disabled, empty or unbounded. See [DSL specification §9](spec.md#9-representation-defaults).

## Status, effect and absence

Keep non-default statuses and effects explicit. `safe`, `idempotent` and
`non_idempotent` describe the provider's requested remote effect; a replayable
body or an idempotency header does not infer it. Pagination requires `safe`.
Use `absent on 404;` for a head-only `Option` outcome, or the buffered
`NotFound<R>` representation when you need the 404 body or accepted bytes (see
[Responses](responses.md)).

## Authentication

Declare credential sources once in `credentials`, then place them with `auth`
blocks where they apply. `auth replace` changes the placement for a subtree and
`auth off` removes it. At most one placement attributes a challenge. An
unavailable credential for a declared placement fails before dispatch; it is
never silently omitted. See [Authentication](auth.md).

## Rate limits

Declare the provider's quota semantics in the contract and select enforcement
in the application. Factor numbers into `windows` sets; give each policy an
explicit ID and ordered key labels; attach policies where their coverage
belongs. Attachments are additive, and a child cannot disable an ancestor
policy. Labels and IDs are state identity: changing them is a reviewed state
reset, not a spelling edit. See [Rate limits](rate_limits.md).

## Pagination

The endpoint's output is one complete page. The controller owns item extraction
and continuation through a public `Init` type and a whole-slot `Cursor`
mapping; its private state can evolve without a DSL edit. See
[Pagination](pagination.md).

## The canonical Catalog

This declaration is compiled in
`crates/concord-examples/src/contracts/catalog_v3.rs`, and
`tests/catalog_v3.rs` and `tests/application.rs` prove its public signatures
and behavior over TLS loopback. Support types (aliases such as
`TenantId = String`, `ApiVersion`, `SessionToken`, the §17.2 envelope
controller) are ordinary Rust in the same module.

<!-- concord-source: crates/concord-examples/src/contracts/catalog_v3.rs#catalog_v3_declaration -->
```concord
api! {
    /// Typed, tenant-scoped Catalog provider client.
    client CatalogApi {
        base "https://catalog.example.test/root/";
        timeout 20s;
        request Json;
        response Json;

        values {
            /// The tenant is sent, but excluded from automatic observation.
            #[sensitive] query tenant: TenantId;
            query locale?: Locale;
            #[default] api_version: ApiVersion;
        }

        credentials {
            secret bootstrap_key;
            credential application_key = api_key(secret.bootstrap_key);
            credential session = endpoint authenticate;
        }

        rate_limits {
            policy tenant_reads(tenant) {
                id "catalog.tenant.read";
                key [origin, "tenant" = tenant];
                allow 120 per 1m;
                allow 5000 per 1h;
            }
        }

        subdomain ["api"];
        path "{api_version}";
        headers { "X-Client-Version" = "3"; }
        auth { header "X-Bootstrap-Key" = application_key; }

        /// Explicit login. Use store_as_session to request local installation.
        POST authenticate(body input: LoginInput) -> SessionToken {
            id "catalog.auth.login";
            path "auth/session";
            effect non_idempotent;
        }

        scope regional(
            region: Region,
            header("Accept-Language") language?: Language,
        ) {
            subdomain [region];
            path "regions/{region}";
            auth replace { bearer session challenge unauthorized; }
            rate_limit tenant_reads(tenant = client.tenant);

            /// One decoded page; select an explicit traversal terminal for more.
            GET item_summaries(
                query category?: Category,
                query offset: u64 = 0,
                query count: NonZeroU32 = DEFAULT_PAGE_SIZE,
                query("tag") each tags: Vec<Tag>,
            ) -> Vec<ItemSummary> {
                id "catalog.items.list";
                path "items";
                paginate via OffsetPagination {
                    init OffsetInit { offset: offset, page_size: count };
                    cursor -> offset;
                }
            }

            /// A strict lookup. Its explicit byte terminal does not decode Json.
            GET item(id: ItemId, locale_override?: Locale) -> Item {
                id "catalog.item.get";
                path "items/{id}";
                query { when_present "locale" = locale_override; }
                expose accepted_bytes;
            }

            /// A head-only 404 is None; the body of that response is not polled.
            GET find_item(id: ItemId) -> Option<Item> {
                id "catalog.item.find";
                path "items/{id}";
                absent on 404;
            }

            /// The provider asserts idempotence for this command-bearing update.
            PUT update_item(
                id: ItemId,
                #[sensitive] header("Idempotency-Key") command_id: CommandId,
                body input: UpdateInput,
            ) -> Item {
                id "catalog.item.update";
                path "items/{id}";
                effect idempotent;
                rate_limit {
                    id "catalog.item.write";
                    key [origin, endpoint, "client.tenant" = client.tenant];
                    cost 5;
                    allow 30 per 1m;
                }
            }

            /// Cursor-envelope page; the controller owns extraction, not Json.
            GET feed(query after?: String, query count: NonZeroU32 = DEFAULT_PAGE_SIZE)
                -> Envelope<ItemSummary>
            {
                id "catalog.item.feed";
                path "feed";
                paginate via EnvelopePages {
                    init CursorStart { after: after };
                    cursor -> after;
                }
            }

            /// Returns a managed byte stream, not a completed export.
            GET export() -> ByteStream via Binary {
                id "catalog.item.export";
                path "export";
            }

            /// One caller-controlled optional is deliberately used three times.
            GET inspect(query language?: Language) -> Item {
                id "catalog.item.inspect";
                path "inspect";
                headers { replace "Accept-Language" = language; }
                query { "languageEcho" = language; }
            }
        }
    }
}
```

### Reading the root

- The base fixes `https://catalog.example.test/root/`. `subdomain ["api"]` and
  `path "{api_version}"` make every generated URL start
  `https://api.catalog.example.test/root/v2/`. `ApiVersion` renders `v2` on
  purpose: it is the provider's wire version, not Concord's major version.
- `timeout 20s` is the root selection; `request Json; response Json;` are the
  directional defaults.
- `tenant` is sent as a query key on every request but withheld from automatic
  observation (`#[sensitive]`). `locale` is an optional query key.
  `api_version` is initialized from `ApiVersion::default()` when the builder is
  created.
- `bootstrap_key` is a required builder secret. `application_key` places it as
  the private `X-Bootstrap-Key` header at the root. `session` is an
  endpoint-backed source that starts empty and is filled only by an explicit
  `authenticate(...).store_as_session()`.
- `tenant_reads` defines numbers and a key schema but covers nothing until an
  attachment binds it.

### Reading the scope

- `regional(region, language?)` prepends the region DNS label and appends
  `regions/{region}`. `language` is a direct `Accept-Language` header.
- `auth replace { bearer session challenge unauthorized; }` replaces the root
  bootstrap placement for the whole scope with the session bearer token; a 401
  is attributed to the applied session version. Because the source is
  endpoint-backed, a challenge never triggers an automatic login.
- `rate_limit tenant_reads(tenant = client.tenant)` covers every regional
  endpoint. Two requests share a bucket only with the same limiter object,
  policy ID, normalized origin and canonical tenant value.

### Reading the endpoints

| Endpoint | What it demonstrates |
|---|---|
| `item_summaries` | Direct placements, an initialized offset and page size, a required repeated `tag` (`each`, positional, no `set_tags`), offset traversal with a public `init` and a whole-slot cursor |
| `item` | `when_present "locale" = locale_override`: absent keeps the inherited root `locale` occurrence in place, present replaces it at the end; `accepted_bytes()` bypasses Json decoding |
| `find_item` | Head-only absence: a 404 resolves to `None` and its body is dropped unread |
| `update_item` | An exact body, a sensitive idempotency header, an asserted `idempotent` effect, and an inline write policy keeping the legacy label `"client.tenant"`; its `endpoint` dimension consumes the pinned ID `catalog.item.update` |
| `feed` | A custom envelope controller (§17.2): `Json` decodes the whole `Envelope`, the controller extracts items |
| `export` | `ByteStream via Binary`: `.await` returns an opened managed stream, not a completed download |
| `inspect` | One optional used three times: a direct `language` query key, an explicit `languageEcho` key and `replace "Accept-Language"`, so omission clears the scope's inherited header |

### The effective view of one endpoint

The static contract (`CatalogApi::contract()`) and the generated rustdoc
express facts equivalent to this expanded view of `item_summaries` (§18.5):

```text
CatalogApi::regional::item_summaries
  id: catalog.items.list (explicit)
  GET https://{regional.region}.api.catalog.example.test/root/{client.api_version}/regions/{regional.region}/items
  query:
    tenant <- client.tenant, required; sensitive                  [client]
    locale <- client.locale, omitted when absent                 [client]
    category <- endpoint.category, omitted when absent           [endpoint]
    offset <- endpoint.offset, initialized 0, page-controlled     [endpoint]
    count <- endpoint.count, initialized DEFAULT_PAGE_SIZE       [endpoint]
    tag <- each endpoint.tags, required; empty emits no values    [endpoint]
  auth: bearer session; root bootstrap placement replaced        [scope regional]
        endpoint-backed source; a challenge does not auto-login
  quota: catalog.tenant.read; key origin + label "tenant"         [scope regional]
         120/1m AND 5000/1h; cost 1; coverage shown separately
  one-response output: Vec<ItemSummary> via Json                 [client default]
  traversal: OffsetPagination; Init(offset,count), Cursor -> offset
  timeout: 20 seconds for the whole operation/traversal           [client]
```

The `endpoint.` prefixes in this view are display provenance, not source
syntax. See [Planning and observation](planning_and_observation.md).

### Using it

The application examples in `crates/concord-examples/src/application/` are the
ordinary caller jobs over this client, each documented where its subject lives:
construction and explicit sign-in ([Authentication](auth.md)), the generic
`execute` helper and saved-state controls ([Generated client](generated_client.md)),
search without lying about exhaustion ([Pagination](pagination.md)), a bounded
export prefix ([Streams](streams.md)) and write reconciliation
([Execution policy](execution_policy.md)).

## Review a parent change

A change to a parent representation, query/header/auth/rate attachment, timeout
or sensitivity can affect many leaves. Review the semantic diff
(`concord-xtask contract diff`) rather than only the textual edit: a hoisted rate
attachment that newly covers a sibling is a behavior change even when the source
got shorter. A parent default is justified when it belongs to a coherent
subtree, is inspectable in the effective contract, and does not hide an
exception.

## Scale

The [Riot walkthrough](riot_walkthrough.md) models 46 endpoints across regional
hosts, preserving every original rate policy and sharing group through the
meaning-preserving migration. It is a scale and domain example, not a current
provider quota guarantee.
