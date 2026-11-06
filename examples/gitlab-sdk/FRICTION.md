# Author observations, in order

Frozen library: `e04236ca`. Reading material besides the docs and rustdoc: the GitHub SDK
(`examples/github-sdk/src/{api,paging,link,context}.rs`), and from wave 1 only the PokéAPI
`FRICTION.md`/`PROVENANCE.md` headings and Twilio's `tests/support/mod.rs` (for the file layout
and the test client builder). Where one of them supplied a solution, the entry says so. No
Concord file was edited; no private item (`__private`, `#[doc(hidden)]`) is used.

Attribution keys: **Concord**, **SDK**, **provider**, **application**, **docs**, **harness**.

1. **Easy — Concord: the URL-encoded project path needs no code.** `src/api.rs:49-50` declares
   `scope project(id: ProjectRef) { path "projects/{id}"; }`; `ProjectRef`'s `Display` writes the
   path with its `/`, and Concord's one-value-one-segment rule sends `acme%2Ftools%2Fwidgets`,
   which is exactly GitLab's documented requirement. Reproduce:
   `tests/protocol.rs:66` `the_project_path_is_one_encoded_segment` (prepared URLs) and
   `tests/workflows.rs:53` (raw targets on the wire). The SDK only refuses inputs that would be
   wrong on the wire (`src/params.rs`, `ProjectRef::path`: no `/`, empty or dot segments, a `%`
   that would be double-encoded); test `tests/protocol.rs:93`.

2. **Easy — Concord: the private token.** `#[sensitive] header(PRIVATE_TOKEN) token: HeaderValue`
   (`src/api.rs:43`); `private-token` is also on Concord's well-known sensitive names. The
   prepared request flags it, `Prepared`'s and the client's `Debug` redact it, errors never show
   it. Reproduce: `tests/protocol.rs:166` `the_token_is_validated_flagged_and_never_printed`;
   `GitLab::validate_header_names()` passes there too.

3. **Easy — Concord: filters and one adapter for every operation.** The eight listing filters
   are generated setters on `GitLabProjectIssues` (`src/api.rs:58-77`); no forwarding builder or
   filter record was written. `concord::Endpoint<T>` let one `Call` impl (`src/call.rs:34-46`)
   give all three operations the SDK's error and metadata. `Received::bytes_limited` plus
   `<Json as Decode<_>>::decode_body` made the bounded error-document read a dozen lines
   (`src/call.rs:73-93`). `Received::url()` gave the base for relative `Link` targets
   (`src/meta.rs:69-70`). Pages: `concord_pagination::Pages::new` over a free function returning
   an owned `Send + 'static` future (`src/pages.rs:109`, `:120-126`) needed no `+ use<..>`
   workaround.

