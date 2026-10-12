# Migrating from Concord v2.1 to v3

**Status:** the migration guide for the unpublished `3.0.0-beta.1` line and the
`migrate-v3` tool shipped in `concord-xtask`. It follows the selected v3
contract (implementation specification §25, with §3-§6, §12.1, §14.1 and §24
for the grammar details). Where this guide and the specification disagree, the
specification wins. `docs-check` verifies that the usage line and the
diagnostic table below match the tool exactly. The v2.0 to v2.1 guide is kept
separately as [historical](migration_v2_1.md).

V3 is a major version. It deliberately breaks source compatibility in a small
number of places and never keeps two meanings under one spelling: a clean v3
parser accepts only canonical v3, and an old form that it recognizes produces a
diagnostic rather than silently keeping its old meaning. Migration therefore
has three kinds of change:

1. **Mechanical changes** keep the exact v2.1 meaning under the new spelling.
   The migration tool performs them.
2. **Behavioral and capability changes** are deliberate v3 contract changes.
   They need review even when the source still compiles.
3. **Unautomatable decisions** need information the tool does not have
   (application intent, type information, provider facts). The tool reports
   each site with its location and a suggested v3 spelling and leaves the
   original source in place.

## Translation table

The complete §25.2 translation, with the section below that details each row:

