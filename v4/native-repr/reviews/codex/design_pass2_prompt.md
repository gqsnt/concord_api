# Codex task — design challenge pass 2: the selected design

You ran a blind design challenge (pass 1) on these two questions. The local team has now selected
a design; it is described in `DECISIONS.md` (N01–N08) and `CONTRACT.md` (grammar + truth table),
with the evidence of three specialist tracks: `track_h_headers.md` (native headers, prototype),
`track_ra_contract.md` (owner-level representation defaults, prototype) and `track_rb_local.md`
(format words in place, prototype). The selected representation design combines them (owner
defaults `request FORMAT;` / `response FORMAT;` + in-place format words `json item: T`,
`-> text String`, `body item: T` for the inherited format, body-bound media types). Nothing of it
is implemented on this snapshot yet: the snapshot is the branch before the implementation (it
already contains the placement-note fix, the multipart recipe, the Clone checks and the Criterion
suite). `CHARTER.md` is the owner's request.

This is a read-only design challenge: find what is wrong, ambiguous, inconsistent, unnecessarily
complex or missing **before** implementation. In particular:

1. Is the combined representation design one coherent primary design, or two synonymous DSLs in
   disguise (charter §5.2)? Is `body item: T` (inherited) next to `json item: T` (explicit) a
   problem? Would pure owner statements (RA) or pure in-place words (RB) be strictly better, and
   why?
2. Grammar: ambiguities between format words, placements (`query x`, `header("k") x`), value
   names, custom format type paths, `-> FORMAT T` vs `-> T` followed by `{`/`;`; lowercase words vs
   type paths; anything the parser cannot decide or that reads misleadingly.
3. The truth table: wrong rows, missing rows (charter §5.3 items 1–12), rows whose diagnostics are
   impossible to produce on stable Rust, media-type and `content-type` interactions (8a–8e),
   feature/cfg behaviour (12a–12b), the `-> ()` diagnostic versus "no spelling special-casing".
4. Native headers: the open `ToHeaderValue` trait, the template escape instead of your
   `Format<T>` wrapper, `IntoValue<HeaderValue>` accepting `&HeaderValue`, generated `Debug` calling
   `to_header_value()`, the per-value sensitivity rule. Anything unsound or leaking.
5. Interactions between the two changes (a format default and header presence/inheritance; an
   optional native `content-type` header and body presence).

Rank findings by severity with concrete counterexamples (declarations that would behave wrongly
or confusingly). Give a verdict: ADOPT, ADOPT WITH CHANGES (list them), or REJECT (with the
alternative). Report the model and effort as observed, and what you executed versus only read.
