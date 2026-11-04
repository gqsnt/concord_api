#!/usr/bin/env python3
"""RETIRED in Concord 4.2: the V4 refinement campaign's mutation driver. It refuses to run.

It applied one production-code mutation at a time to a disposable checkout and ran the tests meant
to catch it (historical record: v4/refinement/evidence/m4_mutations.md, bound to its own revision;
`git log -p -- tests-gen/m4_mutations.py` holds the runnable version). The table below is kept as
that record's inventory; nothing applies it.

Retired rather than maintained, because:
  * nothing maintained calls it: no script, workflow, gate step or current document refers to it
    (only historical records under v4/ do);
  * four of its nine mutations edit files that no longer exist (crates/concord/src/prepared.rs,
    error.rs, __private.rs), so a run would stop at the first of them;
  * its verdict was "killed" whenever the baseline and restored runs passed and the mutant run did
    not, so a mutant that failed to compile, and therefore ran no test, counted as killed (V4.1
    review finding M04: an undefined-name mutant, error E0425, zero tests run, verdict "killed").

A compile failure, or a selection that runs zero tests, is never a runtime "kill". A future
mutation check should keep separate: the baseline result, whether the mutant compiles, the selected
tests and their count, the intended assertion's result, the restoration, timeouts, unrelated
failures and uncertainty (compile-contract negatives under their own rule), with no global score.
"""

import sys

# Historical inventory, applied by nothing.
M = "crates/concord-macros/src/model.rs"
E = "crates/concord-macros/src/emit.rs"

# name -> (description, [(file, old, new)], [cargo test args])
MUTATIONS = {
    "a1_init_on_prepare": (
        "endpoint `prepare` evaluates the endpoint's initializers again",
        [
            (E, "            let clone_impl = endpoint_clone(ty, &owner.values, body.as_ref());",
             "            let reinit: Vec<&syn::Expr> = owner.values.iter().filter_map(|v| v.default.as_ref()).collect();\n"
             "            let clone_impl = endpoint_clone(ty, &owner.values, body.as_ref());"),
            (E, "                    #[inline] pub fn prepare(self) -> ::core::result::Result<#k::Prepared<#out>, #k::Error> {\n",
             "                    #[inline] pub fn prepare(self) -> ::core::result::Result<#k::Prepared<#out>, #k::Error> {\n"
             "                        #(let _ = #reinit;)*\n"),
        ],
        [["-p", "concord", "--test", "values"]],
    ),
    "a2_init_on_clone": (
        "endpoint `Clone` evaluates the endpoint's initializers again",
        [
            (E, "            fn clone(&self) -> Self {\n",
             "            fn clone(&self) -> Self {\n                #(let _ = #reinit;)*\n"),
            (E, "    let mut fields: Vec<&Ident> = values.iter().map(|v| &v.name).collect();",
             "    let reinit: Vec<&syn::Expr> = values.iter().filter_map(|v| v.default.as_ref()).collect();\n"
             "    let mut fields: Vec<&Ident> = values.iter().map(|v| &v.name).collect();"),
        ],
        [["-p", "concord", "--test", "values"]],
    ),
    "b_remove_keeps_ancestor": (
        "`remove` empties nothing: the suppressed ancestor contribution is still emitted",
        [(M, "                        if c.op == Op::Remove {\n                            s.slot.items.clear();",
          "                        if c.op == Op::Remove {\n                            // mutant: items kept")],
        [["-p", "concord", "--test", "oracle_composition"],
         ["-p", "concord", "--test", "composition"]],
    ),
    "c_override_drops_sensitivity": (
        "a later override/remove replaces the slot's sensitivity instead of keeping it (not sticky)",
        [(M, "                        s.slot.sensitive |= c.sensitive;\n                        s.last_writer = writer;\n                        s.history.push(id);\n                        if c.op == Op::Remove",
          "                        s.slot.sensitive = c.sensitive;\n                        s.last_writer = writer;\n                        s.history.push(id);\n                        if c.op == Op::Remove")],
        [["-p", "concord", "--test", "oracle_sensitivity"],
         ["-p", "concord", "--test", "d15_surfaces"],
         ["-p", "concord", "--test", "sensitivity"]],
    ),
    "d_unit_response_buffered": (
        "an endpoint without `->` buffers the response body instead of draining it",
        [("crates/concord/src/prepared.rs", "        let body = if self.drain {",
          "        let body = if false && self.drain {")],
        [["-p", "concord-heap-check"]],
    ),
    "e_error_source_transport": (
        "`concord::Error::source()` returns the transport error again",
        [("crates/concord/src/error.rs", "impl StdError for Error {}",
          "impl StdError for Error { fn source(&self) -> Option<&(dyn StdError + 'static)> { self.transport().map(|e| e as &(dyn StdError + 'static)) } }")],
        [["-p", "concord", "--test", "d15_surfaces"],
         ["-p", "concord", "--test", "tls"],
         ["-p", "concord", "--test", "sensitivity"]],
    ),
    "f_header_append_unchecked": (
        "headers use `HeaderMap::append` instead of `try_append` (no capacity check)",
        [("crates/concord/src/__private.rs", "        self.headers\n            .try_append(name, value)",
          "        Ok::<bool, ()>(self.headers.append(name, value))")],
        [["-p", "concord", "--test", "native_limits"]],
    ),
    "g_segment_display_error_ignored": (
        "`encode_segment` ignores a `Display` error",
        [("crates/concord/src/__private.rs",
          "    write!(EncodeInto(out), \"{value}\").map_err(|_| unformattable(meta, what))",
          "    let _ = (meta, what, write!(EncodeInto(out), \"{value}\"));\n    Ok(())")],
        [["-p", "concord", "--test", "display_errors"]],
    ),
    "h_else_inherit_fallback_empty": (
        "`override ... else inherit` with an absent source emits nothing instead of the inherited value",
        [(M, "let fallback = std::mem::take(&mut s.slot.items);",
          "let fallback = { s.slot.items.clear(); Vec::new() };")],
        [["-p", "concord", "--test", "oracle_composition"],
         ["-p", "concord", "--test", "composition"]],
    ),
}


def main():
    print("tests-gen/m4_mutations.py is retired (Concord 4.2): it applies no mutation, runs no test "
          "and reports no verdict; see its documentation.", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
