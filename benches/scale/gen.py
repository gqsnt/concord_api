"""Generates a crate with one api! declaration of N endpoints for compile-cost measurement.

usage: python3 gen.py OUT_DIR N CONCORD_PATH

Shape: N/10 scopes under the client, each with 10 endpoints; every scope has one path value and
one optional header; every endpoint has one path value, two optional query values and one
defaulted query value. Mirrors a typical REST surface.
"""
import os
import sys

out, n, concord = sys.argv[1], int(sys.argv[2]), os.path.abspath(sys.argv[3])
os.makedirs(os.path.join(out, "src"), exist_ok=True)
open(os.path.join(out, "Cargo.toml"), "w").write(f"""[package]
name = "scale{n}"
version = "0.0.0"
edition = "2024"
publish = false

[dependencies]
concord = {{ path = "{concord}", default-features = false }}

[workspace]
""")
lines = ["#![allow(missing_docs)]", "concord::api! {", "    pub client Scale(#[sensitive] header(\"x-api-key\") key: String) {",
         "        base \"https://api.example.com/v1\";"]
scopes = max(1, n // 10)
for s in range(scopes):
    lines.append(f"        scope s{s}(owner: String, header(\"x-trace\") trace?: String) {{")
    lines.append(f"            path \"s{s}/{{owner}}\";")
    for e in range(n // scopes):
        lines.append(f"            GET e{e}(id: u64, query q?: String, query sort?: String, query limit: u32 = 20) {{ path \"items/{{id}}\"; }}")
    lines.append("        }")
lines += ["    }", "}"]
open(os.path.join(out, "src", "lib.rs"), "w").write("\n".join(lines) + "\n")
