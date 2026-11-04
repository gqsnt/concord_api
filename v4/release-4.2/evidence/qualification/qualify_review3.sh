#!/usr/bin/env bash
set -uo pipefail
cd /home/canarit/projects/concord-v42/candidate
out=/home/canarit/projects/concord-v42/records/mutation; H=/home/canarit/projects/concord-v42/tools/heavy.sh
syn=crates/concord-macros/src/syntax.rs; cp $syn $out/syntax.rs.r3
python3 -I - "$syn" <<'PY'
import sys; p=sys.argv[1]; s=open(p).read()
old="""                let grouped = if expr {
                    g.stream().into_iter().nth(1).is_none()
                } else {
                    bare_fragment(&g)
                };"""
assert s.count(old)==1; open(p,'w').write(s.replace(old,"                let grouped = bare_fragment(&g);"))
PY
echo "== R8-expr-context" > $out/R8-expr-context.log
timeout 1200 $H cargo +1.99.1 test --locked -p concord --test forwarded_types >> $out/R8-expr-context.log 2>&1; echo "exit=$?" >> $out/R8-expr-context.log
cp $out/syntax.rs.r3 $syn; cmp -s $syn $out/syntax.rs.r3 && echo restored-bytes-verified >> $out/R8-expr-context.log
tail -2 $out/R8-expr-context.log; grep -m3 -E "left:|right:|error\[" $out/R8-expr-context.log