| v2.1 form or contract | v3 replacement | Important effect |
|---|---|---|
| `endpoint.x` or current-scope qualification | the bare name after owner resolution ([Bare local names](#bare-local-names-after-owner-resolution)) | no nearest-name lookup |
| ancestor `client.x` / `scope.x` | the exact qualification, kept | a parent insertion cannot capture it |
| path atom list | a relative template lowering the same segments and adapters ([Path atom lists](#path-atom-lists-become-relative-templates)) | literal braces escaped; base and fences kept |
| DNS atom list | the list, kept | no host-template rewrite |
| plain `"k" = e` over an earlier key | `replace "k" = e` | preserves the old exact-clearing law |
| plain `"k" = e` with no earlier owner | an introduction or a direct placement | source and order preserved |
| `when_present`, `append`, `remove` | the same operations under v3 preconditions | pointless missing-target uses need cleanup |
| optional `set_x(opt)` | `replace_x(opt)` first ([Optional setters](#optional-setters-migrate-to-exact-replacement-first)) | exact outer-slot replacement, nested `Option` included |
| payload `set_x(Some(v))` | may later be simplified to `set_x(v)` | never rewrite nested `None` |
| required initialized `set_x`/`reset_x` | the same names, narrowed conversions | reset restores the retained value |
| `set_x(None)` on `x?: Option<U>` | `replace_x(None)` or `omit_x()` | v3 `set_x(None)` is a present inner `None` |
| named/inline rate keys | explicit logical labels with the legacy bytes ([Rate keys](#rate-keys-get-explicit-labels-preserving-the-legacy-bytes)) | IDs alone do not prove continuity |
| repeated same-ID attachments | identical structural deduplication | conflicting repetitions are errors |
| controller public-field initialization | a public Init type and a whole-cursor mapping ([Built-in pagination](#built-in-pagination-uses-a-public-init-value-and-a-whole-cursor-mapping)) | private state can evolve |
| response `fn accept()` | `const ACCEPT: Option<&'static str>` | static negotiation metadata |
| runtime body/page NonZero setters | primitive-count setters, `.get()` ([Runtime configuration](#runtime-configuration-counts-take-primitives)) | aggregate validation at build |
| `.paginate(Prefix(n))` | `.collect_prefix(n)` ([Traversal terminals](#traversal-terminals)) | output type changes (or `.into_items()` is appended) |
| `.paginate(CompleteWithin(n))` | `.collect_all(NonZeroU32)` | the completion limit stays an error |
| no lazy traversal | `.pages(n)` | one aggregate deadline including pauses |
| per-request `.debug(level)`, builder `c_provider(p)` | none (runtime-wide `debug`); `c(provider)` ([Names v3 no longer generates](#names-v3-no-longer-generates)) | reported, never rewritten: an application trait method of the same name may be selected |
| dual body metadata and `prepare` | `PreparedBody` owns Content-Type, length and replay facts | plans run no metadata callbacks |
| Json page-marker iterable bound | the buffered driver plus a typed controller | envelope pages work |
| unconditional `store_as_*` installation | `StoreOutcome` and latest-started intent | callers handle `Superseded` |
| errors defaulting to `NotDispatched` | journal-derived aggregate, last-attempt and response facts | no false no-send guarantee |
| runtime root bare binding | the full root owner projection | wire/auth/quota results may change |
| runtime arbitrary credential-source selection | declared owner auth boundaries; runtime keeps or removes placements | intentional capability restriction |
| body-bearing request `Clone` | a request factory; page stability is separate | no blanket `Clone` |
| status-coded `Option` custom code | the buffered adapter kept, or head-only `absent on` chosen deliberately | acquisition and accepted-byte capabilities differ |

Migrate first for meaning, then clean up. A meaning-preserving conversion keeps
endpoint IDs, rate policy IDs, key labels, windows, attachment locations and
representation fences exactly; showcase cleanups (factoring window sets,
renaming, hoisting quotas) are separate, reviewed provider-contract edits.

## The migration tool

```text
cargo run -p concord-xtask -- migrate-v3 --input <file-or-dir> [--check | --diff | --apply --output <dir>] [--json] [--report <file.json>] [--keep-explicit-blocks]
```

| Mode | Effect |
|---|---|
| `--diff` (default) | Prints a unified diff of every automated edit on stdout; diagnostics and a summary go to stderr. Nothing is written. |
| `--check` | Prints diagnostics and the summary only. Nothing is written. |
| `--apply --output <dir>` | Writes an isolated copy of the whole input tree, with converted files, into `<dir>`. |
| `--json` | Prints the machine-readable report on stdout instead of the diff/summary. |
| `--report <file>` | Also writes the machine-readable report to a file. |
| `--keep-explicit-blocks` | Keeps every query/header statement explicit instead of converting eligible ones to direct placement. |

The exit status is `0` when no unautomatable site remains, `1` when at least
one unautomatable site remains (in every mode), and `2` for usage or I/O
errors.

Safety properties:

- The input tree is never modified. `--apply` refuses an output directory that
  is inside or equal to the input tree, one that contains the input tree, and
  one that already has entries. `--report` refuses a path inside the input tree.
- Every Rust file is parsed with `syn`. `api!` invocations are located by macro
  path (`api`, `concord::api`, or a renamed import of it) proved to name
  Concord's macro (an `api`-named invocation that is not is reported; see
  [Macros](#macros)), and each declaration is parsed by a real v2.1 grammar
  parser. Decisions come
  from the resolved owner graph (client, scopes, endpoints and their values),
  never from textual pattern replacement.
- Edits are exact byte replacements of the tokens involved. Comments,
  formatting and every untouched token stay byte-for-byte identical.
- Caller code is migrated only from declarations found in the same crate (the
  nearest directory with a `Cargo.toml`). When a single file is given, the rest
  of its crate is read, but not written, to resolve declarations.
- A caller edit that depends on a receiver (a `set_x` rename, a `paginate`
  terminal, a runtime `NonZero` argument) is made only when the receiver's
  identity is proved: its type path resolves, through the crate's modules,
  items, `use` imports and generic parameters, to a type generated by one of
  the crate's `api!` declarations. Concord types named in the call (such as
  `PageLimit`) must resolve to Concord's own. A last path segment or a field
  name alone is never proof. A call reached only through an auto-deref or
  auto-ref step is also proved free of trait methods the tool cannot see,
  a site that a `#[cfg]` may change is uncertain, and nothing inside a macro
  is rewritten except the arguments of the standard expression macros.
  Everything unproved is reported, never rewritten. See
  [Receiver identity](#receiver-identity).
- The report captures, before conversion, every endpoint's v2.1 and v3
  operational ID and its complete effective rate plan (policy IDs, costs,
  windows and ordered key labels). Keep it with the migration change for review.

### Diagnostic codes

Only `error` diagnostics are unautomatable sites. `warning` marks an automated
or informational site whose behavior or type changes. `note` records an
automated mechanical change.

| Code | Severity | Meaning and action |
|---|---|---|
| `C3-MIGRATE-PARSE-RUST` | error | The file is not valid Rust; fix it and rerun. |
| `C3-MIGRATE-PARSE-DECLARATION` | error | An `api!` body is not valid v2.1; make it compile under v2.1 first. |
| `C3-MIGRATE-ALREADY-V3` | note | The declaration already uses v3-only syntax and was left unchanged. |
| `C3-MIGRATE-UNRESOLVED-REFERENCE` | error | An owner-qualified reference does not resolve; the source was left unchanged. |
| `C3-MIGRATE-RESERVED-OWNER-NAME` | error | A scope is named `client`, `endpoint` or `__concord*`; rename it and pin IDs. |
| `C3-MIGRATE-PATH-ADAPTER-BRACES` | error | A path adapter type contains braces; name it with a type alias. |
| `C3-MIGRATE-PATH-ADAPTED-LITERAL` | error | A path adapter is applied to a literal or `concat`; use a declared value. |
| `C3-MIGRATE-PATH-LITERAL` | error | A literal segment contains `?`, `#`, a separator, or is empty/dot. |
| `C3-MIGRATE-KEY-REPLACE` | note | A plain assignment over an earlier owner became `replace`. |
| `C3-MIGRATE-SENSITIVITY-STICKY` | warning | A key that carried a `#[sensitive]` value stays redacted after a non-sensitive override. |
| `C3-MIGRATE-ROOT-DEFAULT-TIMING` | warning | A root `#[default]` value's `Default::default()` now runs once when the builder is created, not inside `build()`; review defaults with side effects. |
| `C3-MIGRATE-WHEN-PRESENT-NO-FALLBACK` | error | `when_present` has no earlier owner; write a plain introduction. |
| `C3-MIGRATE-REMOVE-NO-OWNER` | error | `remove` targets a never-owned key; delete it or restore the owner. |
| `C3-MIGRATE-RATE-WINDOW-DUPLICATE` | error | A policy repeats a window duration; keep one capacity per duration. |
| `C3-MIGRATE-RATE-UNKNOWN-POLICY` | error | A named rate attachment references an undefined policy. |
| `C3-MIGRATE-RATE-KEY-ENCODER` | error | A rate dimension adapter must become a `RateKeyEncoder` with `ENCODING_ID`. |
| `C3-MIGRATE-PAGINATION-INIT` | note | Built-in controller fields became a public Init value and a whole-cursor mapping. |
| `C3-MIGRATE-PAGINATION-CUSTOM-CONTROLLER` | error | A custom controller needs a public Init type and Cursor mapping. |
| `C3-MIGRATE-PAGINATION-FIELD` | error | A built-in controller binding cannot be mapped (unknown field, unbound cursor). |
| `C3-MIGRATE-ID-PIN` | note | The v2.1 endpoint ID was pinned with an explicit `id`. |
| `C3-MIGRATE-ID-INVALID` | error | An explicit ID is required but the v2.1 ID is not a valid v3 ID. |
| `C3-MIGRATE-SETTER-REPLACE` | note | An optional-value `set_x` call became `replace_x`. |
| `C3-MIGRATE-SETTER-UNRESOLVED` | error | A `set_x` call may target an optional value, but its receiver is not proved to be a generated type (unresolved, or resolved to another type); the call was left unchanged. Rename it to `replace_x` by hand if it is the Concord value. |
| `C3-MIGRATE-SETTER-AMBIGUOUS` | error | A `set_x` call resolves to both optional and required values. |
| `C3-MIGRATE-PAGINATE-COLLECT-ALL` | note | `.paginate(PageLimit::CompleteWithin(n))` became `.collect_all(n)`. |
| `C3-MIGRATE-PAGINATE-PREFIX` | warning | `.paginate(PageLimit::Prefix(n))` became `.collect_prefix(n)`; the output type changes. |
| `C3-MIGRATE-PAGINATE-DYNAMIC` | error | `.paginate` receives a runtime `PageLimit`; choose an explicit terminal. |
| `C3-MIGRATE-PAGINATE-UNRESOLVED` | error | A `.paginate(..)` call is not proved to be the generated traversal of Concord's `PageLimit` (its receiver is unresolved or another type, or its `PageLimit` is unproved); the call was left unchanged. If it is Concord's, select `.collect_prefix(n)`, `.collect_all(n)` or `.pages(n)` by hand. |
| `C3-MIGRATE-PAGELIMIT-IMPORT` | note | An unused `PageLimit` import was removed. |
| `C3-MIGRATE-PAGELIMIT-REFERENCE` | error | `PageLimit` no longer exists and is still referenced. |
| `C3-MIGRATE-RUNTIME-NONZERO` | note | A runtime configuration NonZero argument now passes `.get()`. |
| `C3-MIGRATE-RUNTIME-NONZERO-UNRESOLVED` | error | A possible runtime configuration setter has an unresolved receiver. |
| `C3-MIGRATE-ACCEPT-CONST` | error | A response representation's `fn accept()` must become `const ACCEPT`. |
| `C3-MIGRATE-STORE-OUTCOME` | error | A `store_as_*` caller must handle `StoreOutcome::Superseded`. |
| `C3-MIGRATE-REQUEST-REPRESENTATION` | error | Exact media facts move from `metadata()` into `PreparedBody`. |
| `C3-MIGRATE-RATE-KEY-PART` | error | A `RateKeyPart` implementation must name its encoding with `ENCODING_ID`. |
| `C3-MIGRATE-RATE-LIMITER-ATOMIC` | error | A custom `RateLimiter` must admit complete plans atomically and describe its enforcement. |
| `C3-MIGRATE-PROVIDER-ORIGINS` | warning | A custom `CredentialProvider` using `ProviderHttp` must declare `http_origins`. |
| `C3-MIGRATE-PAGINATION-CONTROLLER-IMPL` | error | A custom `PaginationController` needs `Init`, `Cursor`, `start` and `cursor`. |
| `C3-MIGRATE-PAGE-RESPONSE-REPRESENTATION` | error | `PageResponseRepresentation` was removed. |
| `C3-MIGRATE-PRELUDE-IMPORT` | note | A declaration type v2.1 resolved through the facade prelude is now imported explicitly. |
| `C3-MIGRATE-PRELUDE-IMPORT-UNRESOLVED` | error | A declaration type needs a facade prelude import the tool cannot place; add the `use` by hand. |
| `C3-MIGRATE-GENERATED-SHADOW` | error | A declaration path would re-resolve through a v3 generated name or a relative `self`/`super` prefix; write an absolute `crate::` path or import it under another name. |
| `C3-MIGRATE-GENERATED-NAME-SHADOW` | error | A call (or function value) may reach a method that v3 generates on a generated type and v2.1 did not (`replace_x`, `set_x_if_some`, `bind`, `with_options`, ...), so the application method it reached in v2.1 (a trait method of the same name) would be silently replaced by the generated inherent method; see [Names only v3 generates](#names-only-v3-generates). Left unchanged: call the application's method explicitly (`Trait::name(receiver, ..)`) or rename it. |
| `C3-MIGRATE-GENERATED-MEMBER-REMOVED` | error | A call (or function value) may reach a method that v2.1 generated on a generated type and v3 no longer generates there (the bound request's `debug(DebugLevel)`, the builder's `c_provider` setter of a custom credential `c`). v3 has no inherent method of that name, so the unchanged call fails to compile, or, when an application trait in scope provides a method of that name for the type, silently calls that trait method; see [Names v3 no longer generates](#names-v3-no-longer-generates). The message names the removed member, its role and the v3 replacement if there is one. Left unchanged: remove the call, replace it with the v3 form, or call the application's method explicitly (`Trait::name(receiver, ..)`). |
| `C3-MIGRATE-GENERATED-NAME-COLLISION` | error | A declaration's values or children produce the same v3 generated name twice on one generated type (for example `x?` and `x_if_some?` both give `set_x_if_some`), which v3 rejects; rename one of them. |
| `C3-MIGRATE-DEAD-VALUE` | error | A value has no wire, route, rate-key or pagination use; remove it, give it a use, or mark an optional one `#[deprecated]` as a no-op bridge. |
| `C3-MIGRATE-PAGINATE-PREFIX-ITEMS` | note | An awaited `.paginate(PageLimit::Prefix(n))` became `.collect_prefix(n)` followed by `.into_items()`, keeping the `Vec` type. |
| `C3-MIGRATE-PAGINATED-REQUEST-TYPE` | note | A `PaginatedRequest<_>` annotation became the selected v3 terminal type. |
| `C3-MIGRATE-PAGINATED-REQUEST-REFERENCE` | error | `PaginatedRequest` no longer exists and is still referenced. |
| `C3-MIGRATE-BUILDER-PATH` | note | A reference to the generated `ClientBuilder` now names it in the generated support module (`client::ClientBuilder`). |
| `C3-MIGRATE-BUILDER-PATH-UNRESOLVED` | error | A reference that may name the generated `ClientBuilder` could not be moved to the support module (its target is unresolved, or the support module name does not resolve at that place); the reference was left unchanged. Name it `client::ClientBuilder` by hand, or use `Client::builder()`. |
| `C3-MIGRATE-API-MACRO-UNRESOLVED` | error | A macro invocation whose path ends in `api` (`api!`, `some::path::api!`) is not proved to be Concord's `api!`: its path names a crate-local `macro_rules!`, another crate's macro, or an import or glob the tool cannot follow; a bare `api!` without an import is not backed by `#[macro_use] extern crate concord` at its crate root; or `concord::` may name a crate-local module there. It was not migrated; migrate it by hand if it is Concord's. |
| `C3-MIGRATE-DEPENDENCY-IDENTITY` | error | The package's `Cargo.toml` does not establish that the crate names `concord`, `concord_core`, `concord_extras` and `concord_macros` denote the genuine Concord packages (see [Dependency identity](#dependency-identity)); the file was left unchanged and nothing in it was migrated. The message gives every reason. Declare the Concord packages under their own crate names and rerun, or migrate by hand. |
| `C3-MIGRATE-NESTING-LIMIT` | error | The file nests delimiters (`()`, `[]`, `{}`) and generic-argument brackets (`<`) together more than 256 levels deep, the bound the tool examines; the file was left unchanged and was not scanned. Migrate it by hand. |
| `C3-MIGRATE-INTERNAL` | error | The tool could not apply a change consistently; report it. |

## Mechanical changes (automated)

### Bare local names after owner resolution

A bare name now means the current owner only. The tool resolves every v2.1
reference first, then removes redundant qualification of the current owner
(`endpoint.x` in an endpoint, `regional.x` in scope `regional`, `client.x` in
the client). References to an ancestor keep their exact qualification, so a
later same-named ancestor insertion cannot capture them.

```text
// v2.1
scope regional(region: Region, language?: Language) {
    subdomain [regional.region];
    GET item(id: ItemId) -> Item {
        path ["items", endpoint.id];
        rate_limit tenant_reads(tenant = client.tenant);
    }
}

// v3
scope regional(region: Region, language?: Language) {
    subdomain [region];
    GET item(id: ItemId) -> Item {
        path "items/{id}";
        rate_limit tenant_reads(tenant = client.tenant);
    }
}
```

Rust constant paths are not references and are never touched:
`count: u32 = crate::defaults::PAGE` and `page_size = defaults::SIZE` keep their
exact qualification even beside a scope named `defaults`.

### Path atom lists become relative templates

Each v2.1 path atom was exactly one segment; v3 writes the same segments as one
relative template. Literal braces are escaped (`{{`, `}}`), references become
holes, adapters stay attached to their hole, and `concat` becomes one segment.
`path replace` and `path off` keep their fence meaning.

```text
// v2.1
path ["champion-masteries", "by-puuid", endpoint.puuid, "by-champion", endpoint.champion_id];
path replace ["legacy", client.api_version];
path ["reports", endpoint.date via IsoDate];
path ["a{b}", concat ["resource-", endpoint.id]];
path off;

// v3
path "champion-masteries/by-puuid/{puuid}/by-champion/{champion_id}";
path replace "legacy/{client.api_version}";
path "reports/{date via IsoDate}";
path "a{{b}}/resource-{id}";
path off;
```

A template hole cannot contain a brace-delimited const argument. The tool
reports `C3-MIGRATE-PATH-ADAPTER-BRACES` and suggests an alias:

```text
type PathAdapter = crate::Fixed<{ 1 + 1 }>;
path "fixed/{id via PathAdapter}";
```

### DNS labels stay lists

`subdomain [...]`, `subdomain replace [...]` and `subdomain off` keep their
list form; only reference qualification changes. No host template is written.

```text
// v2.1
subdomain replace [platform.platform, "api"];
// v3
subdomain replace [platform, "api"];
```

### Plain assignment over an earlier owner becomes `replace`

In v2.1, `"k" = e` removed every earlier occurrence and emitted at the
statement. In v3 a plain `"k" = e` is an introduction and is rejected when an
earlier potential owner exists. The tool computes the potential owners along
the owner chain (inherited and earlier in the same block, with family
`replace`/`off` fences and `remove` taken into account; header names compare
case-insensitively) and writes `replace` exactly where the old clearing law
applied.

```text
// v2.1
client Api {
    values { locale?: String; }
    query { "locale" = client.locale; "a" = "1"; "a" = "2"; }
    scope regional(locale?: String) {
        query { "locale" = regional.locale; }
    }
}

// v3
client Api {
    values { query locale?: String; }
    query { "a" = "1"; replace "a" = "2"; }
    scope regional(locale?: String) {
        query { replace "locale" = locale; }
    }
}
```

### Plain introductions may become direct placement

A plain introduction of a local value can be written as a direct placement on
the value declaration. v3 normalizes each family as: inherited program (or
family replacement), then direct placements in value declaration order, then
explicit statements in source order. The tool therefore converts only the
leading run of plain introductions of local values whose values appear in
declaration order, so no mixed operation is reordered. A value already placed
stays explicit in its other uses. `--keep-explicit-blocks` disables this.

```text
// v2.1
GET match_ids_by_puuid(
    puuid: String,
    start_time?: i64,
    queue?: u32,
    start: u64 = 0,
) -> Vec<String> {
    path ["by-puuid", endpoint.puuid, "ids"];
    query {
        "startTime" = endpoint.start_time;
        "queue" = endpoint.queue;
        "start" = endpoint.start;
    }
}

// v3
GET match_ids_by_puuid(
    puuid: String,
    query("startTime") start_time?: i64,
    query queue?: u32,
    query start: u64 = 0,
) -> Vec<String> {
    path "by-puuid/{puuid}/ids";
}
```

Query key case is preserved exactly (`startTime`); no key is re-derived from
the Rust name unless it already equals it. A mixed block is left as-is from the
first ineligible statement on:

```text
// v2.1
GET mixed(a?: String, b?: String) -> String {
    query { "b" = endpoint.b; "a" = endpoint.a; append "z" = "1"; }
}

// v3 (same wire order b, a, z)
GET mixed(a?: String, query b?: String) -> String {
    query { "a" = a; append "z" = "1"; }
}
```

### `when_present`, `append` and `remove` keep their spelling

They keep their meaning, subject to v3 structural preconditions. A
`when_present` with no earlier owner and a `remove` of a never-owned key were
pointless in v2.1 and are errors in v3; the tool reports
`C3-MIGRATE-WHEN-PRESENT-NO-FALLBACK` / `C3-MIGRATE-REMOVE-NO-OWNER` instead of
deleting anything.

### Rate keys get explicit labels, preserving the legacy bytes

Custom key labels are state-affecting and written explicitly in v3. A named
policy formal `tenant` keeps the label `tenant`; an inline v2.1 dimension
`client.tenant` keeps its owner-qualified label `client.tenant` byte-for-byte.
`origin`, `endpoint`, IDs, windows, costs and attachment locations are not
changed, equal-number policies are not merged, and `cost 1;` is kept. The
header order becomes the canonical `id; key; cost;`.

```text
// v2.1
policy tenant_reads(tenant) {
    id "catalog.tenant.read";
    key [origin, tenant];
    cost 1;
    allow 120 per 1m;
}
rate_limit { id "catalog.item.write"; key [origin, endpoint, client.tenant]; cost 5; allow 30 per 1m; }
rate_limit { id "concord.riot.1"; cost 1; key [origin]; allow 500 per 10s; }

// v3
policy tenant_reads(tenant) {
    id "catalog.tenant.read";
    key [origin, "tenant" = tenant];
    cost 1;
    allow 120 per 1m;
}
rate_limit { id "catalog.item.write"; key [origin, endpoint, "client.tenant" = client.tenant]; cost 5; allow 30 per 1m; }
rate_limit { id "concord.riot.1"; key [origin]; cost 1; allow 500 per 10s; }
```

`"client.tenant" = client.tenant` and `"tenant" = client.tenant` are different
schemas even with equal values; do not "tidy" a legacy label during migration.

### Built-in pagination uses a public Init value and a whole-cursor mapping

For `OffsetPagination`, `PagePagination`, `CursorPagination<C>`,
`NextHeaderPagination` and `LinkCursorPagination`, the v2.1 field bindings map
to the public Init type (`OffsetInit`, `PageInit`, `CursorInit<C>`,
`CursorInit<String>`) and the continuation field's live slot becomes the
`cursor ->` destination. The Init path reuses the controller's qualification;
import the Init type next to an unqualified controller.

```text
// v2.1
paginate via concord::pagination::OffsetPagination {
    offset -> endpoint.start;
    page_size -> endpoint.count;
}

// v3
paginate via concord::pagination::OffsetPagination {
    init concord::pagination::OffsetInit { offset: start, page_size: count };
    cursor -> start;
}
```

The built-in controllers never change `page_size`, so the old write-back of an
unchanged page size is equivalent to the init-only field.

### Endpoint IDs are pinned when the v3 derivation would differ

v3 derives `snake_case(Client).scope_path.endpoint`. The client segment now
follows CALL-01 (for example `Api2URL` derives `api2_url`, where v2.1 derived
`api2url`). When the v3 derivation differs from the v2.1 one, is not a valid
v3 operational ID, or an effective policy uses the `endpoint` key dimension
(which requires an explicit ID in v3), the tool inserts `id "<v2.1 ID>";`.

```text
// v2.1: client Api2URL { ... scope braces(..) { GET keyed() -> String; } }
GET keyed() -> String;

// v3
GET keyed() -> String {
    id "api2url.braces.keyed";
}
```

### Optional setters migrate to exact replacement first

A v2.1 `set_x(Option<T>)` on a Concord-optional value (`x?: T`, including an
optional body) was an exact outer-slot replacement. In v3 that is `replace_x`;
`set_x(v)` now sets a present payload. The tool rewrites a call only after
proving its receiver (builder chains, client/scope navigation, local
bindings, typed parameters, fields, function results, fully qualified calls
and constructors) is the declaring owner in the same crate; see
[Receiver identity](#receiver-identity). A same-named setter on a required
value, or on an unrelated type, is not renamed.

```text
// v2.1, for milestone?: Option<u32>
request.set_milestone(None);         // outer slot absent
request.set_milestone(Some(None));   // present inner None

// v3 (exact migration)
request.replace_milestone(None);
request.replace_milestone(Some(None));
```

After the exact migration, `replace_x(Some(v))` may be simplified by hand to
`set_x(v)`. Never simplify `replace_x(None)` to `set_x(None)`: for
`x?: Option<U>` that means a present inner `None`.

Whether a `set_*` call is a generated setter is decided by the declared values
and the methods v2.1 generates for them, never by the shape of the name. v2.1
generates no conditional setters, so `set_x_if_some(..)` is the exact setter
of a declared value named `x_if_some` (migrated to `replace_x_if_some` like
any other optional value, or reported when its receiver is not proved), and
for any other declaration it is not a v2.1 generated method at all: such a
call reached an application method in v2.1, and it is never renamed. When v3
generates that name for the receiver (the conditional setter `set_x_if_some`
of a declared value `x`), the call is reported, see
[Names only v3 generates](#names-only-v3-generates). The v3 conditional
setter `set_x_if_some` of a value `x` is never produced by the migration.

Required initialized values keep `set_x`/`reset_x`, with the narrowed v3
conversion set (identity, `&str -> String`, `&String -> String`). Domain
newtypes are constructed explicitly.

### Names only v3 generates

v3 generates inherent methods that v2.1 did not generate on the same type:

| Generated type | v3-only methods |
|---|---|
| client builder | `set_x_if_some` for every client value `x`; `replace_x` for an optional one; `c` (the provider setter, v2.1 `c_provider`) for a custom credential `c` |
| scope handle and unbound scope | `set_x_if_some` for an optional or initialized scope value `x`; `replace_x` for an optional one |
| request and unbound intent | the same for endpoint values, and `set_b_if_some`/`replace_b` for an optional body `b` |
| client and scope handles | `bind`, `plan_runtime` |
| request | `cancel_with`, `with_options`, `control_summary`, `description` (v2.1 generated `description()` only on the unbound intent); `pages`, `collect_prefix`, `collect_all` for a paginated endpoint |

A name v2.1 also generated for that type (for example `set_x_if_some` as the
setter of a declared value `x_if_some`, or `description` on the unbound
intent) is not v3-only. An application trait
implemented for a generated type (in the crate or in another crate) may
already provide a method of such a name, and v2.1 calls reached it. In v3 the
generated inherent method takes precedence over a trait method of the same
name, so the unchanged call would silently call the generated method instead:

```text
// v2.1: ClearOnNone::set_x_if_some(None) clears x (an application trait)
client.item().set_x(Some(3)).set_x_if_some(None)   // -> /item
// unchanged in v3: the generated conditional setter ignores None
client.item().replace_x(Some(3)).set_x_if_some(None) // -> /item?x=3
```

Such a site is never rewritten and never silent: it is reported with
`C3-MIGRATE-GENERATED-NAME-SHADOW` (an error, naming the trait method the
crate implements for that type when the tool sees one) and left unchanged.
Migrate it by hand: call the application's method explicitly
(`ClearOnNone::set_x_if_some(request, None)` or
`<Type as ClearOnNone>::set_x_if_some(..)`, which name the trait's method in
both versions) or rename the trait method. Reported are:

- a method call whose receiver is proved to be such a generated type (also
  where auto-deref reaches it: a `self` method such as `set_x_if_some`
  through a `Box`, a `&self` method such as `bind` through any reference or
  smart pointer), or is not proved to be or not to be a generated type (an
  unknown receiver, as for [Receiver identity](#receiver-identity)); for a
  core method (`bind`, `pages`, ...) an unknown receiver is reported only in
  a file that uses Concord, as for `.paginate(..)`, while a value control
  names a declared value and is reported anywhere in the crate, also when
  another crate of the migration run declares that value;
- a type-qualified call (`Type::set_x_if_some(request, v)`,
  `<Type>::set_x_if_some(..)`, `Self::set_x_if_some(..)` inside an `impl` of
  the generated type: a type path finds an inherent function first) and the
  same path named as a function value;
- such a call inside a macro the tool does not expand (see
  [Macros](#macros));
- a receiver of another crate's type when another crate of the migration run
  generates that name.

This table is guarded against drift from the generators: the repository test
`migration::tests::surface_parity` compares the tool's per-type v3-only
inventory with the public inherent members the real generators emit for a
representative declaration covering every capability condition
(`tools/xtask/fixtures/generated_surface/`: a frozen member list derived from
the v2.1 generator, and a v3 member snapshot that the `concord-macros` test
`generated_surface::generated_public_members_match_the_migrator_snapshot`
keeps current, which also checks that the fixture reaches every member
emission site). A method the v3 generator newly emits fails those tests until
this inventory (and table) accounts for it. The v3 credential facade
(`Credentials`, which replaced v2.1's `credentials::<Client>Credentials` and
its handle types) is outside the tool's type model: its methods are new, but
no v2.1 application implementation names that type, and v2.1 calls through
`credentials()` no longer compile.

Not reported: a trait-qualified call (`Trait::name(..)`,
`<Type as Trait>::name(..)`), a receiver proved not to be a generated type (a
crate-local struct, another type the paths resolve to, an `Option` or
`Result`), and a call whose argument count the generated method cannot take
(v3 then fails to compile instead of changing meaning). A generic receiver
`T: Trait` only sees its bounds, but the tool does not model generic
receivers, so such a call is reported as an unknown receiver.

A declaration whose values or children produce the same v3 generated name
twice on one generated type is rejected by v3 (generated names are never
suffixed or truncated). Because v3 generates names v2.1 did not, a
declaration v2.1 accepted may collide: `GET item(x?: u8, x_if_some?: u8)`
gives `set_x_if_some` both as the conditional setter of `x` and as the
setter of `x_if_some`, and an endpoint or scope named `bind` collides with
the handle's `bind`. (A collision v2.1 already rejected, such as an endpoint
named `plan`, is reported the same way.) The tool reports it at the later
declaration with
`C3-MIGRATE-GENERATED-NAME-COLLISION` (an error) and converts the declaration
as usual; rename one of the two (and its callers) to migrate it. The check
covers value controls, child navigation methods, the optional body and the
generated core methods of the client builder, the client and scope handles
and the request; it does not cover secret or custom-provider builder methods
or credential storage terminals.

### Names v3 no longer generates

The reverse change matters as well: v2.1 generated inherent methods that v3
no longer generates on the same type. A v2.1 call reached the generated
method even when an application trait implemented for that type had a method
of the same name, because an inherent method takes precedence. In v3 there is
no inherent method of that name, so the unchanged call either fails to
compile or, when such an application trait is in scope, silently calls the
trait method instead:

```text
// v2.1: the generated request's debug(level); AppDebug::debug is not called
let request = client.item().debug(DebugLevel::None);
// unchanged in v3: the request has no inherent `debug`, so AppDebug::debug runs
```

Every such member has a deliberate disposition:

| Generated type | v2.1 member (receiver, arguments) | v3 | Disposition |
|---|---|---|---|
| bound request | `debug(self, DebugLevel)` | none: the debug level is runtime-wide (`configure_runtime(\|runtime\| runtime.debug(level))`) | manual decision, `C3-MIGRATE-GENERATED-MEMBER-REMOVED` |
| client builder | `c_provider(self, P)` for a custom credential `c`, generic over any `P: CredentialProvider` with the declared provider's `Material` | `c(self, Provider)`, exactly the declared provider type | manual decision, `C3-MIGRATE-GENERATED-MEMBER-REMOVED`, suggesting `c(..)`; not an automatic rename, because the argument type narrows |
| request of a paginated endpoint | `paginate(self, PageLimit)` | `collect_prefix`, `collect_all`, `pages` | the paginate rule ([Traversal terminals](#traversal-terminals)); a type-qualified call or function value of it is reported with `C3-MIGRATE-PAGINATE-UNRESOLVED` |

`C3-MIGRATE-GENERATED-MEMBER-REMOVED` (an error) is reported, and the site
left unchanged, for the same call forms as
[Names only v3 generates](#names-only-v3-generates): a method call (also a
raw identifier, `r#debug`) whose receiver is proved to be such a generated
type, where v2.1 dispatch reached the generated method (a typed binding or
parameter, a `Box`, a call chain), a type-qualified call
(`Type::debug(request, level)`, `<Type>::debug(..)`, `Self::debug(..)`) and
the same path named as a function value, a call inside a macro the tool does
not expand, and a receiver of another crate's type when another crate of the
migration run generated that name. A receiver not proved to be or not to be
a generated type is reported as possible ("may reach", with the reason),
only in a file that uses Concord, never as a proved removal. The message
names the removed member and its role, says that v3 has no such inherent
method and that an application trait method may be selected, names the trait
methods the crate implements for the type when the tool sees them, and gives
the v3 replacement (or says there is none). Migrate it by hand: remove the
call (a `debug(DebugLevel::None)` on a client whose runtime level is already
off has no v3 counterpart), replace it with the v3 form (runtime-wide debug
configuration, or `c(provider)` when the argument is the declared provider
type), or, if the application's trait method is really meant, call it
explicitly (`AppDebug::debug(request, level)`), which names the same method
in both versions.

Inside a macro the tool does not expand (see [Macros](#macros)), a removed
member counts wherever the tokens may name it, not only where `(..)`
follows it: a method call `.debug(..)`, and a qualified path whose last
segment is the member (`Type::debug`, `<Type>::debug`, `Self::debug`, also
with a turbofish) used as a call, as a grouped callee
(`pass!((Type::debug)(request, level))`) or as a function value
(`let apply = pass!(Type::debug);`). Such a site is reported as possible
("may reach", naming the macro as the reason), never as a proved call,
because the macro may use its tokens in any way. The path is resolved as
outside macros: a trait-qualified path (`<Type as Trait>::debug`,
`Trait::debug`) or one proved to name another type is not reported, and a
path to a generated paginated request's `paginate` is reported once by the
paginate rule (`C3-MIGRATE-PAGINATE-UNRESOLVED`). A bare `.debug` without a
call is a field access, and a `debug` segment followed by `!` (a nested
`log::debug!`) or by another segment (a module `a::debug::b`) is not a
member reference; neither is reported. Token-only standard macros
(`stringify!`, ...) stay untouched and unreported.

Not reported: a trait-qualified call, `RuntimeConfigBuilder::debug` (which
v3 keeps), a receiver proved not to be a generated type (a crate-local type,
another type the paths resolve to), a generated type on which v2.1 did not
generate the name (the unbound intent's `debug`), and a call with an
argument count the removed method could not take. A `.paginate(..)` method
call keeps exactly the paginate rule's diagnostic (a rewrite, or one
`C3-MIGRATE-PAGINATE-UNRESOLVED`), never a second removed-member report.

The parity guard `migration::tests::surface_parity` compares both
directions: a member the v2.1 generator emitted and the v3 generator no
longer emits on a modelled type fails the test until the tool's
removed-member inventory gives it a disposition (a manual-decision
diagnostic, a dedicated rule or an automatic rename that names its
migration test, or a concrete reason that no call can change meaning), and a
member whose receiver or arity changed must be classified as well. The
guard's snapshots record each member's receiver and argument count, not its
argument or return types: a signature change within the same arity (for
example a setter that now returns `Result`) is outside it.

### Receiver identity

A renamed call must really be a call of the generated setter: an unrelated
type may offer both `set_x` and `replace_x`, so a wrong rename would still
compile and silently change behavior. Automatic caller rewrites are therefore
default-deny: a call is rewritten only when the tool proves, the way the
compiler resolves names and methods over a model of the crate, that it
reaches the generated item. Everything else is left unchanged and reported
with the precise reason. The proof covers:

- **Paths.** A type path resolves through the module tree (`mod`
  declarations, `#[path]` attributes on file modules and on inline modules,
  which move the directory of the file modules declared inside them exactly
  as rustc does, and inline modules; files that no crate root reaches keep
  the module path of their location), `self::`, `super::` and `crate::`
  prefixes, and each module's items and `use` imports (renames, globs and
  visibility: `pub`, `pub(crate)`, private, `pub(self)`, `pub(super)` and
  `pub(in path)` are evaluated from the module that looks the name up, so a
  glob imports only what that module may see). Local items, generic
  parameters and block-local imports shadow outer names exactly. The
  generated names are placed where the `api!` invocation is: `Client`,
  `ClientBuilder` and `client::scope::EndpointRequest`. So
  `other::ItemRequest`, or an import `use other::ItemRequest as Req`, is
  never the generated `api::ItemRequest`, while `use api::ItemRequest as Req`
  and `type Req = api::ItemRequest` are. Another target of the same package
  (an integration test, an example) reaches the library by its crate name;
  the library is the file `[lib] path` names in `Cargo.toml` (else
  `src/lib.rs`), and `[[bin]]`/`[[test]]`/`[[example]]`/`[[bench]]` `path`
  entries are crate roots too. `Cargo.toml` is read as a TOML document
  (comments, quoting and inline tables as Cargo reads them), and
  `edition.workspace = true` takes `[workspace.package] edition` from the
  workspace root (the package itself when it has a `[workspace]` table, the
  directory `package.workspace` names, else the nearest ancestor whose
  manifest has one).
- **Fields** are typed through the declaration of the struct that owns them
  (`real.request`, `self.request`, a struct literal, a tuple field), never by
  collecting every field with the same name, and only when the field is
  visible where it is accessed (its `pub`, `pub(crate)`, private,
  `pub(self)`, `pub(super)` or `pub(in path)` visibility, from that module
  and crate): outside its visibility rustc skips the field and continues
  through `Deref`, possibly to another field of the same name, so such an
  access is unknown.
- **Calls** are typed through the function the path actually names: a
  block-local function, a block-local `use`, or a local closure shadows a
  module function. Calling an `async fn` produces an opaque future, not its
  declared return type, so its result is unknown (a blanket trait for
  futures may provide the method). `Client::builder()`, `Client::new()` and
  `Client::unbound()` start a receiver only when `Client` resolves to a
  generated client.
- **Annotations.** A declared type always decides a binding's type, never the
  initializer: `let r: &dyn Nav = client;` or `let b: Box<dyn Ext> = boxed;`
  coerces the value, so an annotation the tool does not resolve exactly (a
  trait object, an `impl` type, a generic parameter, a partially inferred
  `Box<_>`, an alias it cannot expand) makes the binding unknown. Only a
  plain `_` annotation constrains nothing.
- **Fully qualified calls** (`api::ItemRequest::set_x(request, value)`,
  `<api::ItemRequest>::set_x(..)`) resolve their type path the same way; a
  trait-qualified call names the trait's method and is never proved.
- **Setters named as values.** A generated optional setter named but not
  called (`fold(request, api::ItemRequest::set_x)`, `let clear =
  api::ItemRequest::set_x;`, `Self::set_x`, `<api::ItemRequest>::set_x`) is
  never rewritten: it is reported with `C3-MIGRATE-SETTER-UNRESOLVED` ("the
  setter is used as a function value") when its type resolves to a generated
  type with that optional setter, to another crate's type or to nothing the
  tool can resolve. Rename it to `replace_x` by hand when it is the generated
  setter. A path whose type is proved not to be generated is not reported.
- **Bindings** introduced by `if let`, `while let` and `match` arms shadow
  outer bindings, and a `ref` binding is a reference. As in rustc, an item of
  a block between a use and the local binding (or parameter) it would name
  shadows that binding, wherever in the block the item is: a `const`, a
  `static`, a unit or tuple struct, a function, a block `use` or a block glob
  that brings such an item (also in a closure body). The receiver is then
  that item, which the tool does not follow, and the call is reported; a
  glob, macro or `#[cfg]`-gated declaration of such a block whose items the
  tool cannot enumerate makes it unknown too. In the binding's own block a
  `let` shadows the block's function items, while a `const`, `static` or
  struct of that name (which would turn the `let` pattern into a constant
  pattern) makes the binding unknown. A macro in pattern
  position may bind any name, so every name visible there becomes unknown.
  After a statement macro that may introduce bindings or items, earlier
  names are unknown: a statement macro is known to introduce none only when
  its path resolves to a known expression macro of the standard library
  (`println!`, `assert!`, `vec!`, ...), `log` or `tracing` (`log::info!`,
  or `debug!` imported with `use tracing::debug;`). A crate-local
  `macro_rules!` with such a name, a local module named `log`, an import from
  another crate, another crate's glob or `#[macro_use]` crate (including one
  a `#[cfg_attr]` may mark `macro_use`), or an unimported `log` macro name
  does not qualify.
- **Wrappers and references** are not the type they wrap: an
  `Option<ItemRequest>`, `Result<ItemRequest, E>`, `Arc<ItemRequest>`,
  `Rc<ItemRequest>`, `&ItemRequest` or `&mut ItemRequest` receiver calls a
  method of the wrapper or of a trait (for example an extension trait
  implemented for `Option<ItemRequest>`), never the generated setter. Only an
  explicit `?`, `.unwrap()`, `.expect(..)` or `*` unwraps. Automatic
  dereferencing is followed only where Rust follows it to the generated
  method: a setter (which takes `self` by value) only through a `Box`, a
  navigation method (`&self`) through any reference or smart pointer. Such a
  call is interception-prone; see
  [Dispatch completeness](#dispatch-completeness).
- **Method chains** advance only through methods the v2.1 generated surface
  defines, with their exact result types: the value methods the declaration
  generates (`set_*`, `omit_*` for an optional value, `reset_*` for an
  initialized one), the builder's `configure_http`/`configure_runtime`/
  `retry`/`build` (a `Result`), child navigation, `Client::request`, and a
  request's `timeout`/`reset_timeout`/`debug`. Any other method, including a
  helper the application added to a generated type, makes the chain unknown.
  `Client::new()` returns a `Result` (`.expect(..)` or `?` unwraps it).
- **Raw identifiers.** `r#set_x` is the same method as `set_x`; a proved call
  becomes `r#replace_x`.
- **Other crates.** Only calls whose receiver is a type generated by the
  caller's own crate are rewritten. A receiver that resolves to a type of
  another crate (possibly generated by that crate's `api!` declarations, even
  in the same migration run) is reported: migrate that crate first, then
  rename the call by hand. The diagnostic says whether a same-named optional
  value is declared in this crate or by another crate of the run; a crate
  outside the run is invisible, so a call into it that the tool cannot type
  is not reported unless this run declares such a value.

When the identity is not proved (the receiver is unresolved, resolves to
another type or a wrapper of it, or depends on something the model cannot
see, such as a missing module file, an item-position macro other than
`api!`, a generic field's substitution or a user method's return type), the
call is left unchanged and reported with `C3-MIGRATE-SETTER-UNRESOLVED` and a
`replace_x` suggestion, if the crate (or another crate of the run) declares
an optional value with that name. Review each such site: rename it by hand
when the receiver is the generated type (or dereferences to it), and leave it
otherwise. The closure of `configure_runtime` is treated as Concord's runtime
configuration builder only when its receiver is proved to be a generated
client builder; otherwise a `NonZero` argument is reported
(`C3-MIGRATE-RUNTIME-NONZERO-UNRESOLVED`) instead of receiving `.get()`.

#### Macros

Macro invocations are never expanded.

- Inside a macro, the tool rewrites only in the expression arguments of a
  standard-library macro that evaluates them in place: `assert!`,
  `assert_eq!`, `assert_ne!`, `debug_assert!`, `debug_assert_eq!`,
  `debug_assert_ne!`, `dbg!`, `vec!` (the list form), `format!`,
  `format_args!`, `print!`, `println!`, `eprint!`, `eprintln!`, `write!`,
  `writeln!`, `panic!`, `todo!`, `unimplemented!` and `unreachable!`. The
  macro's path must resolve to the standard library: `std::`/`core::`/
  `alloc::` paths and imports of them, or a prelude name that nothing may
  replace (no crate-local `macro_rules!` of that name, no import or glob that
  may provide it, no `#[macro_use]` crate other than `std`, `log` and
  `tracing` (a `#[cfg_attr(.., macro_use)]` counts: in that configuration the
  crate's macros replace the prelude's), and no macro in item position or
  attribute macro of unknown expansion in that module).
- Token-only standard macros (`stringify!`, `concat!`, `include_str!`,
  `include_bytes!`, `env!`, `option_env!`, `line!`, `column!`, `file!`,
  `module_path!`, `cfg!`, `compile_error!`) are left untouched without a
  diagnostic: nothing in them is a call.
- Inside any other macro (a crate-local `macro_rules!` invocation or
  definition body, another crate's macro, `matches!`, a standard name that is
  replaced, or arguments a standard macro does not take as a comma-separated
  expression list), a `set_x` that may target a Concord-optional value,
  `.paginate(..)`, a call of a name only v3 generates (see
  [Names only v3 generates](#names-only-v3-generates)), a method call
  `.name(..)` of a name v3 no longer generates or a qualified path
  `Type::name` to one, called, grouped or as a function value (see
  [Names v3 no longer generates](#names-v3-no-longer-generates)) and a
  reference to the generated `ClientBuilder` are reported
  (`C3-MIGRATE-SETTER-UNRESOLVED`, `C3-MIGRATE-PAGINATE-UNRESOLVED`,
  `C3-MIGRATE-GENERATED-NAME-SHADOW`, `C3-MIGRATE-GENERATED-MEMBER-REMOVED`,
  `C3-MIGRATE-BUILDER-PATH-UNRESOLVED`), never rewritten: such a macro may
  rebind, reorder or stringify its arguments. `PageLimit` and
  `PaginatedRequest` there count as remaining references.
- An item annotated with an attribute macro whose expansion is not known is
  that macro's input: nothing in it is rewritten, and its sites are reported
  the same way. Built-in attributes (`#[inline]`, `#[allow]`, `#[doc]`,
  `#[must_use]`, `#[repr]`, ...), tool attributes (`#[rustfmt::skip]`,
  `#[clippy::..]`), `serde`'s helper attribute, the prelude's `#[test]`,
  `#[bench]` and `#[global_allocator]` where no import replaces them, and
  `tokio`'s `#[tokio::test]` and `#[tokio::main]` (their path must resolve to
  the `tokio` crate; they re-emit the function with its body unchanged) are
  known. Any other attribute macro, including `#[test]` after
  `use some_crate::test;`, is unknown: it may also replace its item and add
  items beside it, so the names the item declares and the module's glob and
  prelude names (standard macro names included) become unknown there.
- An invocation whose path ends in `api` is converted only when its path is
  proved to be Concord's `api!`: `concord::api!` (where no module, import or
  non-Concord glob in the enclosing blocks and module may bind the name
  `concord`, which would take precedence over the crate), an import of
  `concord::api` (also renamed, or re-imported through inline modules of the
  file), or a glob of `concord` or `concord::prelude`. A bare `api!` with
  nothing of that name imported in its file is not in the edition 2024
  prelude: it is Concord's only when its crate root has an unconditional
  `#[macro_use] extern crate concord`, no other crate's macros are imported
  with `#[macro_use]` there (the standard library, `log` and `tracing`
  export no `api!`), and no file of the crate defines a `macro_rules! api`
  (which may be in textual scope, also through `#[macro_use] mod`). The
  import and invocation must have stable source provenance: a replacing
  attribute, unread child module, or unknown child expansion cannot prove
  the absence of a crate-wide exported macro or inner `#![macro_use]`.
  A complete known child that supplies no such macro preserves the proof.
  An unrelated child does not erase an explicitly imported API origin.
  A crate-local `macro_rules! api`, another crate's `api`, or a glob of another
  crate disproves it, and `crate::` or cross-file paths are not followed;
  such an invocation is reported with `C3-MIGRATE-API-MACRO-UNRESOLVED` and
  left unchanged.

#### Conditional compilation

Features and other configuration options are never evaluated. A `#[cfg]`, or
a `#[cfg_attr]` that may apply `cfg`, `path`, `macro_use` or `macro_export`,
on a `let`, a function parameter or `self`, a `use`, an item (a module
declaration, a module file's inner `#![cfg]`, a function, a type, a constant,
an `api!` invocation, a foreign item), a named field (and a positional field,
which also shifts the later indexes) makes every name the declaration binds,
defines, imports or shadows uncertain there: a call whose receiver's path or
binding depends on it is reported with the gated declaration named. Only a
predicate built from `all`, `any` and `not` alone is constant (`all()` is
true, `any()` is false, `not(any())` is true); a `#[cfg(all())]` declaration
is certain and a `#[cfg_attr(any(), ..)]` is ignored. A gated `impl` is
treated as present, which can only block a proof (see below). A gated glob
import makes every name not declared or imported explicitly in that scope
uncertain. A `#[cfg_attr(.., macro_use)]` on an `extern crate` counts as
`#[macro_use]` (see [Macros](#macros)), and a `#[cfg_attr(.., path = ..)]` on
an inline module makes the file modules declared inside it unresolved.

#### Dispatch completeness

A call is *exact* when the receiver's own type (the first step of method
resolution) has the generated method: an inherent method wins there, so no
trait can intercept it, and such calls stay provable in every crate. A call
that reaches the generated method only after an auto-deref or auto-ref step
(a setter through `Box<Request>`, a `&self` navigation method on a value,
through a smart pointer, ...) is *interception-prone*: a trait method found
at an earlier step would be selected instead. It is rewritten only when all
of these hold:

- The crate's trait implementations are completely visible: the crate
  contains no invocation of a macro that may expand to an implementation
  (any macro but a standard one other than `include!`, a `log`/`tracing`
  expression macro, or a crate-local `macro_rules!` whose definitions and
  invocation contain no `impl`, no attribute and no macro invocation other
  than a standard prelude macro), no unknown
  attribute macro anywhere (including on an associated item or a field), no
  derive other than the standard ones (`Clone`, `Copy`, `Debug`, `Default`,
  `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Hash`) and `serde`'s `Serialize`
  and `Deserialize`, no macro invocation inside an `impl` or `trait` body, no
  module without a parsable source file (or with a `#[cfg_attr]`-dependent
  `#[path]`, or whose file is already a module of another crate root), no
  item the tool cannot classify, and every file is reached from a crate root
  by `mod` declarations. For a target of the package other than its library
  (an integration test, example, binary, bench or build script), which links
  the library and uses its generated types and traits, the library's trait
  implementations must be completely visible as well.
- No trait of another crate may be in scope where the call is: no import (a
  named one, `use x as _;`, or a glob) in the enclosing blocks and module
  that names an item of another crate, except a crate root itself
  (`use serde;`), the standard library, Concord and `serde`'s `Serialize`,
  `Deserialize` and `DeserializeOwned` (whose methods, `serialize` only, are
  known); an import or glob that does not resolve counts as another crate's,
  and so does a macro of unknown expansion in those scopes, which may add
  imports.
- No trait implemented in the crate (or, for another target of the package,
  in the package library) for a probed type provides a method of that name.
  The trait of each `impl` is resolved through imports, renames and
  re-exports like any path (`use traits::Ext as Other; impl Other for ..`
  implements `Ext`, whatever else is named `Other`); a trait defined in the
  crate is checked by its methods, and a trait defined elsewhere (including
  `Iterator` and other standard traits) or one whose path does not resolve
  may provide any method. Implementations in the package's other targets
  (another integration test, example or binary) are separate crates the
  call never sees.
- A generated method named like a standard blanket or wrapper trait method
  (`clone_into`, `borrow`, `as_ref`, `into`, ...) is proved only at the first
  step.

The diagnostic names one reason, chosen deterministically: the checks above
in order, the crate's constructs in a fixed walk (crate roots, then files and
items in source order) and a scope's imports in name order, so repeated runs
print the same text (for example "the crate uses the attribute macro
`#[my::attr]` (lib.rs:12)", "`other_crate::Ext` imported here is an item of
another crate", "the import `lib::records::X` depends on `records`, which a
declaration gated by `#[cfg]`/`#[cfg_attr]` may define", or "`input.rs` is
not reached from a crate root by `mod` declarations", naming the file of the
call).

#### Limits of the model

Every limit below makes a site unknown (left unchanged and reported), never
a guessed rewrite, except the assumptions listed last, which the tool cannot
check.

- A crate-local `macro_rules!` name is treated as defined everywhere in its
  crate (textual macro scope is not modeled), so a standard macro of the same
  name is never taken as standard anywhere in that crate, and a crate-local
  `macro_rules! api` anywhere in the crate disproves a bare `api!`.
- Only the external items listed above (the standard library, Concord, the
  `log` and `tracing` expression macros, `serde`'s derives and three traits,
  `tokio`'s two attribute macros) are known; any other crate's item, macro,
  derive or attribute macro blocks the proofs described above even when it
  is harmless.
- A file compiled more than once (a module file included by several crate
  roots, for example `tests/common/mod.rs` of several integration tests, a
  file that several `#[path]` modules name, a crate root that is also a
  module) is modeled at its first mount; for the other crates it is a module
  without a source file (the diagnostic names the other crate root). Its
  paths (`crate::`, `super::`, globs) may name other items in each mount, and
  the tool does not prove a site in every mount: no call, builder path,
  traversal terminal or bare `api!` in that file, or in the files of the
  modules it declares, is rewritten; each is reported naming the other mount.
  Its `api!` declarations still convert.
- Crate roots follow Cargo's default layout plus the `path` entries of
  `Cargo.toml` target sections; other manifest settings (`autotests`,
  `required-features`) are not read. A file no crate root reaches is
  modeled apart, and its interception-prone calls are never proved.
- Features and other configuration options are never evaluated.
- Paths follow the edition of the package: on edition 2015 (`edition =
  "2015"`, no `edition` at all, also through `edition.workspace = true`) a
  `use` path and a `::` path start at the crate root. When the manifest does
  not determine the edition (it cannot be read or is not valid TOML, has no
  `[package]` table or package name, names an edition the tool does not know,
  or inherits one that no workspace root declares), the edition is unknown,
  never assumed: a `use` or `::` path is resolved both ways and is unknown
  unless edition 2015 and the later editions agree, and the diagnostic names
  the manifest problem.
- Name resolution is bounded: one query follows at most 48 nested lookup
  steps (`MAX_DEPTH` in `tools/xtask/src/migration/resolve.rs`: re-exports,
  glob imports, aliases and type nesting while resolving one path, and the
  scopes searched for one macro name). A longer chain is cut and the name is
  unknown.
- A crate-local trait implementation counts wherever its trait may be
  implemented, whether or not the trait is in scope at the call.
- The types of generic fields, generic function results, generic aliases,
  associated types, `impl Trait`, trait objects, the output of `.await`,
  casts, indexing, blocks, `if`/`match`/`loop` values and closure
  parameters without a resolvable annotation are not modeled.
- Files are examined up to 256 levels of nesting, counting delimiters and the
  generic-argument brackets open in one statement together (an upper bound:
  a statement with more than 256 unclosed `<` comparisons joined by `&&` or
  `,` is refused too); a deeper file is reported with
  `C3-MIGRATE-NESTING-LIMIT` and left unchanged. The migration runs on a
  256 MiB stack.

Assumptions the tool does not check:

- Doc-comment examples (doctests: the code inside `///`, `//!` and
  `#[doc]` comments) are comments to the parser: they are never scanned,
  rewritten or reported. Migrate them by hand.
- A rewrite inside the arguments of `dbg!`, `assert!` and the other
  standard macros listed under [Macros](#macros) keeps the value's meaning,
  but changes the macro's printed text (`dbg!` prints the expression, a
  failing `assert!` prints its condition), which now shows the new name; the
  tool assumes no behavior depends on that text.
- A registry or `git` dependency is identified by its package name (see
  [Dependency identity](#dependency-identity)): a package named `concord`,
  `concord-core`, `concord-extras` or `concord-macros` is taken to be
  Concord's; so is a `git` or registry `[patch]` of such a package. Crate
  names that the manifest does not declare at all (a package without a
  readable manifest, a manifest without any Concord dependency, a file outside
  any package, or a file whose nearest manifest has no `[package]` table,
  such as a virtual workspace manifest) are assumed to be Concord's, as
  before. An `extern crate` item that renames a crate to or from a Concord
  crate name is not assumed (see
  [Dependency identity](#dependency-identity)).
- Files outside any package (no `Cargo.toml` above them) are read with the
  edition 2018 and later path rules, as one crate.

#### Dependency identity

Every decision of the tool assumes that the crate names `concord`,
`concord_core`, `concord_extras` and `concord_macros` denote Concord's own
packages (`concord`, `concord-core`, `concord-extras`, `concord-macros`). For
each package, the tool reads its `Cargo.toml` (as TOML): `[dependencies]`,
`[dev-dependencies]`, `[build-dependencies]` (also spelled with `_`) and the
same tables under `[target.<cfg>]`, and the package's own library name. A
dependency's package is its `package = ".."` rename or else its key; a
`workspace = true` entry is read from the workspace root's
`[workspace.dependencies]` (the root is found as for
`edition.workspace = true`), with its `path` relative to that root; a `path`
dependency must lead to a manifest whose `[package] name` is that package.
The identity is established when every Concord crate name the manifest
declares is the matching Concord package (renames between the matching
names, such as `concord_core = { package = "concord-core" }`, are fine) and no
Concord package is declared under another name. Otherwise, for example

```toml
concord = { package = "not-concord", version = "1" } # another package
concord = { path = "../fake" }                       # ../fake is not `concord`
cc = { package = "concord", version = "3" }          # Concord under another name
concord = { workspace = true }                       # no root declares it
```

or a package whose own library is named `concord` without being the Concord
package, every file of the package that the tool would have edited or
reported is left unchanged with one `C3-MIGRATE-DEPENDENCY-IDENTITY` error
naming each reason; files with nothing to migrate are not reported. The same
applies when

- a `[patch.<registry>]` or `[replace]` entry of the package's manifest or of
  its workspace root's (where Cargo reads them) redirects a Concord package to
  another package (`concord = { package = "fork", .. }`) or to a `path` whose
  manifest is not that Concord package (`concord = { path = "../fork" }` with
  `../fork` named `fork`); a `[replace]` key is a package ID specification
  (`"concord:3.0.0"`, `"concord@3.0.0"`, `"<index>#concord@3.0.0"`). A `path`
  to the genuine package, and a `git` or registry patch (identified by its
  package name, like such a dependency), are fine;
- a file of the package has an `extern crate` item that makes a Concord crate
  name denote another crate (`extern crate other as concord;`,
  `extern crate concord_core as concord;`, or `extern crate self as concord;`
  outside the genuine Concord library target), whatever the manifest declares; or one
  that renames a Concord crate (`extern crate concord as c;`) when the
  manifest does not prove that crate to be the Concord package (it is
  declared as above, or is the package's own library). The reason names the
  item and its `file:line` relative to the package.

An `extern crate self as concord` alias denotes the actual crate target.
The package's binaries, tests, examples and benches cannot borrow the library's
identity for this alias. Within the actual Concord library, a local `api`
definition or foreign re-export does not establish Concord macro ownership.
A re-export is accepted only when source resolution proves its canonical
`concord_macros::api` or `concord_macros::__api` origin; unknown expansions,
conditional bindings, unresolved origins and ambiguous mounts remain unresolved.
The tool leaves those macro tokens unchanged. The namespace proof is bound to
the exact indexed path and source bytes; if a file changes before rewriting,
rerun against a stable source tree instead of relying on an earlier proof.

A file whose nearest manifest has no `[package]` table (a virtual workspace
manifest, or a manifest that is not a package manifest) belongs to no package
Cargo builds: nothing is read from that manifest for it (its edition is
unknown, see [Limits of the model](#limits-of-the-model)), and its Concord
crate names fall under the assumptions above. The members of a workspace are
packages, and a `[patch]` or `[replace]` of its root applies to them. The tool
does not type-check: a manifest that cannot be read or parsed is not a
package Cargo can build, and falls under the assumptions above.

### Traversal terminals

```text
// v2.1
request.paginate(PageLimit::Prefix(2)).await?        // Vec<Item>
request.paginate(PageLimit::CompleteWithin(n)).await? // Vec<Item> or completion-limit error

// v3
request.collect_prefix(2).await?                     // Prefix<Item>: use .into_items() / .summary()
request.collect_all(n).await?                        // Vec<Item> or completion-limit error
request.pages(10)                                    // new lazy traversal
```

`collect_prefix` resolves to `Prefix<Item>`. When the old call is awaited
directly, the tool keeps the `Vec<Item>` type by appending the conversion
(`C3-MIGRATE-PAGINATE-PREFIX-ITEMS`):

```text
// v2.1
let items = request.paginate(PageLimit::Prefix(2)).await?;
let result = request.paginate(PageLimit::Prefix(2)).await;

// v3
let items = request.collect_prefix(2).await?.into_items();
let result = request.collect_prefix(2).await.map(|prefix| prefix.into_items());
```

Otherwise the output type intentionally changes and the tool warns
(`C3-MIGRATE-PAGINATE-PREFIX`); use `.into_items()` and `.summary()`, which now
also states whether the prefix is complete. A `PaginatedRequest<_>` annotation
becomes the selected terminal type (`C3-MIGRATE-PAGINATED-REQUEST-TYPE`). A
`PageLimit` import whose uses were all rewritten is removed; any remaining
`PageLimit` or `PaginatedRequest` reference is reported.

These edits follow the same [receiver identity](#receiver-identity) standard
as setters. A `.paginate(PageLimit::..)` call (with its `.await`, its
`PaginatedRequest<_>` annotation and its imports) is rewritten only when the
call is proved to reach the generated `paginate` of a request (only an
endpoint with a `paginate` clause has one) and `PageLimit` resolves to
Concord's type. A call on another receiver, for example an application
wrapper with its own `fn paginate(&self, limit: concord::PageLimit)`, an
extension trait on `Option<EndpointRequest>`, or a helper method that returns
another type, is left unchanged and reported
(`C3-MIGRATE-PAGINATE-UNRESOLVED`, with the
`.collect_prefix(n)`/`.collect_all(n)` spelling as suggestion); its
`PageLimit` stays a reported reference. A local type that is merely named
`PageLimit` or `PaginatedRequest` is not Concord's: its uses are neither
rewritten nor reported, and do not keep a Concord import alive.

### The generated builder moves into the support module

v2.1 generates `ClientBuilder` beside `Client`, where `api!` is invoked; v3
generates it in the support module, as `client::ClientBuilder`.
`Client::builder()` is unchanged. A written reference to the builder type (an
`impl` block for it, a parameter or return type, `ClientBuilder::new()`, a
`use` of it) is rewritten to the v3 path when it is proved to name the
generated builder (`C3-MIGRATE-BUILDER-PATH`):

```text
// v2.1
impl ApiBuilder { .. }
use super::ApiBuilder;
fn build(builder: crate::ApiBuilder) -> Api { .. }

// v3
impl api::ApiBuilder { .. }
use super::api::ApiBuilder;
fn build(builder: crate::api::ApiBuilder) -> Api { .. }
```

A path that reaches the builder through an import is left alone, because the
import itself is rewritten. A reference whose target the tool cannot resolve,
or where the support module name is taken by another item, is left unchanged
and reported (`C3-MIGRATE-BUILDER-PATH-UNRESOLVED`, suggesting
`client::ClientBuilder`); a type of another module that is merely named
`ClientBuilder` is neither rewritten nor reported.

### Runtime configuration counts take primitives

The runtime builder's body/page/provider setters take primitive counts in v3.
Existing validated NonZero arguments migrate with `.get()`; aggregate
validation still happens at build.

```text
// v2.1
.configure_runtime(|runtime| runtime.response_body_limit(NonZeroUsize::new(8 * 1024 * 1024).expect("positive")))
// v3
.configure_runtime(|runtime| runtime.response_body_limit(NonZeroUsize::new(8 * 1024 * 1024).expect("positive").get()))
```

### Declaration type imports

v2.1 resolved some declaration types (for example `Json`, `Binary`, `Text`,
`Empty`, `Discard`, `Bytes`, `ByteStream`) through the facade prelude. v3
declarations name ordinary imported Rust types, so the tool adds the missing
`use concord::{...};` next to the invocation (`C3-MIGRATE-PRELUDE-IMPORT`) or
reports a site it cannot place (`C3-MIGRATE-PRELUDE-IMPORT-UNRESOLVED`). A
declaration path that would now resolve through a generated support-module
name, or through a relative `self`/`super` prefix, is reported with a
suggested absolute path (`C3-MIGRATE-GENERATED-SHADOW`).

### Values without a use

v2.1 did not check value liveness; v3 rejects a value with no wire, route,
rate-key or pagination use (VALUE-06). The tool reports each such value
(`C3-MIGRATE-DEAD-VALUE`): remove it (a caller-visible constructor change),
give it a real use, or keep an optional or initialized one as an explicitly
deprecated no-op migration bridge, whose generated docs and semantic diff
disclose the no-op:

```text
#[deprecated(note = "no-op since v3")] legacy_mode?: String
```

### Retained exactly

The root `timeout` (optional in v3 with a visible 30-second default),
`request`/`response` defaults, `request explicit;`/`response explicit;` fences,
credentials, auth placements and challenges, status, effect and
`expose accepted_bytes` keep their v2.1 spelling. A representation fence is
never removed because current output would be equal: it still blocks future
parent edits.

## Behavioral and capability changes (review required)

These rows of the §25.2 translation table change contracts, not just spelling.

| v2.1 | v3 | What to check |
|---|---|---|
| Named/inline rate keys | Explicit logical labels; legacy bytes preserved | IDs alone do not prove continuity; labels, order and encodings do. |
| Repeated same-ID attachments | Deduplicated only when structurally identical | Conflicting repetitions are errors; the effective contract shows deduplication. |
| Controller public-field initialization | Public Init type plus whole Cursor mapping | Custom controllers need adapters; private state can now evolve. |
| Response `fn accept()` | `const ACCEPT: Option<&'static str>` | Inspection never calls user code to learn negotiation. |
| Dual body metadata and `prepare` | `PreparedBody` owns Content-Type, length and replay facts | Plans no longer run metadata callbacks. |
| Json page-marker iterable bound | Buffered driver plus typed controller | Envelope pages work; `PageResponseRepresentation` is gone. |
| Unconditional `store_as_*` installation | `StoreOutcome` and latest-started intent | Callers must handle `Superseded`. |
| Generic error defaulting to NotDispatched | Journal-derived aggregate/last-attempt/response facts | No false no-send guarantee; reconcile writes by command ID. |
| Runtime root bare binding | Full root owner projection | Restoring documented behavior may change wire/auth/quota results. |
| Runtime arbitrary declared-source selection | Explicit owner auth boundary; keep/remove active placements only | Runtime auth additions move to declared owners. |
| Body-bearing request `Clone` assumptions | Request factory; page stability is a separate promise | No blanket `Clone` for body-bearing requests. |
| Status-coded `Option` custom code | Keep the buffered `NotFound<R>` adapter, or select head-only `absent on` | The two differ in body acquisition and accepted-byte capability. |

Additional deliberate behavior changes (§25.3):

- **Suppressed public encoders no longer run.** An ancestor query/header
  contribution removed by a later operation is not encoded, so a failing
  suppressed adapter no longer fails the request.
- **Sensitivity of a key is sticky.** A key that carried a `#[sensitive]`
  contribution anywhere on the root-to-owner chain stays redacted in plans,
  errors and observers after a replacement, removal or family fence. The real
  request is unchanged. The tool flags such overrides with
  `C3-MIGRATE-SENSITIVITY-STICKY`.
- **Root defaults are captured at builder creation** and not reevaluated by
  `build`, binding or reset. v2.1 evaluated a root default lazily inside
  `build()`, only when the value had not been overridden, and evaluated it
  again there for a value restored by `reset_*`; v3 evaluates it exactly once
  when `builder()` (or `new()`) creates the builder, even if the value is
  overridden later, and `reset_*` restores that retained payload. Scope and
  endpoint defaults keep their v2.1 timing (once at selection/construction).
  The tool flags every root `#[default]` value with
  `C3-MIGRATE-ROOT-DEFAULT-TIMING`, and `tools/xtask/tests/migrated_v3.rs`
  classifies the difference with a counting `Default`.
- **The root timeout may be omitted** and then selects a visible 30-second
  default; migrated declarations keep their explicit timeout.
- **Custom limiter admission is atomic.** A complete conjunctive plan is
  admitted or nothing is debited; v2.1's permitted partial prefix reservations
  are gone (`C3-MIGRATE-RATE-LIMITER-ATOMIC`).
- **Rate key encodings are named.** `RateKeyPart` and `RateKeyEncoder` declare
  a stable `ENCODING_ID`; built-in canonical bytes are unchanged
  (`C3-MIGRATE-RATE-KEY-PART`).
- **Provider helper origins are enforced.** `ProviderHttp` only reaches origins
  a provider declares in `http_origins` (`C3-MIGRATE-PROVIDER-ORIGINS`).
- **No non-idempotent auth replay after a response.** A 401 is not proof that a
  non-idempotent write had no effect.
- **Lazy streams have finite leases.** Acquired streams get a total lifetime
  and a pending-read idle limit.
- **Aggregate response-byte accounting covers failed and retried pages**, not
  only delivered pages.
- **Runtime auth additions move to declared owners.** Runtime endpoints can keep
  or remove active placements, not introduce new credential sources.

## Unautomatable decisions

The tool reports these with an `error` diagnostic and leaves the source as
written. It never invents a `safe` effect, a policy ID, an API-version path, a
secret value or an always-`None` response to make code compile.

- **Exact versus partial configuration** intent in application code (for
  example `set_x(opt)` on a receiver the tool cannot prove to be a generated
  type: `C3-MIGRATE-SETTER-UNRESOLVED`).
- **Body clone factories** for code that cloned body-bearing requests.
- **Changed quota state**: any intended change of policy ID, label, window or
  attachment location is a reviewed state reset, not a migration step.
- **A newly enforced provider allowlist** for custom providers.
- **A page controller's private state design** for custom controllers
  (`C3-MIGRATE-PAGINATION-CUSTOM-CONTROLLER`, `C3-MIGRATE-PAGINATION-CONTROLLER-IMPL`).
- **`StoreOutcome::Superseded` handling** at every `store_as_*` call site:

  ```text
  match client.authenticate(login).store_as_session().await? {
      StoreOutcome::Installed { version } => { /* installed */ }
      StoreOutcome::Superseded => { /* a newer explicit installation intent won */ }
  }
  ```

- **Static negotiation** for `fn accept()` implementations:

  ```text
  // v2.1
  fn accept() -> Option<&'static str> { Some("text/csv") }
  // v3
  const ACCEPT: Option<&'static str> = Some("text/csv");
  ```

- **Runtime `PageLimit` values** passed to `.paginate(limit)`: choose
  `.collect_prefix`, `.collect_all` or `.pages` explicitly.
- **Declaration sites v3 rejects**: brace-containing path adapter types,
  literal `?`/`#` segments, repeated window durations, `when_present` without a
  fallback, `remove` of a never-owned key, reserved scope names, and endpoints
  that need an explicit ID whose v2.1 ID is not a valid v3 ID.

## Identity and registry continuity

Capture generated endpoint pins, policy IDs, ordered labels, canonical type
tags and window identity before converting (the tool's JSON report records
them). Registering a changed schema under an existing ID is rejected unless a
shared adapter implements an explicit coordinated migration. With the built-in
local limiter, a process restart resets local state by definition; that is not
persistent preservation. With a shared backend, stopping all old writers and
waiting a backend-defined safe interval is one possible operational protocol,
not a universal guarantee. Keep the v2.1 baseline sources available for
differential tests, and do not normalize away type-path qualification, docs,
labels or function identities to make tokens compare equal.

## Worked example: the Catalog baseline

The v2.1 Catalog contract converts with its endpoint IDs, rate IDs, windows,
costs and labels unchanged. Abridged result (doc comments, credentials, auth
and some endpoints omitted):

```text
client CatalogApi {
    base "https://catalog.example.test/root/";
    timeout 20s;

    values {
        #[sensitive]
        query tenant: TenantId;
        query locale?: Locale;
        #[default]
        api_version: ApiVersion;
    }

    subdomain ["api"];
    path "{api_version}";

    rate_limits {
        policy tenant_reads(tenant) {
            id "catalog.tenant.read";
            key [origin, "tenant" = tenant];
            cost 1;
            allow 120 per 1m;
            allow 5_000 per 1h;
        }
    }

    scope regional(
        region: Region,
        header("Accept-Language") language?: Language,
    ) {
        subdomain [region];
        path "regions/{region}";
        rate_limit tenant_reads(tenant = client.tenant);

        GET item_summaries(
            query category?: Category,
            query offset: u64 = 0,
            query count: PageSize = DEFAULT_PAGE_SIZE,
            query("tag") each tags: Vec<Tag>,
        ) -> Vec<ItemSummary> via Json {
            id "catalog.items.list";
            path "items";
            paginate via concord::pagination::OffsetPagination {
                init concord::pagination::OffsetInit { offset: offset, page_size: count };
                cursor -> offset;
            }
        }

        PUT update_item(
            id: ItemId,
            #[sensitive]
            header("Idempotency-Key") command_id: CommandId,
            body update: UpdateInput via Json,
        ) -> Item via Json {
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
    }
}
```

The only unautomatable site in that file is the `store_as_session()` caller,
which must decide what `StoreOutcome::Superseded` means for the application.
