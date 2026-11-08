"""Expanded-size comparison for the H03 costs: the macro expansion of each API crate of the
`benches/scale` suites, per suite (one per arm).

    python3 -I tools/desirability/h03/expanded.py OUT SUITE...

For each suite and API crate (scale50, scale200, scale1000, scale_deep, scale_shared, scale_typed,
scale_fanout): `cargo +1.99.1 rustc --lib -p <crate> -- -Zunpretty=expanded` with
`RUSTC_BOOTSTRAP=1` (the pinned toolchain, not a nightly), in a target directory of its own
(`<WS>/work/targets/base/h03-expand/<suite>`, so the timing suites' targets are untouched), one
cargo command at a time. Writes `OUT/<suite>.<crate>.expanded.rs` and `OUT/sizes.json`
({suite: {crate: {bytes, lines, sha256}}}). The benches/scale directory has no README at the
measured commits; this follows the H08 lane's comparison (expanded output of the suite's API crates).
"""
import hashlib
import json
import os
import subprocess
import sys

WS = "/home/canarit/projects/Concord_Desirability_Loop"
CRATES = ["scale50", "scale200", "scale1000", "scale_deep", "scale_shared", "scale_typed", "scale_fanout"]


def main():
    out, suites = os.path.abspath(sys.argv[1]), [os.path.abspath(s) for s in sys.argv[2:]]
    os.makedirs(out, exist_ok=True)
    sizes = {}
    for s in suites:
        tag = os.path.basename(s)
        env = dict(os.environ, RUSTC_BOOTSTRAP="1", CARGO_TARGET_DIR=os.path.join(WS, "work", "targets", "base", "h03-expand", tag))
        env.setdefault("CARGO_BUILD_JOBS", "6")
        for c in CRATES:
            path = os.path.join(out, f"{tag}.{c}.expanded.rs")
            with open(path, "wb") as fo:
                r = subprocess.run(["cargo", "+1.99.1", "rustc", "-q", "--offline", "--locked", "--manifest-path",
                                    os.path.join(s, "Cargo.toml"), "-p", c, "--lib", "--", "-Zunpretty=expanded"],
                                   stdout=fo, stderr=subprocess.PIPE, env=env)
            if r.returncode:
                raise SystemExit(f"{tag} {c}: {r.stderr.decode()[-800:]}")
            data = open(path, "rb").read()
            sizes.setdefault(tag, {})[c] = {"bytes": len(data), "lines": data.count(b"\n"),
                                            "sha256": hashlib.sha256(data).hexdigest()}
            print(tag, c, len(data), flush=True)
    json.dump(sizes, open(os.path.join(out, "sizes.json"), "w"), indent=1, sort_keys=True)


if __name__ == "__main__":
    main()
