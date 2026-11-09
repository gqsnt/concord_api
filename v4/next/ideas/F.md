# F. Paging facts — merged ideas
Sources: EF-claude.md, EF-codex.md (family F parts). 10 lane designs → 7 entries: 3 KEEP, 2 KILL, 2 MERGED.

Evidence: `glue:paging` = 4,006 lines (GitHub alone 577). FRICTION F1 "paging cannot be declared"
(17 SDKs), F2 "vocabulary hard to learn" (12), F3 "repeated cursor not refused" (10), F4 "resume key not
bound to its listing" (9), F7 "cannot start from a position" (5), F10 "numbered/offset walks by hand" (4),
F13 "a page whose continuation is refused fails whole" (2). Checked: 18 `pub fn *_from(.., Resume<..>)`
twins in 11 SDKs; 28 copies of the "Use `…Resume` for the position" sentence in SDK sources.

## F1. `Pages` starts anywhere and refuses repeated keys (library, no DSL) — KEEP (rank 1)
Sources: EF-claude.md § F1, EF-codex.md § F4 (via F4)
Fact: none declared. It removes two restatements every paging SDK writes: "a traversal can start at a position" (the `_from` twin) and "a cursor the provider repeats is a protocol error" (Datadog's FRICTION §2 asks for exactly `Pages::refuse_repeated_keys()`).
Mechanism: concord-pagination gains `Pages::starting_at(self, Resume<K>) -> Self` and an opt-in `Pages::refuse_repeated_keys()` (`K: Eq + Hash + Clone`) that ends with `PagesError::Repeated { resume }` without sending the repeated request.
Variants: codex (F4, merged) changes no library: the SDK's single traversal method takes `from: Resume<K>`, which keeps eager validation inside its constructor but refuses no repeated key.
Declaration: unchanged.
Call site:
```rust
- client.search_pages_from(request, Resume::At(cursor))            // Datadog client.rs:154-161
+ client.search_pages(request).starting_at(Resume::At(cursor))
- notion.children_from(id, size, from)   →   + notion.children(id, size).starting_at(from)
  // codex variant: api.droplet_pages(size, cap) → api.droplet_pages(size, Resume::Start, cap)
```
Glue removed: claude ≈85 twin lines in 11–12 SDKs, ≈45 repeat-check lines in 9 SDKs and the 28 doc copies; six SDK error variants (`CursorCycle`, `RepeatedCursor`, `CursorDidNotAdvance`, `LinkDidNotAdvance`, `ContinueDidNotAdvance`, `StalledCursor`) collapse into one. Codex's variant: 51 lines over 12 pairs, no new concept. Checked: the 18 twins (Cloudflare 3, DigitalOcean 4, Registry 2, Notion 2, Datadog, Discord, GitHub, GitLab, Mailgun, Graph, Stripe 1 each) and the 28 doc copies exist; DigitalOcean `pagination.rs:28-30` is a 3-line start delegator as claimed.
Counterexample: Mailgun refuses a cursor of another search or credential eagerly (`events.rs:263-271`, pinned by `tests/first_use.rs:952-955`); `starting_at` cannot, so Mailgun keeps its one `pages_from` or uses the codex variant. Tail-polling cursors that legitimately repeat simply do not opt in. A continuation that fails after accepted records: `Incomplete` keeps the prefix, and `starting_at(failed)` resumes from the failed page.
Simple control: unchanged (none of the three has a paging helper).
New concepts: `Pages::starting_at`, `Pages::refuse_repeated_keys`, `PagesError::Repeated` (+ `Outcome::Repeated`); codex variant: none.  Breaking: `PagesError` is not `#[non_exhaustive]` (only its variants are; `concord-pagination/src/pages.rs:691`), so exhaustive caller matches need an arm. The codex variant breaks every first-page call (`Resume::Start`) and every `_from` name.
Shares mechanism with: H (one sentence in concord-pagination replaces 28 copies), G (twins are forwarders), A (six SDK error variants and their `Display` arms), F3 (a next link equal to the requested URL is `Repeated`).
Screen:
- C2: traversal behavior stays in the paging crate.
- C6: removes the H09c `Resume` paragraph from every SDK.
- C7: `Pages::new(from, factory)` is unchanged for engine users.
- C8: no I/O; the seen-key set grows with pages read (bounded by `max_pages`).
- Codex's objection (moving a running traversal) must hold: `starting_at` applies only to an unpolled `Pages`.
- C9: a repeated key ends with `Repeated` and its position after the right number of requests; `new(from, f)` ≡ `new(Start, f).starting_at(from)`; `starting_at` after the first poll is refused.

## F2. Declared cursor relation: where the cursor goes, where the next one comes from — KEEP (rank 2)
Sources: EF-claude.md § F2, EF-claude.md § F3, EF-codex.md § F1 (+ numbered kinds via F5)
Fact: which request value carries the position (query, header, body field) and, in the decoded page, where the items are, where the next cursor is, and what says more pages exist.
Mechanism: request side, a `#[cursor]` role on one optional input. It generates `concord::Cursor { type Key; fn at(self, Option<Key>) -> Self }`; body cursors use a Rust `CursorField` impl on the body type. Response side, an endpoint statement `page items <path>, next <path>, more <path>;` with Rust field paths of the declared output (`?` through `Option`, `[last]` on a `Vec`). It generates `concord::Paged::split` and the `more`/`next` consistency checks, plus `Operation::is_paged()`. concord-pagination (`Pages::over`, `Paginate::pages()`) drives it through the client's send policy.
Variants: codex states one relation, `page cursor request.cursor <- response.cursor, empty = end;` plus `page items response.payments;`, generates `CursorSlot`/`PageProjection`, and calls a Rust `cursor_pages(endpoint, from)`; `empty = end` is an explicit sentinel where claude leaves empty cursors to serde · request role only (claude F2, projection still by hand) as stage 1 · numbered kinds (F5).
Declaration: Zendesk (`lib.rs:68-77`):
```rust
        GET list(
            query("page[size]") page_size: PageSize = PageSize::MAX,
-           query("page[after]") after?: String,
+           #[cursor] query("page[after]") after?: String,
            query sort?: TicketSort,
            query external_id?: String,
-       ) -> TicketPage;
+       ) -> TicketPage { page items tickets, next meta.after_cursor, more meta.has_more; }
// Square, codex form: page items response.payments; page cursor request.cursor <- response.cursor, empty = end;
```
Call site:
```rust
- api.payments().list().pages(Resume::Start, cap)                  // Square lib.rs:135-159
+ api.payments().list().pages().max_pages(cap)
- ticket_pages(api.tickets().list(), Resume::Start, cap)   →   + api.tickets().list().pages().max_pages(cap)
```
A direct `list().send()` still returns one page.
Glue removed: claude ≈300 lines in ≈9 SDKs: request role ≈115 in 7 (Stripe `ListRequest` + `list_request!` ≈50, Sentry `listing!` ≈26, Square 17, Intercom 15, Datadog 6, Kubernetes 3), projection ≈230 in 6 (Intercom 65, Zendesk 56, Stripe 35, Notion 31, Datadog 30, Square 22), overlapping on Square and Intercom. Codex: Square 25, Intercom 25, Notion 7. Checked: Square `lib.rs:135-159` (25 lines, including the `Resume` doc) and Intercom `pages.rs:19-43` (two adapters that differ only in cursor placement, as its comment at `:45-46` says).
Counterexample: Elasticsearch's partial results and a PIT id that changes per page (`pit.rs:443-453`) → must not use `page`; it keeps a hand `Paged`, and partial-result signals are refused by the send policy before paging sees the page (Notion `client.rs:105-115` already does). Kubernetes snapshot consistency (`list.rs:299-328`) and Discord's ordering and short-page end (`api.rs:149-178`) stay by hand; Discord can still mark `#[cursor]` on its `Anchor`, and Kubernetes can replace `with_resource_version(None, None)` with one exclusive `ListPosition` value. Empty cursors: Square ends on one, Datadog refuses one, so it is per endpoint, never a default. Stripe's backward direction and binding stay.
Simple control: unchanged (PokeAPI's `offset` is not marked; opt-in only).
New concepts: `#[cursor]`, `concord::Cursor`, `concord::CursorField`, `page items/next/more`, `concord::Paged`, `Paginate`, `Operation::is_paged()` (codex: `page items`, `page cursor … <- …`, `empty = end`, `CursorSlot`, `PageProjection`, `cursor_pages`).  Breaking: none (additive). `Paginate` needs the policy output to expose the value: `Result<T, E>` works today, `Result<Response<T>, E>` needs B1. SDKs that delete their inherent `.pages(from, cap)` change callers.
Shares mechanism with: E2 (a role on a declared value plus a generated accessor), E1 (`is_paged()` on the same `Operation` carrier, removing Sentry's `paged` column and half of GitHub's `listing_id`), B1/B3 (the `items` path is a list-envelope unwrap; B1 lets the engine reach the value), D/J (cursors already `#[sensitive]` keep their redaction), G/H (per-listing wrappers, `Resume` prose).
Screen:
- C1: each clause copies a provider doc line ("`page[after]`: the previous page's `meta.after_cursor`").
- C2: the loop stays in concord-pagination.
- C4: no name convention (`cursor`, `after`, `starting_after` are not recognized); the only default (`items` is the output when it is a `Vec`) is visible in `->`.
- C5: a wrong path is rustc's E0609 at the path token; two `#[cursor]`, or `#[cursor]` on a required value, is an error at the attribute.
- C6: the risk both lanes name. The clause set (`items`, `next`, `more`, `[last]`, `empty = end`) must stay closed or it becomes an expression language; field paths also hide serde renames.
- C7: field paths are Rust model fields, so no format is privileged.
- C9: fixture tests for `more`/`next` contradictions; compile-fail for a wrong path and for two cursors; `Cursor::at` produces the same request as the setter.

## F3. A provider-issued next URL is followed as the same endpoint, through the client's policy — KEEP (rank 3)
Sources: EF-claude.md § F4, EF-codex.md § F2
Fact: the next page is a URL the provider issues (`Link: rel="next"` for GitHub and Sentry; a body field for DigitalOcean `links.pages.next`, Mailgun `paging.next`, Graph `@odata.nextLink`, PokeAPI `next`) and is requested verbatim as the same operation.
Mechanism: endpoint statement `page next link;` / `page next url <path>;`. A followed page is `Followed<E>: Endpoint<T>` with E's identity, sent through the client's send policy, so auth, admission and error documents apply to every page; the key is an opaque `NextUrl` with a withheld `Debug`.
Variants: trust derived from the declaration — same origin, at or below the client's base path, no userinfo or fragment, no knob (claude) — vs the SDK's own pure validator and explicit typed context in a Rust `FollowPlan` that validates the start position eagerly (codex, `continuation next <- response.paging.next;` and `<- header Link(rel = "next")`).
Declaration:
```rust
- GET labels(query per_page?: PerPage) -> Vec<Label> { path "labels"; }                     // GitHub api.rs:80-81
+ GET labels(query per_page?: PerPage) -> Vec<Label> { path "labels"; page next link; }
- GET list(..Paging, query tag_name?: TagName) -> Droplets;                                   // DigitalOcean lib.rs:39
+ GET list(..Paging, query tag_name?: TagName) -> Droplets { page items droplets, next url links?.pages?.next; }
// Mailgun, codex form: `-> EventPage { path "events"; continuation next <- response.paging.next; }`
```
Call site:
```rust
- repo.label_pages(repo.handle().labels()).pages()   →   + repo.handle().labels().pages()        // GitHub
- events.pages_from(from, cap)?   →   + FollowPlan::new(events, MailgunEvents)?.pages(from, cap)?   // codex
```
Glue removed: claude ≈570 in 5 SDKs (GitHub ≈330, Sentry ≈80, DigitalOcean ≈70, Mailgun ≈50, Graph ≈40); codex ≈36 gross in Mailgun (minus a 10–15-line adapter) and ≈42 gross in DigitalOcean. **Checked, and the lane total is high:** GitHub `link.rs` (175 lines) already delegates RFC 8288 syntax to `concord_pagination::link` (`link.rs:20`), so it is relation projection (`next/prev/first/last`) and `LinkError`, not a parser to move into core; GitHub's 12 per-listing factories (`pages.rs:47-198`) carry the listing binding (operation constant, parent number), which this entry does not remove without the killed F6; codex keeps `paging.rs:454-579` (restored links, repository-id paths). Read GitHub's share as ≤330 and the total as ≈100 (codex) to ≈570 (claude, upper bound). It also closes the bypass that makes DigitalOcean (`pagination.rs:150`), Mailgun (`events.rs:289`) and PokeAPI (`lib.rs:216`) call their policy's finisher by hand (A/B glue).
Counterexample: Sentry uses the cursor *parameter* of its `Link` plus a `results="true|false"` flag (`pagination.rs:94-98`) → keeps a ≈10-line hook. GitHub links may address the repository by id (`/repositories/1300192/…`) → "below the base path" admits them, equal-path trust would not. DigitalOcean and Graph check the same collection path and `tag_name` today → claude's base-path rule relaxes that unless they keep a hook (codex keeps the SDK validator). Graph delta checkpoints stay a separate type. QuickBooks' policy rewrites the host (`policy.rs:35-36`) → following through the policy is wrong there. Mailgun and DigitalOcean reject a page whose next link is invalid before accepting its records (FRICTION F13): preserved, not solved.
Simple control: unchanged (PokeAPI keeps its manual `.follow`; opting in would delete `lib.rs:208-217` but is not required).
New concepts: `page next link`, `page next url <path>` (or `continuation next <- …`), `concord::NextUrl`, `concord::Followed<E>`; codex adds `Continuation<E, C>`, `FollowPlan` and a generated dispatch handle.  Breaking: SDK-local `NextLink`/`EventCursor` types and stored cursor formats change (versioning needed).
Shares mechanism with: A1/B1 (followed pages get the policy's error document and result shape; FRICTION O2, 7 SDKs: `Prepared::send` bypasses the policy), F1 (a link equal to the requested URL is `Repeated`), F2 (the `page` statement), D (hand-written `NextLink(<redacted>)` and `Links(<redacted>)` `Debug`), G (GitHub's factories are forwarders).
Screen:
- C1: "GitHub paginates with `Link: rel="next"`" is the provider's pagination guide.
- C2: the relation is declared; trust, context identity and terminal decisions are behavior. Prefer codex's Rust validator: claude's derived rule is weaker than today's checks, a silent relaxation (C4).
- C7: `Prepared::follow` stays public for SDKs with other rules.
- C8: one request per page, through the SDK's own policy; nothing at construction.
- C9: canary that a followed URL is withheld; wire test that cross-origin and outside-base links are refused before I/O; a test that the policy runs on page 2; Mailgun's eager-refusal test unchanged.

## F4. One traversal method takes `Resume` (no library change) — MERGED into F1
Sources: EF-codex.md § F4
The same glue as F1's `starting_at` (the `_from` twins, 51 lines over 12 pairs), removed by SDK convention: rename `_from` to the only method and delete the start delegator. Zero concepts, and it keeps Mailgun's eager check; the cost is `Resume::Start` at every first-page call, and nothing refuses repeated keys. Listed as F1's variant so the gallery shows both.

## F5. Numbered positions: `#[cursor(page)]`, `#[cursor(offset)]`, `page more nonempty`, `page total <path>` — MERGED into F2
Sources: EF-claude.md § F6
An extension of F2's role and clauses for page-number and offset walks (Strava `lib.rs:213-265`, Cloudflare `pagination.rs:79-140`, Kraken `trades.rs:65-94`: ≈90 lines in 3 SDKs; FRICTION F10, 4 SDKs) for four more words. Add it only after F2 is measured: the lane warns it pushes `page` toward a small language, and numbered paging carries quirks (Cloudflare's echoed page, Strava's conservative empty page, Kraken's shifting offsets).

## F6. Listing identity = the declared request minus its cursor (`Listing`, `Bound<K>`, eager `Foreign`) — KILL (C4: whether a credential binds the cursor depends on the process boundary)
Sources: EF-claude.md § F5
Sensitive values enter only an in-memory digest with a per-process salt, and the stored form drops them. So whether a rotated credential invalidates a cursor depends on whether it was held or restored, which a reader cannot predict from the declaration, and the lane shows it is wrong both ways (GitHub must survive a token rotation, Mailgun must not). The per-process salt is also global state (C8), and the persisted digest a format to keep stable forever. FRICTION F4 (9 SDKs) stays open; F3's codex variant (explicit typed context in Rust) covers the link-following cases.

## F7. Composite snapshot continuation (`page snapshot`, `page initial`, `SnapshotPager`) — KILL (glue: ≈60 lines in one SDK for four concepts)
Sources: EF-codex.md § F3
Kubernetes only (`list.rs:119-153`, `:270-279`, `:303-328`, ≈60 net). The lane itself says first/next request shapes plus invariant keys "may be too much language for one strong witness", and the design does not fit Elasticsearch's changing PIT id. Kubernetes keeps its hand pager; F2's `#[cursor]` with one exclusive `ListPosition` value still removes its `with_resource_version(None, None)` plumbing.
