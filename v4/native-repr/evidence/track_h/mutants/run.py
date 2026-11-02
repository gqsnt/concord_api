#!/usr/bin/env python3
"""Mutation checks for the native-header prototype: each mutant re-introduces a defect; the
`native_headers` suite must fail on it, then pass again once the source is restored.
Usage: python3 exp/mutants/run.py   (from the repository root; prints one line per mutant)"""
import subprocess, sys, pathlib

MUTANTS = [
    ("clear-native-flag (old `set_sensitive(declaration)`)", "crates/concord/src/__private.rs",
     "        if sensitive {\n            value.set_sensitive(true);\n        }\n        self.headers\n            .try_append(name, value)",
     "        value.set_sensitive(sensitive);\n        self.headers\n            .try_append(name, value)"),
    ("owner-Debug ignores native flag", "crates/concord/src/__private.rs",
     "            Some(value) if value.is_sensitive() => f.write_str(\"<redacted>\"),\n", ""),
    ("text policy relaxed to http's (obs-text accepted)", "crates/concord/src/header.rs",
     "        if visible {\n            HeaderValue::from_str(self).ok()", "        if visible || true {\n            HeaderValue::from_str(self).ok()"),
    ("literal not lowered to a static", "crates/concord-macros/src/emit.rs",
     "            if args.is_empty() && id.family == Family::Header {", "            if args.is_empty() && id.family == Family::Header && false {"),
    ("Prepared Debug prints sensitive values", "crates/concord/src/prepared.rs",
     "            if value.is_sensitive() {\n                list.entry(&format_args!(\"{name}: <redacted>\"));\n            } else if",
     "            if false {\n            } else if"),
]

def run():
    r = subprocess.run(["cargo", "test", "-p", "concord", "--test", "native_headers", "--locked", "--offline"],
                       capture_output=True, text=True)
    failed = [l.split()[1] for l in r.stdout.splitlines() if l.startswith("test ") and l.endswith("FAILED")]
    return r.returncode, failed

code, _ = run()
print(f"baseline: {'pass' if code == 0 else 'FAIL'}")
status = 0 if code == 0 else 1
for name, path, old, new in MUTANTS:
    p = pathlib.Path(path); src = p.read_text()
    assert src.count(old) == 1, f"{name}: anchor not found once"
    p.write_text(src.replace(old, new))
    try:
        code, failed = run()
    finally:
        p.write_text(src)
    verdict = "killed" if code != 0 else "SURVIVED"
    if code == 0: status = 1
    print(f"{verdict:9} {name}: {', '.join(failed) or ('(compile error)' if code else '-')}")
code, _ = run()
print(f"restored: {'pass' if code == 0 else 'FAIL'}")
sys.exit(status if code == 0 else 1)