4. **Workaround — Concord: following a `Link` URL prepares a placeholder first page.** To send a
   linked page as the same endpoint (headers, labelled decoder, operation name in errors), the
   factory prepares the generated listing — building and discarding the first-page URL and query
   on every page — then replaces the URL natively: `src/pages.rs:124` (`listing.prepare()`),
   `:132` (`into_parts`), `:146` (`*request.url_mut() = key.url`), `:151`
   (`decoder.execute`). `Decoder::new::<Json>()` would be unlabelled (errors without the
   endpoint name) and would still lack the declared headers, so there is no shorter public path.
   AUTHOR_GUIDE §5.11 names exactly this route, and the GitHub SDK's `paging.rs` (`fetch_page`)
   supplied the pattern; nothing failed. It is the case W1-17 deferred to this wave ("no decoder
   from an `Endpoint` type: following a server URL prepares a placeholder operation"). Reproduce:
   any linked-page test, e.g. `a_follows_the_returned_link_whatever_its_representation` or
   `changed_link_query_survives_cap_and_failed_retry` in `tests/workflows.rs` (the latter also
   exercises the same native URL replacement after a cap and a failed-page retry).

5. **Workaround — Concord: the base URL is not readable from a generated handle.** The link
   check needs the API origin and base path (so the token never leaves them and the link names an
   issue collection). Generated `GitLab`/`GitLabProject*` expose `with_base_url` but no getter
   (checked in the generated rustdoc of `GitLab`, `GitLabProject`, `GitLabProjectIssues`). The
   SDK derives both from the prepared first-page URL and strips the three trailing segments it
   knows the declaration produces (`projects/<id>/issues`): `src/pages.rs:134`, `:208-209`. This
   couples `check_link` to the path templates at `src/api.rs:50` and `:76`: editing either
   silently changes what the check accepts. The GitHub SDK instead keeps its own copy of the base
   in `GitHubContext` (`DECLARED_BASE`), which duplicates the declaration. Reproduce:
   `tests/protocol.rs:374` `a_link_to_another_collection_is_refused` (passes; the coupling is
   only visible by reading the code).

6. **Repeated library work — SDK (Concord ecosystem): a second RFC 8288 `Link` parser.**
   `src/link.rs` (295 lines with docs) parses `next`/`prev`/`first`/`last`, strictly, with a
   URL-free `Debug`; the GitHub SDK has its own `src/link.rs` for the same header, and its design
   (strict parsing, a malformed header is an error, URLs withheld) supplied mine. Neither Concord
   nor `concord-pagination` offers `Link` relations, although the pagination README lists
   `Link: rel="next"` as a typical key. A third-party crate was considered and not added: the
   strictness rules (a malformed header must not read as "last page") are the part that matters,
   and the parser is small. Reproduce: `tests/protocol.rs` `link_values_are_read_by_rfc_8288`.

7. **Construction — Concord/SDK: validated credentials sit beside the generated constructor.**
   `ToHeaderValue` is sealed and a credential newtype must not go through `Display` (W1-15), so
   the declaration takes a `HeaderValue` and the SDK adds `PrivateToken` plus an inherent
   `GitLab::with_token` on the generated type (`src/api.rs:88-112`, `src/params.rs:229`). The
   generated `GitLab::new(http, GitLabConfig { token })` and `set_token` stay public and accept
   any `HeaderValue`, including an empty one, so the validation can be bypassed. Adding inherent
   methods to generated types (`impl GitLab` in `src/api.rs:88`, `src/context.rs:84`;
   `impl GitLabProjectIssues` in `src/pages.rs:84`) works in the declaring crate, but the guides
   do not say whether this is a supported extension point or how to avoid future generated-name
   collisions (docs).

8. **Two error types — Concord shared mechanic.** Every operation has the generated `send()`
   (`concord::Error`) and the SDK's `call()` (`GitLabError` with GitLab's document and
   metadata). The README says which to use; nothing prevents a caller from using `send()` and
   losing the error document. Observed in wave 1 as W1-9; unchanged here.

9. **Transport — application (documented): `PRIVATE-TOKEN` follows cross-origin redirects with
   reqwest's default policy.** GitLab redirects `GET`s on a moved project's old path. With
   reqwest's default policy the token was forwarded to another origin:
   `tests/protocol.rs:584` `control_the_default_redirect_policy_forwards_the_token` (the
   control) versus `:543` with the SDK's `redirect_policy()` (`src/lib.rs:83`). CALLER_GUIDE §8
   documents that reqwest forwards custom headers on redirects; the policy is a property of the
   whole `reqwest::Client`, so the SDK can only recommend it. The policy code follows the GitHub
   SDK's `context.rs::redirect_policy`.

10. **Provider: pagination headers can disagree with `Link`.** GitLab documents `Link` as the
    way to the next page and `x-next-page` as informational. The SDK follows `Link` and refuses
    only the case that would truncate silently (no `rel="next"` while `x-next-page` names a page):
    `src/pages.rs` `next_link`; tests `tests/protocol.rs:404` and `:413`. A mutation that built
    the URL from `x-next-page` was caught by `:413` only after I fixed a test that had added a
    second `x-next-page` line (the first, empty, line was the one read) — test authoring, not
    Concord.

11. **Provider: the pinned strategy.** `GET /projects/:id/issues` documents offset and keyset
    (GitLab 18.3+) pagination and recommends keyset for consecutive pages; the `order_by` values
    differ between them (`title`, `id` are keyset-only). The SDK pins offset pagination and
    follows whatever `next` link is returned (`tests/workflows.rs:140` follows a cursor link).
    Supporting keyset would add a declared `pagination` value, a second `order_by` enum and a
    server-version question; none of it is a Concord limitation.

12. **D-A5 (repeated parameter placement): not observed.** This slice declares one listing, so
    its eight filters appear once (`src/api.rs:58-77`). GitLab's group and instance issue
    listings (`/groups/:id/issues`, `/issues`) take largely the same filters (inferred from the
    Issues API page's sections, not exercised); declaring them would repeat the block. Out of
    scope here, so no measurement.

13. **Harness: absolute links need the server address before the replies.** `Script` replies are
    built before `TestServer::start` returns its URL, so tests that send absolute `Link` targets
    create an empty `Script`, start the server, then `push` (`tests/workflows.rs` `serve`,
    `tests/protocol.rs` `first_page_with`). Small and local.
