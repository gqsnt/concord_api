#!/usr/bin/env python3
"""Targeted mutation checks (handoff guides/09_TESTING.md "Mutation checks").

Applies one production-code mutation at a time to a DISPOSABLE checkout, runs the tests that
should catch it, and restores the file with `git checkout`. Never point it at a shared checkout.

    git worktree add --detach <root> refine-m4-tests        # throwaway checkout
    python3 -I tests-gen/m4_mutations.py <root> [name ...]   # all mutations or a subset
    git worktree remove <root>

Environment: CARGO_TARGET_DIR (default <root>/target), CARGO_BUILD_JOBS (default 6).
For each mutation it records: baseline pass (unmutated), mutant result, restored pass.
"""

import os
import re
import subprocess
import sys

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


def run_tests(root, cmds, env, log):
    """Returns (all_passed, summary)."""
    ok, parts = True, []
    for args in cmds:
        cmd = ["cargo", "test", "--locked", *args]
        p = subprocess.run(cmd, cwd=root, env=env, capture_output=True, text=True)
        out = p.stdout + p.stderr
        log.write(f"$ {' '.join(cmd)}  (exit {p.returncode})\n{out}\n")
        results = re.findall(r"test result: (\w+)\. (\d+) passed; (\d+) failed", out)
        failed = sorted(set(re.findall(r"^    (\S+)$", out, re.M)))
        target = args[-1]
        if p.returncode == 0:
            parts.append(f"{target}: pass ({sum(int(r[1]) for r in results)} passed)")
        else:
            ok = False
            if not results:
                parts.append(f"{target}: FAIL (build error)")
            else:
                shown = ", ".join(failed[:4]) + (" ..." if len(failed) > 4 else "")
                parts.append(f"{target}: FAIL ({sum(int(r[2]) for r in results)} failed: {shown})")
    return ok, "; ".join(parts)


def apply(root, edits):
    for path, old, new in edits:
        full = os.path.join(root, path)
        with open(full) as f:
            text = f.read()
        n = text.count(old)
        if n != 1:
            raise SystemExit(f"anchor occurs {n} times in {path}: {old!r}")
        with open(full, "w") as f:
            f.write(text.replace(old, new))


def restore(root, edits):
    files = sorted({path for path, _, _ in edits})
    subprocess.run(["git", "checkout", "--", *files], cwd=root, check=True)


def main():
    if len(sys.argv) < 2:
        raise SystemExit(__doc__)
    root = os.path.abspath(sys.argv[1])
    names = sys.argv[2:] or list(MUTATIONS)
    if "/mut" not in root:
        raise SystemExit("refusing: the checkout path must contain '/mut' (disposable copies only)")
    status = subprocess.run(["git", "status", "--porcelain", "--untracked-files=no"], cwd=root,
                            capture_output=True, text=True, check=True).stdout
    if status.strip():
        raise SystemExit(f"refusing: {root} has local changes:\n{status}")
    env = dict(os.environ)
    env.setdefault("CARGO_TARGET_DIR", os.path.join(root, "target"))
    env.setdefault("CARGO_BUILD_JOBS", "6")
    log_path = os.path.join(root, "m4_mutations.log")
    rows = []
    with open(log_path, "w") as log:
        for name in names:
            descr, edits, cmds = MUTATIONS[name]
            log.write(f"\n===== {name}: baseline\n")
            base_ok, base = run_tests(root, cmds, env, log)
            log.write(f"\n===== {name}: mutant\n")
            apply(root, edits)
            try:
                mut_ok, mut = run_tests(root, cmds, env, log)
            finally:
                restore(root, edits)
            log.write(f"\n===== {name}: restored\n")
            rest_ok, rest = run_tests(root, cmds, env, log)
            verdict = "killed" if base_ok and not mut_ok and rest_ok else (
                "SURVIVED" if mut_ok else "INVALID (baseline/restore failed)")
            rows.append((name, descr, base, mut, rest, verdict))
            print(f"{name}: {verdict}\n  baseline: {base}\n  mutant:   {mut}\n  restored: {rest}",
                  flush=True)
    print(f"\nlog: {log_path}")
    print("\n| mutation | baseline | mutant | restored | verdict |\n|---|---|---|---|---|")
    for name, descr, base, mut, rest, verdict in rows:
        print(f"| `{name}`: {descr} | {base} | {mut} | {rest} | {verdict} |")
    return 0 if all(r[5] == "killed" for r in rows) else 1


if __name__ == "__main__":
    sys.exit(main())
