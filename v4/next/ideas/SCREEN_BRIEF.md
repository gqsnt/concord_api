# P3.2 merge and screen — brief for a merge lane

You merge two independent idea lanes (one Claude, one Codex) for the same families into one file per
family, then screen every merged idea against the Concord charter. You do not invent new designs;
you may combine two lane designs into one idea when they are the same mechanism.

Read first: `/home/canarit/projects/Concord_Next/CHARTER.md` (rules C1–C9, glue, facts vs behavior),
`/home/canarit/projects/Concord_Next/IDEAS.md` (the family sections you handle),
`/home/canarit/projects/Concord_Next/METHOD.md` § Lessons. Evidence you may check: the repository
at `/home/canarit/projects/Concord_Next/work/wt/rv-p1` (read-only), the glue inventory
`/home/canarit/projects/Concord_Next/work/glue-dev/GLUE.draft.md`, the friction catalogue
`/home/canarit/projects/Concord_Next/work/inventory/FRICTION.md`.

## Merge

- One entry per distinct mechanism. If the two lanes propose the same mechanism, merge them into
  one entry, keep the stronger evidence, and list both sources (`AB-claude.md § A2`, `AB-codex.md § A1`).
- Ideas that are variants of each other become one entry with the variants listed in one line.
- Keep the lanes' best concrete code (before → after declaration, call site) but shorten it.
- Check at least the headline glue claim of each idea against the source (open the files it cites);
  correct numbers that are wrong and say so.

## Screen (per entry)

- **KILL** if it breaks a charter rule; name the rule and the evidence in one line:
  C1 a statement about Concord or a negation (turning off generated surface), not a provider fact;
  C2 behavior (retry, refresh, limiter, traversal loop) in the declaration, or a provider fact
  restated by hand; C3 the simple case (Nager, PokeAPI, Frankfurter) gains lines or concepts;
  C4 inference a reader cannot predict from the declaration; C5 errors that name internals the user
  never wrote; C6 needs a paragraph of documentation to understand; C7 loses an exit (`reqwest`
  client, `prepare()`, `into_parts()`, `send_raw()`) or privileges built-in formats; C8 hidden
  actors, implicit retries, middleware or global state in the default path; C9 a guarantee without
  a test that fails when it breaks.
- **KILL** also if it only moves glue (for example a new trait every SDK must implement with the same
  lines), or if it removes almost nothing for a new concept.
- **MERGED** into another entry when it is a variant.
- **KEEP** otherwise. Rank the KEEP entries within the family (1 = strongest) by glue removed per
  concept added and by how many SDKs it helps.
- Aim for about 2 KEEPs per family (fewer if the family is weak, up to 4 if the ideas are genuinely
  different). Do not keep two ideas that would compete for the same mechanism unless they are real
  alternatives worth showing side by side at the gallery; mark such pairs "ALTERNATIVE TO X".

## Output: one file per family, `/home/canarit/projects/Concord_Next/work/ideas/merged/<letter>.md`

```
# <letter>. <family title> — merged ideas
Sources: <lane files>. <N> lane designs → <M> entries: <k> KEEP, <x> KILL, <m> MERGED.

## <letter><n>. <name> — KEEP (rank r) | KILL (Cx: reason) | MERGED into <id>
Sources: <file § heading>, ...
Fact: <the provider fact, one line>
Mechanism: <2–3 lines: statement / attribute / core type / library change>
Declaration: <witness SDK, before → after, ≤ 10 lines>
Call site: <before → after, ≤ 4 lines, or "unchanged">
Glue removed: <estimate, categories, SDKs; "checked against <file:line>" or "lane estimate">
Counterexample: <SDK: how handled, or not>
Simple control: <unchanged | change>
New concepts: <list>  Breaking: <list or none>
Shares mechanism with: <ids in other families, if any>
Screen: <one line per rule that matters>
```
KILL and MERGED entries may be 3–5 lines. Then, in your final message, report per family: the KEEP
list (id, name, rank, one-line glue estimate), the KILL list (id, rule), and cross-family mechanism
overlaps you noticed. Write nothing outside `/home/canarit/projects/Concord_Next/work/ideas/merged/`.
