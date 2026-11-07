# Plan amendment A2 — what Concord is and is not (owner, in chat, 2026-11-07 ~23:45 CET)

The owner's words (lightly edited for typos, meaning unchanged):

> The core is the DSL → usage: its client → scope → endpoint signature, sitting on top of reqwest.
> Everything else can be changed — like "no hook", "no operation identity". It gives you powerful
> features and functionality at minimal or no cost. People flatten their clients because handling
> scopes is hell to write, and hell to rewrite or remove; Concord gives the best representation
> and the best usage for it. Each persona that reads, writes or uses this code can easily read and
> understand the context they need, and understand the domain. It is like a Rust compiler for APIs
> that backs you up and lets you think only about what truly matters, and tells you: if you stay in
> Concord's safe place, your sensitive data won't leak. Content type, encoding, headers: handled
> nicely for you. (Decoding JSON without a content type should not be the default case.) For I4,
> Codex is fine.

This amends `MISSION.md` § Fixed / Challengeable, `AUTHORITY.md` § Not granted (charter
non-responsibilities), `contracts/NORTH_STAR.md` and `PLAN_AMENDMENT_A1.md` where they conflict.
Push, merge, tag, publish and delete grants are unchanged.

## 1. Fixed
- The DSL as the source of the usage: one declaration, client → scope → endpoint, and the
  generated signatures that follow it.
- Built on top of reqwest.
- Honest evidence and independent review (unchanged).

## 2. Challengeable (newly)
Everything else, **including the product charter's non-responsibilities** (`v4/PRODUCT.md` "What
Concord is not": no hooks, no operation identity, no auth orchestration, retries, rate limiting,
pagination, managed executor, contract tooling). A hypothesis that crosses them is decided by the
loop's rule like any other (card, pilot, review, WIN/LOSS/INCONCLUSIVE); it no longer goes to the
owner queue for acceptance. The North-star properties remain the desirability criteria the
reviewer checks; where a property encodes one of those non-responsibilities (e.g. P7 "no hidden
actors", P8 "not a cage"), a card may argue a change and the reviewer judges it against the
owner's objective below.

## 3. The objective, in the owner's terms (refines A1 §1)
Powerful features at minimal or no cost to the author and caller. Concretely, the loop favours
changes that:
- make **scopes** the easy, natural representation (authors flatten clients today because scopes
  are costly to write, rewrite or remove): measure flattening and the cost of adding/removing a
  scope level;
- let **each persona** (author, caller, maintainer, reviewer) find the context they need and
  understand the domain from the declaration and its generated surface;
- act like a **compiler for the API**: mistakes caught early with useful messages;
- keep the **safe place** promise: stay inside Concord and sensitive data does not leak (I2 is the
  instrument; observability inside the safe place becomes a candidate, see O1 below);
- handle **content type, encoding and headers** for the user. JSON decoding stays strict by
  default (current behaviour: `Json` refuses a 2xx without a JSON `Content-Type`;
  `IgnoreContentType<Json>` is the explicit per-endpoint opt-out). No change needed.

## 4. Queue and owner-queue consequences
- **O4** (charter wording) → no longer an owner decision: docs-truth item **T1** rewrites
  `v4/PRODUCT.md` "What Concord is not" on `concord-desirability` to state the fixed core (§1)
  and present the non-responsibilities as current scope, not a permanent boundary (reviewed).
- **O6** (Claude edit agents) → resolved: I4 runs Codex-only; single-provider is accepted for I4.
- **O1** (Concord-produced logging/observability with the secret guarantee) → moves into the
  loop as candidate **H15** (SENSITIVE/CAPABILITY), behind H04, H08, H03, H06.
- New emphasis for **H06/H07/H12**: scope representation (flattening, cost of scope changes) is a
  primary outcome there, not only D1b counts.
