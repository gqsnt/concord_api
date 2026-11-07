# Independent review: Concord between-waves (W1→W2) core, macro and adoption changes

You are an independent reviewer (read-only). The working directory is a git worktree of the
Concord Rust workspace on branch `sdk50/integration`. Review the diff `8dc53558..HEAD`
(`git diff 8dc53558..HEAD`, `git log --oneline 8dc53558..HEAD`). Do not build: the sandbox is
read-only and the coordinator's verification logs are not your evidence; read the code and tests.
Contact no external service.

## What changed (claims to check, not to trust)

1. `limit EXPR;` statement (client/scope/endpoint): a declared finite-body bound carried by
   `Decoder`, honoured by generated `send()`, `Received::decode()` and native `prepare()?.send()`;
   finite typed outputs only (crates/concord-macros model/emit, crates/concord-core prepared.rs).
2. `ResponseHead::single` / `unsigned` with `FieldError { Repeated, Invalid, Overflow }`
   (crates/concord-core/src/codec.rs) and their adoption in NASA, GitHub, GitLab SDKs.
3. `query(..) x?: T` placement with `QueryPairs` / `QueryPairsWriter` (crates/concord-core
   value.rs, __private.rs `Parts::query_pairs`, query redaction in the prepared Debug renderer;
   crates/concord-macros syntax.rs/model.rs/emit.rs; tests checks/core-concepts/tests/f3_query_pairs.rs,
   crates/concord/tests/ui/query_pairs_*). Rules claimed: endpoints only, never `#[sensitive]`,
   no key/each/header(..), pairs after the chain's declared query, endpoint Debug withholds the
   value, an empty key is a Build error, well-known secret key names redacted in Prepared Debug.
4. `concord-ratelimit` crate (crates/concord-ratelimit): admission, cooldowns, leases.
5. `concord-codecs::check_content_type` + `Charset`; `concord-pagination::next_offset` +
   `OffsetError`; FRED/USGS adoption.
6. SDK adoption of `limit` (10 SDKs) and Discord's limiter overlap fix
   (examples/discord-rest-sdk/src/limiter.rs `observe`: keep lower remaining, later reset).
7. Docs: docs/AUTHOR_GUIDE.md §3 (query-credential transport), §4.1a, §4.7, §5.10a;
   docs/CALLER_GUIDE.md §8; docs/CHEATSHEET.md.

## What to look for

Correctness and safety defects with a concrete failure scenario: a bound that some finite path
bypasses or applies to a stream/drain; a redaction leak (Debug/Display of endpoints, Prepared,
Error) or a new over-redaction that changes documented output; macro hygiene, spans, cfg-gated
values, raw identifiers, groups (`params`/`..Name`) interacting with `query(..)` or `limit`;
query order on the wire; percent-encoding of run-time keys; an error case that panics; lock
poisoning or time-arithmetic overflow in concord-ratelimit; cancellation that leaks a charge or a
lease; a doc claim the code does not satisfy; tests whose assertions do not test the claim.
Also: anything in the public API that is larger than its evidence justifies.

## Output

A list of findings, most severe first. For each: file:line, one-sentence defect, concrete
failure scenario (inputs → wrong result), confidence (CONFIRMED by reading / PLAUSIBLE), and
the smallest fix. Then a short list of checked claims you found sound. Say explicitly if you
found nothing in an area. A missing finding is not approval of an area you did not read: list
the areas you did not cover.
