# Fresh-use trial (short) — Terra 6.1 XHigh on `ab3cb85`

Brief: `/home/canarit/projects/concord_v4_finish_work/briefs/FRESH_USE_DELTA.md` (three tasks on the
journeys the review-pack changes touched: decode hints, borrowed error metadata, declaration
changes; docs first, no library source or `v4/`). Not exposed to the review pack. Observed settings
gpt-6.1-terra / xhigh (session `turn_context`), 21:15:27→21:17:04Z, tracked files unchanged.
Archive: `/home/canarit/projects/concord_v4_finish_work/evidence/fresh-use/terra-delta/` (SHA256SUMS).
The report below is verbatim; its links point into the disposable snapshot.

---

The trial completed against a loopback server. The three baseline calls and the 429 handling passed; both deliberate response mistakes produced messages that identified the declaration change. The scope change broke the old calls as predicted, while adding an optional query parameter preserved them.

The working trial is in [trial/](/home/canarit/projects/concord_v4_finish_work/snapshots/trial-delta-terra/trial), with predictions and observations in [PREDICTIONS.md](/home/canarit/projects/concord_v4_finish_work/snapshots/trial-delta-terra/trial/PREDICTIONS.md). These are trial findings, not an acceptance verdict.

**Task 1 — declaration and calls.** I predicted `get(7)`, `text(7)` and `delete(7)` would return request values whose `send()` results are `Notebook`, `String` and `()` respectively. That was correct. The server recorded:

```text
GET    /v1/notebooks/7
GET    /v1/notebooks/7/text
DELETE /v1/notebooks/7
```

The JSON notebook decoded, the text was `# Title\nbody`, and the 204 succeeded.

For the two mistakes, I correctly predicted decode failures but underestimated how helpful the top-level messages would be:

```text
GET FieldNotes.text: response not decoded: JSON decode error (syntax) at line 1, column 1; if the response is not JSON, the endpoint may need `response text;` or `response bytes;`
```

This tells the user to add `response text;`. The underlying body source was `expected value at line 1 column 1`.

```text
DELETE FieldNotes.delete: response not decoded: JSON decode error: the response body is empty or only whitespace, which is not JSON; an endpoint whose response has no body omits `->`
```

This tells the user to remove `-> ()`. The underlying body source was `EOF while parsing a value at line 1 column 0`.

The [Quick start](/home/canarit/projects/concord_v4_finish_work/snapshots/trial-delta-terra/docs/QUICKSTART.md) and [Cheat sheet](/home/canarit/projects/concord_v4_finish_work/snapshots/trial-delta-terra/docs/CHEATSHEET.md) answered both declaration questions. The generated client rustdoc confirmed the effective format for each endpoint.

**Task 2 — retain the error.** I predicted that `status()` and borrowed `response()` would let `wait_for(&error)` read `Retry-After`, after which `into_response()` could still take the unread body. The run confirmed a 12-second duration and decoded the title `Slow down`. The exact status display was:

```text
GET FieldNotes.get: HTTP status 429 Too Many Requests
```

The implementation is in [lib.rs](/home/canarit/projects/concord_v4_finish_work/snapshots/trial-delta-terra/trial/src/lib.rs). The generated `Error::response` rustdoc supplied a borrowed `Retry-After` example; the [Caller guide](/home/canarit/projects/concord_v4_finish_work/snapshots/trial-delta-terra/docs/CALLER_GUIDE.md) explained taking and decoding the body afterward.

**Task 3 — declaration changes.** I predicted that moving `get` into `scope notebook(id: u64)` would remove `get` from the client and require `api.notebook(7).get()`. The unchanged calls produced this diagnostic twice, once for each call site:

```text
error[E0599]: no method named `get` found for struct `FieldNotes` in the current scope
```

The [full compiler output](/home/canarit/projects/concord_v4_finish_work/snapshots/trial-delta-terra/trial/scope-compiler.txt) includes the locations. The corrected scoped calls compiled and passed with the same wire paths.

I predicted that adding `query fields?: String` would leave `api.get(7)` valid and add a setter. That also held: the unchanged calls passed without a query, and `.set_fields("id,title")` sent `?fields=id%2Ctitle`.

One fixture issue arose before these runs: alternate endpoint declarations behind `#[cfg]` still triggered Concord’s duplicate-child checks. I replaced them with one declaration at a time. A small documentation improvement would be to say explicitly whether mutually exclusive `#[cfg]` declarations may reuse a child name. The fixture mistake is not counted as a library defect.

I personally ran the locked, offline tests and both requested rustdoc builds, and read the resulting public rustdoc pages. The copied lockfile initially needed an update for the new workspace package, so I regenerated **the trial copy** offline before building with `--locked --offline`. I read the requested public guides and the permitted loopback support code; I did not inspect library source, library tests, macro expansions, or `v4/` design records. The test servers were dropped after each run. No existing tracked files were modified.

The snapshot HEAD is `bd33434f08a3707135a4f2ff968503d3bab8fdce`, as supplied. The original source SHA was unavailable in this checkout’s object database, so I could not verify its relationship to the snapshot. Rust and Cargo both reported version 1.97.1. The environment exposed `CLAUDE_EFFORT=xhigh`, but I could not independently verify the effective model or that this variable controls the active reasoning setting.